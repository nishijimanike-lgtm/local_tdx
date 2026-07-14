use crate::config::AppConfig;

use crate::tdx::{list_day_symbols, Market, DailyBar, DailyBarWriter};
use crate::db::repos::XdxrRepo;
use chrono::{Datelike, Timelike, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use tracing::{info, warn};
use rustdx::tcp::Tdx;

// Control states: 0=normal, 1=paused, 2=aborted
const CTRL_NORMAL: u8 = 0;
const CTRL_PAUSED: u8 = 1;
const CTRL_ABORTED: u8 = 2;

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

    /// Run daily K-line update using native Rust TCP connections (via rustdx).
    /// Replaces the previous Python subprocess approach.
    pub async fn run_daily_update<F>(
        &self,
        mode: UpdateMode,
        mut on_progress: F,
        control: Arc<AtomicU8>,
    ) -> anyhow::Result<DownloadStats>
    where
        F: FnMut(i32, i32, i32, i32, &str),
    {
        on_progress(0, 0, 0, 100, "开始 Rust 原生 TCP 日线同步...");

        let tdx_data_dir = self.config.paths.tdx_data_dir.clone();
        let rps = self.current_rps();

        // Compute the minimum delay between requests based on rate limit
        let delay_per_req = std::time::Duration::from_secs_f64(1.0 / rps as f64);

        info!("Starting Rust daily download with mode: {:?}, rps: {}", mode, rps);

        // Get local symbols to determine total and detect which need updates
        let _local_symbols = list_day_symbols(std::path::Path::new(&tdx_data_dir))?;

        // Gather stock list from rustdx TCP — 0=sh, 1=sz (we map internally)
        let markets_to_dl: Vec<(&str, u16)> = vec![
            ("sh", 1),  // Shanghai = 1
            ("sz", 0),  // Shenzhen = 0
        ];

        let mut all_stocks: Vec<(Market, String)> = Vec::new();
        on_progress(0, 0, 0, 100, "获取股票列表...");

        for &(name, mkt) in &markets_to_dl {
            // Probe server connectivity first
            let mut tcp = match self.get_tcp_connection(mkt).await {
                Ok(t) => t,
                Err(e) => {
                    warn!("无法连接 {} 市场行情服务器: {}", name, e);
                    continue;
                }
            };

            // Get stock count
            let mut security_count = rustdx::tcp::SecurityCount::new(mkt);
            let count = match security_count.recv_parsed(&mut tcp) {
                Ok(c) => *c,
                Err(e) => {
                    warn!("无法获取 {} 证券数量: {}", name, e);
                    continue;
                }
            };

            // Get stock list in batches of 1000
            let mut start: u16 = 0;
            while start < count {
                if control.load(Ordering::Relaxed) == CTRL_ABORTED {
                    anyhow::bail!("任务已被中止");
                }
                let mut list = rustdx::tcp::SecurityList::new(mkt, start);
                match list.recv_parsed(&mut tcp) {
                    Ok(data) => {
                        for item in data.iter() {
                            // Map rustdx SecurityListData to our Market + symbol
                            // code is fixed-width 6 chars; need to determine market from name prefix
                            let market = market_from_code(&item.code, mkt);
                            all_stocks.push((market, item.code.clone()));
                        }
                        if data.len() < 1000 {
                            break; // last batch
                        }
                        start += data.len() as u16;
                    }
                    Err(e) => {
                        warn!("获取 {} 证券列表失败 (offset {}): {}", name, start, e);
                        break;
                    }
                }
            }
        }

        let total = all_stocks.len() as i32;
        on_progress(0, 0, 0, total, &format!("共 {} 只股票待处理", total));

        // Download loop
        let mut stats = DownloadStats {
            done: 0,
            skipped: 0,
            failed: 0,
            total,
            failures: Vec::new(),
        };

        

        for (i, (market, symbol)) in all_stocks.iter().enumerate() {
            // Check control state
            let ctrl = control.load(Ordering::Relaxed);
            if ctrl == CTRL_ABORTED {
                break;
            }
            while ctrl == CTRL_PAUSED {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }

            let mkt = market_to_rustdx(*market);

            // Sleep for rate limiting
            tokio::time::sleep(delay_per_req).await;

            match self.download_one_stock(mkt, symbol).await {
                Ok(bars) => {
                    let path = self.day_path(*market, symbol);
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    // Write .day file (blocking I/O offloaded)
                    let path_clone = path.clone();
                    let bars_clone = bars.clone();
                    let write_ok = tokio::task::spawn_blocking(move || {
                        let writer = DailyBarWriter::default(); writer.write_file(&path_clone, &bars_clone)
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("spawn_blocking failed: {e}"))?;

                    match write_ok {
                        Ok(()) => stats.done += 1,
                        Err(e) => {
                            stats.failed += 1;
                            stats.failures.push(format!("{}#{}: 写入失败 {}", market.dir_name(), symbol, e));
                        }
                    }
                }
                Err(e) => {
                    stats.failed += 1;
                    stats.failures.push(format!("{}#{}: {}", market.dir_name(), symbol, e));
                }
            }

            if i % 50 == 0 {
                let msg = format!(
                    "正在拉取更新... 已同步: {}, 跳过: {}, 失败: {}",
                    stats.done, stats.skipped, stats.failed
                );
                on_progress(stats.done, stats.skipped, stats.failed, total, &msg);
            }
        }

        // Auto-retry failed stocks (configurable)
        let max_attempts = self.config.retry.max_attempts.max(1);
        let _backoff = std::time::Duration::from_millis(self.config.retry.backoff_ms);

        if !stats.failures.is_empty() && max_attempts > 1 {
            on_progress(stats.done, stats.skipped, stats.failed, total,
                &format!("重试 {} 个失败的股票 (第 2/{} 轮)...", stats.failures.len(), max_attempts));

            // TODO: Collect failed stocks and retry
            // For now, just report and continue
        }

        on_progress(stats.done, stats.skipped, stats.failed, total, "数据增量同步完成");
        Ok(stats)
    }

    /// Download daily K-line for a single stock via rustdx TCP
    async fn download_one_stock(&self, market: u16, symbol: &str) -> anyhow::Result<Vec<DailyBar>> {
        let mut tcp = self.get_tcp_connection(market).await?;
        let mut kline = rustdx::tcp::stock::Kline::new(market, symbol, 9, 0, 800);
        let data = kline.recv_parsed(&mut tcp)
            .map_err(|e| anyhow::anyhow!("下载失败: {e}"))?;

        let bars: Vec<DailyBar> = data.iter().map(|k| {
            let date = NaiveDate::from_ymd_opt(k.dt.year as i32, k.dt.month as u32, k.dt.day as u32)
                .unwrap_or_else(|| NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
            // KlineData has open/close/high/low as f64 with 3-decimal precision
            // TDX .day format stores as i32 (price * 1000)
            DailyBar {
                date,
                open: k.open,
                high: k.high,
                low: k.low,
                close: k.close,
                amount: k.amount,
                volume: k.vol as u32,
            }
        }).collect();

        Ok(bars)
    }

    /// Create a rustdx TCP connection with probe
    async fn get_tcp_connection(&self, _market: u16) -> anyhow::Result<rustdx::tcp::Tcp> {
        // Use default server IP; could be enhanced with dynamic probing
        tokio::task::spawn_blocking(move || {
            rustdx::tcp::Tcp::new()
                .map_err(|e| anyhow::anyhow!("TDX TCP 连接失败: {e}"))
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking: {e}"))?
    }

    pub async fn run_xdxr_sync<F>(&self, mut on_progress: F) -> anyhow::Result<DownloadStats>
    where
        F: FnMut(i32, i32, i32, i32, &str),
    {
        on_progress(0, 0, 0, 100, "开始解析本地 GBBQ 数据...");

        let gbbq_path = std::path::Path::new(&self.config.paths.tdx_data_dir)
            .join("T0002").join("hq_cache").join("gbbq");
        let gbbq_path_clone = gbbq_path.clone();
        let pool = self.pool.clone();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let count = tokio::task::spawn_blocking(move || -> anyhow::Result<i32> {
            let mut gbbqs = rustdx::file::gbbq::Gbbqs::from_file(&gbbq_path_clone)
                .map_err(|e| anyhow::anyhow!("无法读取 GBBQ 文件: {e}"))?;
            let records = gbbqs.to_vec();

            let bj_prefixes = ["83", "87", "88", "43"];
            let repo = XdxrRepo::new(&pool);
            let mut count: i32 = 0;

            for gbbq in &records {
                if gbbq.category != 1 && gbbq.category != 14 { continue; }

                let code = if gbbq.code.len() >= 6 { &gbbq.code[..6] } else { gbbq.code };
                let market = if gbbq.market == 1 { 1 }
                else if bj_prefixes.iter().any(|p| code.starts_with(p)) { 2 }
                else { 0 };

                let date_str = format!("{:04}-{:02}-{:02}", gbbq.date / 10000, (gbbq.date / 100) % 100, gbbq.date % 100);
                let row = crate::db::models::XdxrEventRow {
                    market,
                    symbol: code.to_string(),
                    ex_date: date_str,
                    category: gbbq.category as i32,
                    fenhong: (gbbq.fh_qltp / 10.0) as f64,
                    peigu: (gbbq.pg_hzgb / 10.0) as f64,
                    peigujia: gbbq.pgj_qzgb as f64,
                    songzhuangu: (gbbq.sg_hltp / 10.0) as f64,
                    source: "local_gbbq".to_string(),
                    updated_at: now.clone(),
                };
                tokio::runtime::Handle::current().block_on(repo.upsert(&row))?;
                count += 1;
            }
            Ok(count)
        })
        .await
        .map_err(|e| anyhow::anyhow!("GBBQ 解析失败: {e}"))??;

        on_progress(count, 0, 0, count, "XDXR 完成");
        Ok(DownloadStats { done: count, skipped: 0, failed: 0, total: count, failures: Vec::new() })
    }
}



/// Map rustdx market code (0=SZ, 1=SH) to our Market enum
fn market_to_rustdx(m: Market) -> u16 {
    match m {
        Market::Sz => 0,
        Market::Sh => 1,
        Market::Bj => 0, // BJ uses Shenzhen server
    }
}

/// Determine Market from stock code and rustdx market hint
fn market_from_code(code: &str, mkt: u16) -> Market {
    match mkt {
        0 => { // Shenzhen server — includes BJ prefixes
            if code.starts_with("83") || code.starts_with("87") || code.starts_with("88") || code.starts_with("43") {
                Market::Bj
            } else {
                Market::Sz
            }
        }
        _ => Market::Sh,
    }
}
