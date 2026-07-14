use crate::config::AppConfig;

use crate::tdx::{list_day_symbols, Market, DailyBar, DailyBarReader, DailyBarWriter};
use crate::db::repos::{XdxrRepo, DownloadCheckpointRepo};
use chrono::{Datelike, Timelike, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use tracing::{info, warn};
use rustdx::tcp::Tdx;

// Control states: 1=paused, 2=aborted (0=normal is implicit default)
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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
        let change_name = "daily-update";
        let checkpoint_repo = DownloadCheckpointRepo::new(&self.pool);

        // Check for existing checkpoint (resume after restart)
        let start_idx = if let Ok(Some(cp)) = checkpoint_repo.load(change_name, "all").await {
            let resume_idx = all_stocks.iter().position(|(_, s)| s == &cp.last_symbol).unwrap_or(0);
            let msg = format!("检测到断点，从 {} 恢复 (进度 {}/{})", cp.last_symbol, cp.progress, total);
            info!("{}", msg);
            on_progress(cp.progress, 0, 0, total, &msg);
            resume_idx + 1
        } else {
            0
        };

        on_progress(start_idx as i32, 0, 0, total, &format!("共 {} 只股票待处理", total));

        let mut stats = DownloadStats { done: start_idx as i32, skipped: 0, failed: 0, total, failures: Vec::new() };
        let mut failed_stocks: Vec<(Market, String)> = Vec::new();

        for i in start_idx..all_stocks.len() {
            let (market, symbol) = &all_stocks[i];

            let ctrl = control.load(Ordering::Relaxed);
            if ctrl == CTRL_ABORTED {
                if i > 0 { let _ = checkpoint_repo.save(change_name, "all", &all_stocks[i-1].1, i as i32, total).await; }
                anyhow::bail!("任务已被用户中止 (断点已保存)");
            }
            while ctrl == CTRL_PAUSED { tokio::time::sleep(std::time::Duration::from_millis(200)).await; }

            let mkt = market_to_rustdx(*market);
            tokio::time::sleep(delay_per_req).await;

            match self.download_one_stock(mkt, symbol).await {
                Ok(bars) => {
                    let path = self.day_path(*market, symbol);
                    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
                    let path_clone = path.clone();
                    let bars_clone = bars.clone();
                    let write_ok = tokio::task::spawn_blocking(move || {
                        DailyBarWriter::default().write_file(&path_clone, &bars_clone)
                    }).await.map_err(|e| anyhow::anyhow!("spawn_blocking failed: {e}"))?;

                    match write_ok {
                        Ok(()) => stats.done += 1,
                        Err(e) => {
                            stats.failed += 1;
                            stats.failures.push(format!("{}#{}: 写入失败 {}", market.dir_name(), symbol, e));
                            failed_stocks.push((*market, symbol.clone()));
                        }
                    }
                }
                Err(e) => {
                    stats.failed += 1;
                    stats.failures.push(format!("{}#{}: {}", market.dir_name(), symbol, e));
                    failed_stocks.push((*market, symbol.clone()));
                }
            }

            if i % 50 == 0 {
                let _ = checkpoint_repo.save(change_name, "all", symbol, i as i32, total).await;
                on_progress(stats.done, stats.skipped, stats.failed, total,
                    &format!("正在拉取更新... 已同步: {}, 跳过: {}, 失败: {}", stats.done, stats.skipped, stats.failed));
            }
        }

        // Clear checkpoint on normal completion
        let _ = checkpoint_repo.clear(change_name).await;

        // Auto-retry failed stocks
        let max_attempts = self.config.retry.max_attempts.max(1);
        let backoff = std::time::Duration::from_millis(self.config.retry.backoff_ms);

        if !failed_stocks.is_empty() && max_attempts > 1 {
            let mut retry_list = failed_stocks.clone();
            for attempt in 2..=max_attempts {
                let mut next_retry: Vec<(Market, String)> = Vec::new();
                on_progress(stats.done, stats.skipped, stats.failed, total,
                    &format!("重试 {} 个失败的股票 (第 {}/{} 轮)...", retry_list.len(), attempt, max_attempts));

                for (market, symbol) in &retry_list {
                    if control.load(Ordering::Relaxed) == CTRL_ABORTED { break; }
                    tokio::time::sleep(backoff).await;
                    let mkt = market_to_rustdx(*market);
                    match self.download_one_stock(mkt, symbol).await {
                        Ok(bars) => {
                            let path = self.day_path(*market, symbol);
                            if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
                            let path_clone = path.clone();
                            let bars_clone = bars.clone();
                            if tokio::task::spawn_blocking(move || DailyBarWriter::default().write_file(&path_clone, &bars_clone))
                                .await.is_ok()
                            {
                                stats.done += 1;
                                stats.failed -= 1;
                                stats.failures.retain(|f| !f.contains(&symbol[..]));

                            } else { next_retry.push((*market, symbol.clone())); }
                        }
                        Err(_) => { next_retry.push((*market, symbol.clone())); }
                    }
                }
                if next_retry.is_empty() { break; }
                retry_list = next_retry;
            }
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

    /// Import local TDX .day files — scan, validate, and report statistics.
    /// No network access; pure local file I/O.
    pub async fn run_local_import<F>(&self, mut on_progress: F) -> anyhow::Result<DownloadStats>
    where
        F: FnMut(i32, i32, i32, i32, &str),
    {
        on_progress(0, 0, 0, 100, "扫描本地 TDX 数据目录...");

        let tdx_dir = self.config.paths.tdx_data_dir.clone();
        let symbols = list_day_symbols(std::path::Path::new(&tdx_dir))?;
        let total = symbols.len() as i32;

        on_progress(0, 0, 0, total, &format!("发现 {} 个本地 .day 文件，开始校验...", total));

        let tdx_dir_clone = tdx_dir.clone();
        let stats = tokio::task::spawn_blocking(move || -> anyhow::Result<DownloadStats> {
            let reader = DailyBarReader::default();
            let mut done = 0i32;
            let mut skipped = 0i32;
            let mut failed = 0i32;
            let mut failures = Vec::new();
            let mut total_bars: i64 = 0;
            let mut first_date: Option<String> = None;
            let mut last_date: Option<String> = None;

            for (idx, (market, symbol)) in symbols.iter().enumerate() {
                let filename = crate::tdx::get_day_filename(*market, symbol,
                    &std::path::Path::new(&tdx_dir_clone));
                let base_dir = if tdx_dir_clone.ends_with("vipdoc") {
                    std::path::PathBuf::from(&tdx_dir_clone)
                } else {
                    std::path::PathBuf::from(&tdx_dir_clone).join("vipdoc")
                };
                let path = base_dir.join(market.dir_name()).join("lday").join(&filename);

                if !path.exists() {
                    skipped += 1;
                    continue;
                }

                match reader.read_file(&path) {
                    Ok(bars) => {
                        if bars.is_empty() {
                            skipped += 1;
                            continue;
                        }
                        total_bars += bars.len() as i64;
                        if let Some(d) = bars.first().map(|b| b.date.format("%Y-%m-%d").to_string()) {
                            if first_date.as_ref().map_or(true, |fd| d < *fd) { first_date = Some(d); }
                        }
                        if let Some(d) = bars.last().map(|b| b.date.format("%Y-%m-%d").to_string()) {
                            if last_date.as_ref().map_or(true, |ld| d > *ld) { last_date = Some(d); }
                        }
                        done += 1;
                    }
                    Err(e) => {
                        failed += 1;
                        failures.push(format!("{}#{}: {}", market.dir_name(), symbol, e));
                    }
                }

                if idx % 200 == 0 {
                    // Progress reported via stats; caller polls
                }
            }

            Ok(DownloadStats {
                done, skipped, failed, total, failures,
            })
        })
        .await
        .map_err(|e| anyhow::anyhow!("本地导入失败: {e}"))??;

        on_progress(stats.done, stats.skipped, stats.failed, total,
            &format!("本地数据校验完成: {} 有效, {} 跳过, {} 失败", stats.done, stats.skipped, stats.failed));
        Ok(stats)
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
