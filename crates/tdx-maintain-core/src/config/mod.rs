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
    pub retry: RetryConfig,
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
    pub parquet_dir: String,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub backoff_ms: u64,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_load_from_file() {
        let dir = std::env::temp_dir().join("tdx_test_config");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.toml");
        std::fs::write(&path, r#"
[server]
host = "0.0.0.0"
port = 9999

[paths]
tdx_data_dir = "/data/tdx"
metadata_db_path = "/data/metadata.db"
backup_dir = "/data/backup"
parquet_dir = "/data/parquet"

[calendar]
benchmark_index_market = 1
benchmark_index_symbol = "000001"
exchange = "SSE"

[tushare]
enabled = true
token = "test_token"
base_url = "http://test.api"

[rate_limit]
market_hours_rps = 10
pre_post_market_rps = 20
off_hours_rps = 50

[adj_factor]
conflict_threshold_pct = 2.0
default_tier = "L2"

[alerts]
daily_completeness_threshold_pct = 90.0

[schedule]
daily_increment_cron = "0 * * * *"
xdxr_sync_cron = "0 1 * * *"
adj_factor_sync_cron = "0 2 * * *"
calendar_check_cron = "0 3 * * *"
weekly_scan_cron = "0 4 * * *"

[retry]
max_attempts = 5
backoff_ms = 2000
"#).unwrap();

        let old_env = std::env::var("TDX_MAINTAIN_CONFIG").ok();
        std::env::set_var("TDX_MAINTAIN_CONFIG", path.to_str().unwrap());

        let config = AppConfig::load().unwrap();
        assert_eq!(config.server.port, 9999);
        assert_eq!(config.paths.tdx_data_dir, "/data/tdx");
        assert_eq!(config.tushare.enabled, true);
        assert_eq!(config.rate_limit.market_hours_rps, 10);
        assert_eq!(config.adj_factor.default_tier, "L2");
        assert_eq!(config.retry.max_attempts, 5);
        assert_eq!(config.retry.backoff_ms, 2000);

        // Restore env
        match old_env {
            Some(v) => std::env::set_var("TDX_MAINTAIN_CONFIG", v),
            None => std::env::remove_var("TDX_MAINTAIN_CONFIG"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_day_dir_builder() {
        let config = AppConfig {
            server: ServerConfig { host: "".into(), port: 0 },
            paths: PathsConfig { tdx_data_dir: "/tdx/vipdoc".into(), metadata_db_path: "".into(), backup_dir: "".into(), parquet_dir: "".into() },
            calendar: CalendarConfig { benchmark_index_market: 1, benchmark_index_symbol: "".into(), exchange: "".into() },
            tushare: TushareConfig { enabled: false, token: "".into(), base_url: "".into() },
            rate_limit: RateLimitConfig { market_hours_rps: 0, pre_post_market_rps: 0, off_hours_rps: 0 },
            adj_factor: AdjFactorConfig { conflict_threshold_pct: 1.0, default_tier: "".into() },
            alerts: AlertsConfig { daily_completeness_threshold_pct: 0.0 },
            schedule: ScheduleConfig { daily_increment_cron: "".into(), xdxr_sync_cron: "".into(), adj_factor_sync_cron: "".into(), calendar_check_cron: "".into(), weekly_scan_cron: "".into() },
            retry: RetryConfig { max_attempts: 3, backoff_ms: 1000 },
        };
        let path = config.day_dir("sh");
        assert!(path.to_str().unwrap().contains("vipdoc"));
        assert!(path.to_str().unwrap().contains("lday"));
    }
}
