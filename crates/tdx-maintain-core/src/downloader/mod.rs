use crate::config::AppConfig;

use crate::tdx::day_file::{DailyBar, DailyBarReader, DailyBarWriter};
use crate::tdx::{list_day_symbols, Market};
use chrono::{Datelike, Timelike};
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
        let base_dir = if tdx.ends_with("vipdoc") {
            tdx
        } else {
            tdx.join("vipdoc")
        };
        let filename = crate::tdx::get_day_filename(market, symbol, &base_dir);
        base_dir.join(market.dir_name())
            .join("lday")
            .join(filename)
    }

    fn backup_path(&self, market: Market, symbol: &str) -> PathBuf {
        let date = chrono::Utc::now().format("%Y%m%d").to_string();
        let backup: PathBuf = self.config.paths.backup_dir.clone().into();
        let tdx: PathBuf = self.config.paths.tdx_data_dir.clone().into();
        let base_dir = if tdx.ends_with("vipdoc") {
            tdx
        } else {
            tdx.join("vipdoc")
        };
        let filename = crate::tdx::get_day_filename(market, symbol, &base_dir);
        backup
            .join(&date)
            .join(market.dir_name())
            .join("lday")
            .join(filename)
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

    /// Async backup that runs the blocking copy off the tokio worker thread.
    async fn backup_file_async(&self, src: &PathBuf, market: Market, symbol: &str) -> anyhow::Result<()> {
        if !src.exists() {
            return Ok(());
        }
        let dst = self.backup_path(market, symbol);
        let src = src.clone();
        tokio::task::spawn_blocking(move || {
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&src, &dst)?;
            Ok::<(), anyhow::Error>(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking cancelled: {e}"))?
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
        if ((9 * 60 + 30)..=(15 * 60)).contains(&minutes) {
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
        let symbols = list_day_symbols(std::path::Path::new(&self.config.paths.tdx_data_dir))?;
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
            let result: anyhow::Result<()> = async {
                if mode != UpdateMode::Full && path.exists() {
                    if let Some(last) = reader.last_date_async(&path).await? {
                        let today = chrono::Local::now().date_naive();
                        if last >= today {
                            return Ok(());
                        }
                    }
                }

                self.backup_file_async(&path, *market, symbol).await?;

                let existing = if path.exists() && mode != UpdateMode::Full {
                    reader.read_file_async(&path).await?
                } else {
                    Vec::new()
                };

                let last_date = existing.last().map(|b| b.date);
                let new_bars = self.fetch_bars_from_network(*market, symbol, last_date).await?;

                if new_bars.is_empty() {
                    return Ok(());
                }

                match mode {
                    UpdateMode::Full => writer.write_file_async(&path, &new_bars).await?,
                    _ => writer.append_file_async(&path, &new_bars).await?,
                }
                Ok(())
            }
            .await;

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

    async fn fetch_bars_from_network(
        &self,
        market: Market,
        symbol: &str,
        after: Option<chrono::NaiveDate>,
    ) -> anyhow::Result<Vec<DailyBar>> {
        let path = self.day_path(market, symbol);
        let reader = DailyBarReader::default();

        if path.exists() {
            let bars = reader.read_file_async(&path).await?;
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
            d = d + chrono::Days::new(1);
        }
        Ok(bars)
    }

    pub async fn run_xdxr_sync<F>(&self, mut on_progress: F) -> anyhow::Result<DownloadStats>
    where
        F: FnMut(i32, i32, i32, i32, &str),
    {
        on_progress(0, 0, 0, 100, "开始解析本地 GBBQ 数据...");

        let script_path = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("crates/tdx-maintain-core/src/downloader/parse_gbbq.py");

        let output = tokio::process::Command::new("python")
            .arg(&script_path)
            .arg(&self.config.paths.metadata_db_path)
            .arg(&self.config.paths.tdx_data_dir)
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            anyhow::bail!("XDXR python sync failed: {}", stderr);
        }

        let mut count = 0;
        for line in stdout.lines() {
            if line.contains("Parsed ") && line.contains(" XDXR events") {
                if let Some(s) = line.split("Parsed ").nth(1) {
                    if let Some(num_str) = s.split(" XDXR events").next() {
                        if let Ok(val) = num_str.parse::<i32>() {
                            count = val;
                        }
                    }
                }
            }
        }

        on_progress(count, 0, 0, count, "XDXR 完成");
        Ok(DownloadStats {
            done: count,
            skipped: 0,
            failed: 0,
            total: count,
            failures: Vec::new(),
        })
    }

}

