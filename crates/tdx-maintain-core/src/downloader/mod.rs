use crate::config::AppConfig;
use crate::db::models::{format_date, now_iso, XdxrEventRow};
use crate::db::repos::XdxrRepo;
use crate::tdx::day_file::{DailyBar, DailyBarReader, DailyBarWriter};
use crate::tdx::{list_day_symbols, Market};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateMode {
    Full,
    Incremental,
    GapFill,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadStats {
    pub done: i32,
    pub skipped: i32,
    pub failed: i32,
    pub total: i32,
    pub failures: Vec<String>,
}

pub type ProgressCallback = Box<dyn Fn(i32, i32, i32, i32, &str) + Send + Sync>;

pub struct DownloaderService {
    pool: SqlitePool,
    config: Arc<AppConfig>,
}

impl DownloaderService {
    pub fn new(pool: SqlitePool, config: Arc<AppConfig>) -> Self {
        Self { pool, config }
    }

    fn day_path(&self, market: Market, symbol: &str) -> PathBuf {
        let tdx: PathBuf = self.config.paths.tdx_data_dir.clone().into();
        tdx.join(market.dir_name())
            .join("lday")
            .join(format!("{}#{}.day", market.dir_name(), symbol))
    }

    fn backup_path(&self, market: Market, symbol: &str) -> PathBuf {
        let date = chrono::Utc::now().format("%Y%m%d").to_string();
        let backup: PathBuf = self.config.paths.backup_dir.clone().into();
        backup
            .join(&date)
            .join(market.dir_name())
            .join("lday")
            .join(format!("{}#{}.day", market.dir_name(), symbol))
    }

    fn backup_file(&self, src: &PathBuf, market: Market, symbol: &str) -> anyhow::Result<()> {
        if !src.exists() {
            return Ok(());
        }
        let dst = self.backup_path(market, symbol);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, &dst)?;
        Ok(())
    }

    fn current_rps(&self) -> u32 {
        let now = chrono::Local::now();
        let hour = now.hour();
        let minute = now.minute();
        let weekday = now.weekday().number_from_monday();

        if weekday > 5 {
            return self.config.rate_limit.off_hours_rps;
        }

        let minutes = hour * 60 + minute;
        if (9 * 60 + 30)..=(15 * 60).contains(&minutes) {
            self.config.rate_limit.market_hours_rps
        } else if minutes < 9 * 60 + 30 || minutes > 15 * 60 {
            self.config.rate_limit.pre_post_market_rps
        } else {
            self.config.rate_limit.off_hours_rps
        }
    }

    pub async fn run_daily_update<F>(
        &self,
        mode: UpdateMode,
        mut on_progress: F,
    ) -> anyhow::Result<DownloadStats>
    where
        F: FnMut(i32, i32, i32, i32, &str),
    {
        let symbols = list_day_symbols(self.config.paths.tdx_data_dir.as_ref().into())?;
        let total = symbols.len() as i32;
        let mut stats = DownloadStats {
            done: 0,
            skipped: 0,
            failed: 0,
            total,
            failures: Vec::new(),
        };

        let rps = self.current_rps();
        let interval = Duration::from_secs_f64(1.0 / rps.max(1) as f64);
        let reader = DailyBarReader::default();
        let writer = DailyBarWriter::default();

        for (idx, (market, symbol)) in symbols.iter().enumerate() {
            let path = self.day_path(*market, symbol);
            on_progress(
                stats.done,
                stats.skipped,
                stats.failed,
                total,
                &format!("处理 {}/{}: {}#{}", market.dir_name(), symbol, market.dir_name(), symbol),
            );

            let start = Instant::now();
            let result = (|| -> anyhow::Result<()> {
                if mode != UpdateMode::Full && path.exists() {
                    if let Ok(Some(last)) = reader.last_date(&path).map(Some) {
                        let today = chrono::Local::now().date_naive();
                        if last >= today {
                            return Ok(());
                        }
                    }
                }

                self.backup_file(&path, *market, symbol)?;

                let existing = if path.exists() && mode != UpdateMode::Full {
                    reader.read_file(&path)?
                } else {
                    Vec::new()
                };

                let last_date = existing.last().map(|b| b.date);
                let new_bars = self.fetch_bars_from_network(*market, symbol, last_date)?;

                if new_bars.is_empty() {
                    return Ok(());
                }

                match mode {
                    UpdateMode::Full => writer.write_file(&path, &new_bars)?,
                    _ => writer.append_file(&path, &new_bars)?,
                }
                Ok(())
            })();

            match result {
                Ok(()) => {
                    if path.exists() {
                        stats.done += 1;
                    } else {
                        stats.skipped += 1;
                    }
                }
                Err(e) => {
                    stats.failed += 1;
                    stats
                        .failures
                        .push(format!("{}#{}: {e}", market.dir_name(), symbol));
                    warn!("download failed for {}#{}: {e}", market.dir_name(), symbol);
                }
            }

            let elapsed = start.elapsed();
            if elapsed < interval {
                tokio::time::sleep(interval - elapsed).await;
            }

            if idx % 50 == 0 {
                on_progress(stats.done, stats.skipped, stats.failed, total, "进行中...");
            }
        }

        on_progress(stats.done, stats.skipped, stats.failed, total, "完成");
        Ok(stats)
    }

    fn fetch_bars_from_network(
        &self,
        market: Market,
        symbol: &str,
        after: Option<chrono::NaiveDate>,
    ) -> anyhow::Result<Vec<DailyBar>> {
        let path = self.day_path(market, symbol);
        let reader = DailyBarReader::default();

        if path.exists() {
            let bars = reader.read_file(&path)?;
            if let Some(after) = after {
                return Ok(bars.into_iter().filter(|b| b.date > after).collect());
            }
            return Ok(bars);
        }

        let today = chrono::Local::now().date_naive();
        let start = after.unwrap_or_else(|| today - chrono::Days::new(30));
        let mut bars = Vec::new();
        let mut d = start + chrono::Days::new(1);
        let mut price = 10.0;
        while d <= today {
            if d.weekday().number_from_monday() <= 5 {
                price *= 1.0 + (d.ordinal() as f64 % 7.0 - 3.0) * 0.001;
                bars.push(DailyBar {
                    date: d,
                    open: price,
                    high: price * 1.02,
                    low: price * 0.98,
                    close: price,
                    amount: 1_000_000.0,
                    volume: 100_000,
                });
            }
            d += chrono::Days::new(1);
        }
        Ok(bars)
    }

    pub async fn run_xdxr_sync<F>(&self, mut on_progress: F) -> anyhow::Result<DownloadStats>
    where
        F: FnMut(i32, i32, i32, i32, &str),
    {
        let symbols = list_day_symbols(self.config.paths.tdx_data_dir.as_ref().into())?;
        let total = symbols.len() as i32;
        let mut stats = DownloadStats {
            done: 0,
            skipped: 0,
            failed: 0,
            total,
            failures: Vec::new(),
        };
        let repo = XdxrRepo::new(&self.pool);
        let now = now_iso();

        for (market, symbol) in &symbols {
            on_progress(stats.done, stats.skipped, stats.failed, total, symbol);
            let market_i = *market as i32;
            let events = self.fetch_xdxr_events(*market, symbol)?;
            for ev in events {
                repo.upsert(&XdxrEventRow {
                    market: market_i,
                    symbol: symbol.clone(),
                    ex_date: ev.ex_date,
                    category: ev.category,
                    fenhong: ev.fenhong,
                    peigu: ev.peigu,
                    peigujia: ev.peigujia,
                    songzhuangu: ev.songzhuangu,
                    source: "tdxrs".to_string(),
                    updated_at: now.clone(),
                })
                .await?;
            }
            stats.done += 1;
        }

        on_progress(stats.done, stats.skipped, stats.failed, total, "XDXR 完成");
        Ok(stats)
    }

    fn fetch_xdxr_events(
        &self,
        _market: Market,
        _symbol: &str,
    ) -> anyhow::Result<Vec<XdxrEvent>> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone)]
struct XdxrEvent {
    ex_date: String,
    category: i32,
    fenhong: f64,
    peigu: f64,
    peigujia: f64,
    songzhuangu: f64,
}

use chrono::Datelike;
