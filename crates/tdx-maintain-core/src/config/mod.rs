use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub paths: PathsConfig,
    pub calendar: CalendarConfig,
    pub tushare: TushareConfig,
    pub rate_limit: RateLimitConfig,
    pub adj_factor: AdjFactorConfig,
    pub alerts: AlertsConfig,
    pub schedule: ScheduleConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PathsConfig {
    pub tdx_data_dir: String,
    pub metadata_db_path: String,
    pub backup_dir: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CalendarConfig {
    pub benchmark_index_market: i32,
    pub benchmark_index_symbol: String,
    pub exchange: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TushareConfig {
    pub enabled: bool,
    pub token: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitConfig {
    pub market_hours_rps: u32,
    pub pre_post_market_rps: u32,
    pub off_hours_rps: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdjFactorConfig {
    pub conflict_threshold_pct: f64,
    pub default_tier: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlertsConfig {
    pub daily_completeness_threshold_pct: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleConfig {
    pub daily_increment_cron: String,
    pub xdxr_sync_cron: String,
    pub adj_factor_sync_cron: String,
    pub calendar_check_cron: String,
    pub weekly_scan_cron: String,
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let config_path = std::env::var("TDX_MAINTAIN_CONFIG")
            .unwrap_or_else(|_| "config/default.toml".to_string());

        let settings = config::Config::builder()
            .add_source(config::File::with_name(&config_path))
            .add_source(config::Environment::with_prefix("TDX_MAINTAIN").separator("__"))
            .build()?;

        Ok(settings.try_deserialize()?)
    }

    pub fn day_dir(&self, market: &str) -> std::path::PathBuf {
        std::path::Path::new(&self.paths.tdx_data_dir).join(market).join("lday")
    }
}
