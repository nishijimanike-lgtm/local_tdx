use crate::adj_factor::{AdjFactorService, AdjFactorTier};
use crate::alert::AlertEngine;
use crate::calendar::CalendarService;
use crate::config::AppConfig;
use crate::db::repos::{SyncMetaRepo, TaskLogRepo};
use crate::downloader::DownloaderService;
use crate::scanner::ScannerService;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::{atomic::{AtomicU8, Ordering}, Arc};
use tokio::sync::{broadcast, Mutex};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    pub task_id: i64,
    pub task_type: String,
    pub done: i32,
    pub skipped: i32,
    pub failed: i32,
    pub total: i32,
    pub message: String,
    pub finished: bool,
    pub paused: bool,
    pub aborted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    CalendarSync,
    DailyFull,
    DailyIncrement,
    DailyGapFill,
    XdxrSync,
    AdjFactorSync,
    DailyBarScan,
    XdxrScan,
    AdjFactorScan,
}

impl TaskKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskKind::CalendarSync => "calendar_sync",
            TaskKind::DailyFull => "daily_bar_full",
            TaskKind::DailyIncrement => "daily_bar_update",
            TaskKind::DailyGapFill => "daily_bar_gap_fill",
            TaskKind::XdxrSync => "xdxr_sync",
            TaskKind::AdjFactorSync => "adj_factor_update",
            TaskKind::DailyBarScan => "daily_bar_scan",
            TaskKind::XdxrScan => "xdxr_scan",
            TaskKind::AdjFactorScan => "adj_factor_scan",
        }
    }
}

const CTRL_NORMAL: u8 = 0;
const CTRL_PAUSED: u8 = 1;
const CTRL_ABORTED: u8 = 2;

pub struct ActiveTask {
    pub task_id: i64,
    pub kind: TaskKind,
    pub control: Arc<AtomicU8>,
    pub paused: bool,
    pub aborted: bool,
    pub done: i32,
    pub skipped: i32,
    pub failed: i32,
    pub total: i32,
    pub message: String,
}

pub struct TaskQueue {
    pool: SqlitePool,
    config: Arc<AppConfig>,
    alerts: Arc<AlertEngine>,
    running: Arc<Mutex<bool>>,
    pub active_task: Arc<Mutex<Option<ActiveTask>>>,
    progress_tx: broadcast::Sender<TaskProgress>,
}

