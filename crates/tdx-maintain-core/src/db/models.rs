use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TradeCalendarRow {
    pub exchange: String,
    pub trade_date: String,
    pub is_open: i32,
    pub source: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct XdxrEventRow {
    pub market: i32,
    pub symbol: String,
    pub ex_date: String,
    pub category: i32,
    pub fenhong: f64,
    pub peigu: f64,
    pub peigujia: f64,
    pub songzhuangu: f64,
    pub source: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdjFactorRow {
    pub market: i32,
    pub symbol: String,
    pub trade_date: String,
    pub adj_factor: f64,
    pub data_source: String,
    pub confidence: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FactorValidationRow {
    pub market: i32,
    pub symbol: String,
    pub trade_date: String,
    pub tushare_value: Option<f64>,
    pub local_value: Option<f64>,
    pub diff_pct: Option<f64>,
    pub status: String,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SyncMetaRow {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskLogRow {
    pub id: i64,
    pub task_type: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub done_count: i32,
    pub skipped_count: i32,
    pub failed_count: i32,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AlertRow {
    pub id: i64,
    pub level: String,
    pub category: String,
    pub message: String,
    pub detail: Option<String>,
    pub acknowledged: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ScanResultRow {
    pub id: String,
    pub scan_type: String,
    pub status: String,
    pub result_json: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub fn parse_date(s: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

pub fn format_date(d: chrono::NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}
