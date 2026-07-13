use crate::config::AppConfig;

use crate::tdx::{list_day_symbols, Market};
use chrono::{Datelike, Timelike};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::info;

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
        on_progress(0, 0, 0, 100, "开始启动外部日线同步下载器...");

        // 获取本地总股票数量作为进度分母
        let symbols = list_day_symbols(std::path::Path::new(&self.config.paths.tdx_data_dir))?;
        let total = symbols.len() as i32;

        let script_path = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("crates/tdx-maintain-core/src/downloader/download_data.py");

        let mode_str = match mode {
            UpdateMode::Full => "full",
            _ => "incremental",
        };

        let rps = self.current_rps();

        info!("Starting python daily download process with mode: {}, rps: {}", mode_str, rps);

        let mut child = tokio::process::Command::new("python")
            .env("PYTHONIOENCODING", "utf-8")
            .arg(&script_path)
            .arg("--tdx-dir")
            .arg(&self.config.paths.tdx_data_dir)
            .arg("--mode")
            .arg(mode_str)
            .arg("--rate-limit")
            .arg(rps.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("failed to capture stdout"))?;
        let mut reader = BufReader::new(stdout);

        let mut stats = DownloadStats {
            done: 0,
            skipped: 0,
            failed: 0,
            total,
            failures: Vec::new(),
        };

        let mut buf = Vec::new();
        loop {
            buf.clear();
            let n = reader.read_until(b'\n', &mut buf).await?;
            if n == 0 {
                break;
            }
            let line_decoded = String::from_utf8_lossy(&buf);
            let line = line_decoded.trim_end();

            if line.starts_with("PROGRESS:") {
                let json_part = &line["PROGRESS:".len()..];
                if let Ok(prog) = serde_json::from_str::<serde_json::Value>(json_part) {
                    let done = prog.get("done").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    let skipped = prog.get("skipped").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    let failed = prog.get("failed").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    
                    stats.done = done;
                    stats.skipped = skipped;
                    stats.failed = failed;

                    let msg = format!("正在拉取更新... 已同步: {}, 跳过: {}, 失败: {}", done, skipped, failed);
                    on_progress(done, skipped, failed, total, &msg);
                }
            } else if line.starts_with("COMPLETED:") {
                let json_part = &line["COMPLETED:".len()..];
                if let Ok(res) = serde_json::from_str::<serde_json::Value>(json_part) {
                    let done = res.get("done").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    let skipped = res.get("skipped").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    let failed = res.get("failed").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    
                    stats.done = done;
                    stats.skipped = skipped;
                    stats.failed = failed;
                    
                    if let Some(err_msg) = res.get("error").and_then(|v| v.as_str()) {
                        if !err_msg.is_empty() {
                            stats.failures.push(err_msg.to_string());
                        }
                    }
                }
            } else if line.starts_with("INFO:") || line.starts_with("ERROR:") {
                on_progress(stats.done, stats.skipped, stats.failed, total, line);
            }
        }

        let status = child.wait().await?;
        if !status.success() {
            anyhow::bail!("tdxrs download process exited with error. Please check server console logs for details.");
        }

        on_progress(stats.done, stats.skipped, stats.failed, total, "数据增量同步完成");
        Ok(stats)
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
            .env("PYTHONIOENCODING", "utf-8")
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