impl TaskQueue {
    pub fn new(pool: SqlitePool, config: Arc<AppConfig>, alerts: Arc<AlertEngine>) -> Self {
        let (progress_tx, _) = broadcast::channel(64);
        Self {
            pool,
            config,
            alerts,
            running: Arc::new(Mutex::new(false)),
            active_task: Arc::new(Mutex::new(None)),
            progress_tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TaskProgress> {
        self.progress_tx.subscribe()
    }

    pub async fn is_running(&self) -> bool {
        *self.running.lock().await
    }

    #[allow(dead_code)]
    fn emit(&self, progress: TaskProgress) {
        let _ = self.progress_tx.send(progress);
    }

    pub async fn pause(&self) -> anyhow::Result<()> {
        let guard = self.active_task.lock().await;
        if let Some(ref task) = *guard {
            if task.paused {
                return Ok(());
            }
            task.control.store(CTRL_PAUSED, Ordering::Relaxed);
            let progress = TaskProgress {
                task_id: task.task_id,
                task_type: task.kind.as_str().to_string(),
                done: task.done,
                skipped: task.skipped,
                failed: task.failed,
                total: task.total,
                message: format!("正在拉取更新... 已暂停 (已同步: {}, 跳过: {}, 失败: {})", task.done, task.skipped, task.failed),
                finished: false,
                paused: true,
                aborted: task.aborted,
            };
            let _ = self.progress_tx.send(progress);
            Ok(())
        } else {
            anyhow::bail!("没有运行中的任务可以暂停");
        }
    }

    pub async fn resume(&self) -> anyhow::Result<()> {
        let guard = self.active_task.lock().await;
        if let Some(ref task) = *guard {
            if !task.paused {
                return Ok(());
            }
            task.control.store(CTRL_NORMAL, Ordering::Relaxed);
            let progress = TaskProgress {
                task_id: task.task_id,
                task_type: task.kind.as_str().to_string(),
                done: task.done,
                skipped: task.skipped,
                failed: task.failed,
                total: task.total,
                message: format!("正在拉取更新... 已同步: {}, 跳过: {}, 失败: {}", task.done, task.skipped, task.failed),
                finished: false,
                paused: false,
                aborted: task.aborted,
            };
            let _ = self.progress_tx.send(progress);
            Ok(())
        } else {
            anyhow::bail!("没有已暂停的任务可以恢复");
        }
    }

    pub async fn abort(&self) -> anyhow::Result<()> {
        let guard = self.active_task.lock().await;
        if let Some(ref task) = *guard {
            task.control.store(CTRL_ABORTED, Ordering::Relaxed);
            Ok(())
        } else {
            anyhow::bail!("没有运行中的任务可以中止");
        }
    }

    pub async fn enqueue(&self, kind: TaskKind) -> anyhow::Result<i64> {
        let mut running = self.running.lock().await;
        if *running {
            anyhow::bail!("another task is already running");
        }
        *running = true;

        let task_id = TaskLogRepo::new(&self.pool).create(kind.as_str()).await?;
        let pool = self.pool.clone();
        let config = self.config.clone();
        let alerts = self.alerts.clone();
        let progress_tx = self.progress_tx.clone();
        let running_lock = self.running.clone();
        let active_task = self.active_task.clone();

        tokio::spawn(async move {
            {
                let mut guard = active_task.lock().await;
                *guard = Some(ActiveTask {
                    task_id,
                    kind,
                    control: Arc::new(AtomicU8::new(CTRL_NORMAL)),
                    paused: false,
                    aborted: false,
                    done: 0,
                    skipped: 0,
                    failed: 0,
                    total: 100,
                    message: "正在初始化任务...".to_string(),
                });
            }

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(3600),
                run_task(pool.clone(), config, alerts.clone(), kind, task_id, progress_tx, active_task.clone()),
            )
            .await;

            {
                let mut guard = active_task.lock().await;
                *guard = None;
            }

            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    tracing::error!("task {} failed: {e}", task_id);
                    let _ = alerts
                        .error("task", &format!("任务 {task_id} 失败"), Some(&e.to_string()))
                        .await;
                    let _ = TaskLogRepo::new(&pool)
                        .finish(task_id, "failed", 0, 0, 1, Some(&e.to_string()))
                        .await;
                }
                Err(_elapsed) => {
                    tracing::error!("task {} timed out after 1 hour", task_id);
                    let _ = alerts
                        .error("task", &format!("任务 {task_id} 超时"), Some("任务执行超过1小时，已自动终止"))
                        .await;
                    let _ = TaskLogRepo::new(&pool)
                        .finish(task_id, "timeout", 0, 0, 1, Some("task timed out after 1 hour"))
                        .await;
                }
            }

            let mut guard = running_lock.lock().await;
            *guard = false;
        });

        Ok(task_id)
    }

    pub fn release_running(&self) {
        if let Ok(mut guard) = self.running.try_lock() {
            *guard = false;
        }
    }
}

