use crate::config::AppConfig;
use crate::db::models::now_iso;
use crate::db::repos::{CalendarRepo, ScanRepo};
use crate::tdx::{list_day_symbols, DailyBarReader, Market};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::sync::Arc;

pub struct ScannerService {
    pool: SqlitePool,
    config: Arc<AppConfig>,
}

impl ScannerService {
    pub fn new(pool: SqlitePool, config: Arc<AppConfig>) -> Self {
        Self { pool, config }
    }

    pub async fn run_scan(&self, scan_type: &str, scan_id: &str) -> anyhow::Result<Value> {
        let scan_repo = ScanRepo::new(&self.pool);
        scan_repo.create(scan_id, scan_type).await?;

        let result = match scan_type {
            "daily_bars" => self.scan_daily_bars().await,
            "xdxr" => self.scan_xdxr().await,
            "adj_factors" => self.scan_adj_factors().await,
            _ => Err(anyhow::anyhow!("unknown scan type: {scan_type}")),
        };

        match result {
            Ok(val) => {
                let json_str = serde_json::to_string(&val)?;
                scan_repo.finish(scan_id, "success", &json_str).await?;
                Ok(val)
            }
            Err(e) => {
                let err_val = json!({ "error": e.to_string() });
                let json_str = serde_json::to_string(&err_val)?;
                scan_repo.finish(scan_id, "failed", &json_str).await?;
                Err(e)
            }
        }
    }

    async fn scan_daily_bars(&self) -> anyhow::Result<Value> {
        let calendar_repo = CalendarRepo::new(&self.pool);
        let exchange = &self.config.calendar.exchange;
        
        // Fetch all trading days from the trade_calendar table
        let trading_days = calendar_repo.get_trading_days(exchange, "1990-01-01", "2099-12-31").await?;
        if trading_days.is_empty() {
            return Ok(json!({
                "summary": {
                    "total_symbols": 0,
                    "missing_files": 0,
                    "gaps_count": 0,
                    "lagging_symbols": 0,
                    "message": "交易日历为空，请先同步交易日历"
                },
                "missing_files": [],
                "gaps": [],
                "lags": []
            }));
        }

        let latest_trading_day = trading_days.last().cloned().unwrap();
        let symbols = list_day_symbols(std::path::Path::new(&self.config.paths.tdx_data_dir))?;
        
        let mut missing_files = Vec::new();
        let mut gaps = Vec::new();
        let mut lags = Vec::new();
        
        let reader = DailyBarReader::default();
        let total_symbols = symbols.len();

        for (market, symbol) in &symbols {
            let path = std::path::Path::new(&self.config.paths.tdx_data_dir)
                .join(market.dir_name())
                .join("lday")
                .join(format!("{}#{}.day", market.dir_name(), symbol));

            if !path.exists() {
                missing_files.push(json!({
                    "market": market.dir_name(),
                    "symbol": symbol
                }));
                continue;
            }

            match reader.read_file(&path) {
                Ok(bars) => {
                    if bars.is_empty() {
                        missing_files.push(json!({
                            "market": market.dir_name(),
                            "symbol": symbol,
                            "reason": "empty file"
                        }));
                        continue;
                    }

                    // Check latest date lag
                    let file_latest_date = bars.last().unwrap().date.format("%Y-%m-%d").to_string();
                    if file_latest_date < latest_trading_day {
                        let lag_days = trading_days.iter()
                            .filter(|&&ref d| *d > file_latest_date && *d <= latest_trading_day)
                            .count();
                        if lag_days > 0 {
                            lags.push(json!({
                                "market": market.dir_name(),
                                "symbol": symbol,
                                "latest_date": file_latest_date,
                                "lag_days": lag_days
                            }));
                        }
                    }

                    // Check for gaps
                    let file_dates: std::collections::HashSet<String> = bars.iter()
                        .map(|b| b.date.format("%Y-%m-%d").to_string())
                        .collect();

                    let file_start = bars.first().unwrap().date.format("%Y-%m-%d").to_string();
                    let file_end = file_latest_date;

                    let mut current_gap_start: Option<String> = None;
                    let mut current_gap_end: Option<String> = None;

                    for day in &trading_days {
                        if *day < file_start {
                            continue;
                        }
                        if *day > file_end {
                            break;
                        }

                        if !file_dates.contains(day) {
                            if current_gap_start.is_none() {
                                current_gap_start = Some(day.clone());
                            }
                            current_gap_end = Some(day.clone());
                        } else if let (Some(start), Some(end)) = (current_gap_start.take(), current_gap_end.take()) {
                            gaps.push(json!({
                                "market": market.dir_name(),
                                "symbol": symbol,
                                "start": start,
                                "end": end
                            }));
                        }
                    }
                    if let (Some(start), Some(end)) = (current_gap_start.take(), current_gap_end.take()) {
                        gaps.push(json!({
                            "market": market.dir_name(),
                            "symbol": symbol,
                            "start": start,
                            "end": end
                        }));
                    }
                }
                Err(e) => {
                    missing_files.push(json!({
                        "market": market.dir_name(),
                        "symbol": symbol,
                        "reason": format!("read error: {e}")
                    }));
                }
            }
        }

        let summary = json!({
            "total_symbols": total_symbols,
            "missing_files": missing_files.len(),
            "gaps_count": gaps.len(),
            "lagging_symbols": lags.len()
        });

        Ok(json!({
            "summary": summary,
            "missing_files": missing_files,
            "gaps": gaps,
            "lags": lags
        }))
    }

    async fn scan_xdxr(&self) -> anyhow::Result<Value> {
        // Count events in the xdxr_events table
        let count_row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM xdxr_events")
            .fetch_one(&self.pool)
            .await?;
        
        let unique_symbols: (i64,) = sqlx::query_as("SELECT COUNT(DISTINCT symbol) FROM xdxr_events")
            .fetch_one(&self.pool)
            .await?;

        Ok(json!({
            "summary": {
                "total_events": count_row.0,
                "unique_symbols": unique_symbols.0
            }
        }))
    }

    async fn scan_adj_factors(&self) -> anyhow::Result<Value> {
        let calendar_repo = CalendarRepo::new(&self.pool);
        let exchange = &self.config.calendar.exchange;
        let latest_trading_day = calendar_repo.latest_trading_day(exchange).await?;

        let total_factors: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM adj_factor")
            .fetch_one(&self.pool)
            .await?;

        let unique_symbols: (i64,) = sqlx::query_as("SELECT COUNT(DISTINCT symbol) FROM adj_factor")
            .fetch_one(&self.pool)
            .await?;

        let mut lagging_symbols = Vec::new();
        if let Some(ref latest_day) = latest_trading_day {
            // Find symbols whose latest adj_factor trade_date is older than latest_trading_day
            let rows: Vec<(i32, String, String)> = sqlx::query_as(
                "SELECT market, symbol, MAX(trade_date) FROM adj_factor GROUP BY market, symbol HAVING MAX(trade_date) < ?"
            )
            .bind(latest_day)
            .fetch_all(&self.pool)
            .await?;

            for r in rows {
                lagging_symbols.push(json!({
                    "market": r.0,
                    "symbol": r.1,
                    "latest_factor_date": r.2
                }));
            }
        }

        Ok(json!({
            "summary": {
                "total_factors": total_factors.0,
                "unique_symbols": unique_symbols.0,
                "lagging_symbols_count": lagging_symbols.len()
            },
            "lagging_symbols": lagging_symbols
        }))
    }
}
