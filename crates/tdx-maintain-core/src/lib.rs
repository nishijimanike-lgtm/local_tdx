pub mod adj_factor;
pub mod alert;
pub mod calendar;
pub mod checker;
pub mod config;
pub mod db;
pub mod downloader;
pub mod qlib;
pub mod scanner;
pub mod task;
pub mod tdx;
pub mod tdx_servers;
pub mod tushare;

pub use config::AppConfig;
pub use qlib::QlibProgressState;

use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<AppConfig>,
    pub task_queue: Arc<task::TaskQueue>,
    pub alert_engine: Arc<alert::AlertEngine>,
    pub qlib_progress: QlibProgressState,
}

impl AppState {
    pub async fn new(config: AppConfig) -> anyhow::Result<Self> {
        let config = Arc::new(config);
        let pool = db::init_pool(&config.paths.metadata_db_path).await?;
        db::run_migrations(&pool).await?;
        let alert_engine = Arc::new(alert::AlertEngine::new(pool.clone()));
        let task_queue = Arc::new(task::TaskQueue::new(
            pool.clone(),
            config.clone(),
            alert_engine.clone(),
        ));
        Ok(Self {
            pool,
            config,
            task_queue,
            alert_engine,
            qlib_progress: QlibProgressState::new(),
        })
    }
}
