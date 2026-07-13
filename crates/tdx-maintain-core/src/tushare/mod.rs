use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
struct TushareResponse {
    code: i32,
    msg: Option<String>,
    data: Option<TushareData>,
}

#[derive(Debug, Clone, Deserialize)]
struct TushareData {
    fields: Vec<String>,
    items: Vec<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeCalDay {
    pub date: String,
    pub is_open: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjFactorDay {
    pub ts_code: String,
    pub trade_date: String,
    pub adj_factor: f64,
}

pub struct TushareClient {
    token: String,
    base_url: String,
    client: reqwest::Client,
}

impl TushareClient {
    pub fn new(token: &str, base_url: &str) -> Self {
        Self {
            token: token.to_string(),
            base_url: base_url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn probe(&self) -> anyhow::Result<bool> {
        if self.token.is_empty() {
            return Ok(false);
        }
        let today = chrono::Utc::now().format("%Y%m%d").to_string();
        let result = self
            .fetch_trade_calendar(&today, &today)
            .await;
        Ok(result.is_ok())
    }

    pub async fn fetch_trade_calendar(
        &self,
        start: &str,
        end: &str,
    ) -> anyhow::Result<Vec<TradeCalDay>> {
        let body = serde_json::json!({
            "api_name": "trade_cal",
            "token": self.token,
            "params": {
                "exchange": "SSE",
                "start_date": start.replace('-', ""),
                "end_date": end.replace('-', ""),
            },
            "fields": "cal_date,is_open"
        });

        let resp: TushareResponse = self
            .client
            .post(&self.base_url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        if resp.code != 0 {
            anyhow::bail!(
                "tushare error: {}",
                resp.msg.unwrap_or_else(|| "unknown".to_string())
            );
        }

        let data = resp.data.ok_or_else(|| anyhow::anyhow!("empty tushare data"))?;
        let mut days = Vec::new();
        for item in data.items {
            let date = item
                .first()
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let formatted = if date.len() == 8 {
                format!("{}-{}-{}", &date[0..4], &date[4..6], &date[6..8])
            } else {
                date
            };
            let is_open = item.get(1).and_then(|v| v.as_i64()).unwrap_or(0) == 1;
            days.push(TradeCalDay {
                date: formatted,
                is_open,
            });
        }
        Ok(days)
    }

    pub async fn fetch_adj_factors(
        &self,
        ts_code: &str,
        start: &str,
        end: &str,
    ) -> anyhow::Result<Vec<AdjFactorDay>> {
        let body = serde_json::json!({
            "api_name": "adj_factor",
            "token": self.token,
            "params": {
                "ts_code": ts_code,
                "start_date": start.replace('-', ""),
                "end_date": end.replace('-', ""),
            },
            "fields": "ts_code,trade_date,adj_factor"
        });

        let resp: TushareResponse = self
            .client
            .post(&self.base_url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        if resp.code != 0 {
            anyhow::bail!(
                "tushare error: {}",
                resp.msg.unwrap_or_else(|| "unknown".to_string())
            );
        }

        let data = resp.data.ok_or_else(|| anyhow::anyhow!("empty tushare data"))?;
        let mut factors = Vec::new();
        for item in data.items {
            let ts_code = item
                .first()
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let trade_date_raw = item.get(1).and_then(|v| v.as_str()).unwrap_or("");
            let trade_date = if trade_date_raw.len() == 8 {
                format!(
                    "{}-{}-{}",
                    &trade_date_raw[0..4],
                    &trade_date_raw[4..6],
                    &trade_date_raw[6..8]
                )
            } else {
                trade_date_raw.to_string()
            };
            let adj_factor = item.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0);
            factors.push(AdjFactorDay {
                ts_code,
                trade_date,
                adj_factor,
            });
        }
        Ok(factors)
    }
}