async fn run_task(
    pool: SqlitePool,
    config: Arc<AppConfig>,
    alerts: Arc<AlertEngine>,
    kind: TaskKind,
    task_id: i64,
    progress_tx: broadcast::Sender<TaskProgress>,
    active_task: Arc<Mutex<Option<ActiveTask>>>,
) -> anyhow::Result<()> {
    let emit = |done: i32, skipped: i32, failed: i32, total: i32, message: &str, finished: bool| {
        let (paused, aborted) = {
            if let Ok(mut guard) = active_task.try_lock() {
                if let Some(ref mut task) = *guard {
                    task.done = done;
                    task.skipped = skipped;
                    task.failed = failed;
                    task.total = total;
                    task.message = message.to_string();
                    (task.paused, task.aborted)
                } else {
                    (false, false)
                }
            } else {
                (false, false)
            }
        };

        let _ = progress_tx.send(TaskProgress {
            task_id,
            task_type: kind.as_str().to_string(),
            done,
            skipped,
            failed,
            total,
            message: message.to_string(),
            finished,
            paused,
            aborted,
        });
    };

    info!("starting task {} ({})", task_id, kind.as_str());

    let _result = match kind {
        TaskKind::CalendarSync => {
            let svc = CalendarService::new(pool.clone(), (*config).clone());
            emit(0, 0, 0, 1, "构建交易日历...", false);
            let count = svc.build_from_index().await?;
            if config.tushare.enabled && !config.tushare.token.is_empty() {
                let ts = crate::tushare::TushareClient::new(
                    &config.tushare.token,
                    &config.tushare.base_url,
                );
                let end = chrono::Utc::now().format("%Y-%m-%d").to_string();
                let _ = svc.sync_from_tushare(&ts, "1990-01-01", &end).await;
            }
            emit(count as i32, 0, 0, count as i32, "交易日历更新完成", true);
            TaskLogRepo::new(&pool)
                .finish(task_id, "success", count as i32, 0, 0, None)
                .await?;
        }
        TaskKind::DailyFull | TaskKind::DailyIncrement | TaskKind::DailyGapFill => {
            let dl = DownloaderService::new(pool.clone(), config.clone());
            let mode = match kind {
                TaskKind::DailyFull => crate::downloader::UpdateMode::Full,
                TaskKind::DailyIncrement => crate::downloader::UpdateMode::Incremental,
                TaskKind::DailyGapFill => crate::downloader::UpdateMode::GapFill,
                _ => unreachable!(),
            };

            let control = {
                let guard = active_task.lock().await;
                guard.as_ref().map(|t| t.control.clone()).unwrap_or_else(|| Arc::new(AtomicU8::new(CTRL_NORMAL)))
            };
            let stats = dl
                .run_daily_update(
                    mode,
                    |done, skipped, failed, total, msg| {
                        emit(done, skipped, failed, total, msg, false);
                    },
                    control,
                )
                .await?;
            let status = if stats.failed > 0 { "partial" } else { "success" };
            emit(stats.done, stats.skipped, stats.failed, stats.total, "日线更新完成", true);
            let detail = serde_json::to_string(&stats)?;
            TaskLogRepo::new(&pool)
                .finish(task_id, status, stats.done, stats.skipped, stats.failed, Some(&detail))
                .await?;
            SyncMetaRepo::new(&pool)
                .set("last_daily_update", &chrono::Utc::now().to_rfc3339())
                .await?;
        }
        TaskKind::XdxrSync => {
            let dl = DownloaderService::new(pool.clone(), config.clone());
            let stats = dl
                .run_xdxr_sync(|done, skipped, failed, total, msg| {
                    emit(done, skipped, failed, total, msg, false);
                })
                .await?;
            emit(stats.done, stats.skipped, stats.failed, stats.total, "XDXR 同步完成", true);
            TaskLogRepo::new(&pool)
                .finish(task_id, "success", stats.done, stats.skipped, stats.failed, None)
                .await?;
        }
        TaskKind::AdjFactorSync => {
            let svc = AdjFactorService::new(pool.clone(), config.clone(), alerts.clone());
            let stats = svc
                .sync(|done, skipped, failed, total, msg| {
                    emit(done, skipped, failed, total, msg, false);
                })
                .await?;
            emit(stats.done, stats.skipped, stats.failed, stats.total, "复权因子更新完成", true);
            TaskLogRepo::new(&pool)
                .finish(task_id, "success", stats.done, stats.skipped, stats.failed, None)
                .await?;
            SyncMetaRepo::new(&pool)
                .set("last_adj_factor_update", &chrono::Utc::now().to_rfc3339())
                .await?;
        }
        TaskKind::DailyBarScan | TaskKind::XdxrScan | TaskKind::AdjFactorScan => {
            let scanner = ScannerService::new(pool.clone(), config.clone());
            let scan_type = match kind {
                TaskKind::DailyBarScan => "daily_bars",
                TaskKind::XdxrScan => "xdxr",
                TaskKind::AdjFactorScan => "adj_factors",
                _ => unreachable!(),
            };
            emit(0, 0, 0, 1, "扫描中...", false);
            let scan_id = uuid::Uuid::new_v4().to_string();
            let result = scanner.run_scan(scan_type, &scan_id).await?;
            emit(1, 0, 0, 1, "扫描完成", true);
            let detail = serde_json::to_string(&result)?;
            TaskLogRepo::new(&pool)
                .finish(task_id, "success", 1, 0, 0, Some(&detail))
                .await?;
        }
    };

    Ok(())
}

pub async fn probe_adj_factor_tier(config: &AppConfig, pool: &SqlitePool) -> anyhow::Result<AdjFactorTier> {
    if !config.tushare.enabled || config.tushare.token.is_empty() {
        return Ok(AdjFactorTier::L3);
    }
    let client = crate::tushare::TushareClient::new(&config.tushare.token, &config.tushare.base_url);
    let ok = client.probe().await.unwrap_or(false);
    let tier = if ok { AdjFactorTier::L0 } else { AdjFactorTier::L3 };
    let tier_str = tier.to_string();
    let old = SyncMetaRepo::new(pool).get("adj_factor_tier").await?;
    SyncMetaRepo::new(pool).set("adj_factor_tier", &tier_str).await?;
    SyncMetaRepo::new(pool)
        .set("last_probe_at", &chrono::Utc::now().to_rfc3339())
        .await?;
    if let Some(old) = old {
        if old != tier_str {}
    }
    Ok(tier)
}
