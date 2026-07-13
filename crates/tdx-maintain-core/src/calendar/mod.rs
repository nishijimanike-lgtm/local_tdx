use crate::config::AppConfig;
use crate::db::models::{format_date, now_iso, TradeCalendarRow};
use crate::db::repos::{CalendarRepo, SyncMetaRepo};
use crate::tdx::day_file::DailyBarReader;
use crate::tdx::Market;
use sqlx::SqlitePool;

pub struct CalendarService {
    pool: SqlitePool,
    config: AppConfig,
}

impl CalendarService {
    pub fn new(pool: SqlitePool, config: AppConfig) -> Self {
        Self { pool, config }
    }

    pub async fn build_from_index(&self) -> anyhow::Result<usize> {
        let market = match self.config.calendar.benchmark_index_market {
            0 => Market::Sz,
            1 => Market::Sh,
            2 => Market::Bj,
            _ => Market::Sh,
        };
        let symbol = &self.config.calendar.benchmark_index_symbol;
        let path = self
            .config
            .paths
            .tdx_data_dir
            .clone()
            .into()
            .join(market.dir_name())
            .join("lday")
            .join(format!("{}#{}.day", market.dir_name(), symbol));

        let reader = DailyBarReader::default();
        let bars = reader.read_file(&path)?;
        let now = now_iso();
        let exchange = &self.config.calendar.exchange;

        let rows: Vec<TradeCalendarRow> = bars
            .iter()
            .map(|b| TradeCalendarRow {
                exchange: exchange.clone(),
                trade_date: format_date(b.date),
                is_open: 1,
                source: "index_derived".to_string(),
                updated_at: now.clone(),
            })
            .collect();

        let repo = CalendarRepo::new(&self.pool);
        let count = rows.len();
        repo.upsert_batch(&rows).await?;

        let meta = SyncMetaRepo::new(&self.pool);
        meta.set("calendar_source", "index_derived").await?;
        Ok(count)
    }

    pub async fn sync_incremental(&self) -> anyhow::Result<usize> {
        let repo = CalendarRepo::new(&self.pool);
        let latest = repo
            .latest_trading_day(&self.config.calendar.exchange)
            .await?;

        let market = match self.config.calendar.benchmark_index_market {
            0 => Market::Sz,
            1 => Market::Sh,
            2 => Market::Bj,
            _ => Market::Sh,
        };
        let symbol = &self.config.calendar.benchmark_index_symbol;
        let path: std::path::PathBuf = self
            .config
            .paths
            .tdx_data_dir
            .clone()
            .into();
        let path = path
            .join(market.dir_name())
            .join("lday")
            .join(format!("{}#{}.day", market.dir_name(), symbol));

        let reader = DailyBarReader::default();
        let bars = reader.read_file(&path)?;
        let now = now_iso();
        let exchange = &self.config.calendar.exchange;

        let new_rows: Vec<TradeCalendarRow> = bars
            .iter()
            .filter(|b| {
                latest
                    .as_ref()
                    .map(|l| format_date(b.date) > *l)
                    .unwrap_or(true)
            })
            .map(|b| TradeCalendarRow {
                exchange: exchange.clone(),
                trade_date: format_date(b.date),
                is_open: 1,
                source: "index_derived".to_string(),
                updated_at: now.clone(),
            })
            .collect();

        let count = new_rows.len();
        if !new_rows.is_empty() {
            repo.upsert_batch(&new_rows).await?;
        }
        Ok(count)
    }

    pub async fn sync_from_tushare(
        &self,
        tushare: &crate::tushare::TushareClient,
        start: &str,
        end: &str,
    ) -> anyhow::Result<usize> {
        let days = tushare.fetch_trade_calendar(start, end).await?;
        let now = now_iso();
        let rows: Vec<TradeCalendarRow> = days
            .into_iter()
            .map(|d| TradeCalendarRow {
                exchange: self.config.calendar.exchange.clone(),
                trade_date: d.date,
                is_open: if d.is_open { 1 } else { 0 },
                source: "tushare".to_string(),
                updated_at: now.clone(),
            })
            .collect();
        let count = rows.len();
        CalendarRepo::new(&self.pool)
            .upsert_batch(&rows)
            .await?;
        SyncMetaRepo::new(&self.pool)
            .set("calendar_source", "tushare")
            .await?;
        Ok(count)
    }

    pub async fn is_trading_day(&self, date: &str) -> anyhow::Result<bool> {
        CalendarRepo::new(&self.pool)
            .is_trading_day(&self.config.calendar.exchange, date)
            .await
    }

    pub async fn latest_trading_day(&self) -> anyhow::Result<Option<String>> {
        CalendarRepo::new(&self.pool)
            .latest_trading_day(&self.config.calendar.exchange)
            .await
    }

    pub async fn list(&self, start: &str, end: &str) -> anyhow::Result<Vec<TradeCalendarRow>> {
        CalendarRepo::new(&self.pool)
            .list(&self.config.calendar.exchange, start, end)
            .await
    }
}
