use crate::config::AppConfig;
use crate::db::models::{now_iso, AdjFactorRow, XdxrEventRow, FactorValidationRow};
use crate::db::repos::{AdjFactorRepo, CalendarRepo, SyncMetaRepo, XdxrRepo};
use crate::alert::AlertEngine;
use crate::tdx::{list_day_symbols, DailyBarReader, Market};
use crate::downloader::DownloadStats;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AdjFactorTier {
    L0,
    L1,
    L2,
    L3,
}

impl std::fmt::Display for AdjFactorTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AdjFactorTier::L0 => "L0",
            AdjFactorTier::L1 => "L1",
            AdjFactorTier::L2 => "L2",
            AdjFactorTier::L3 => "L3",
        };
        write!(f, "{}", s)
    }
}

pub struct AdjFactorService {
    pool: SqlitePool,
    config: Arc<AppConfig>,
    alerts: Arc<AlertEngine>,
}

impl AdjFactorService {
    pub fn new(pool: SqlitePool, config: Arc<AppConfig>, alerts: Arc<AlertEngine>) -> Self {
        Self { pool, config, alerts }
    }

    pub async fn sync<F>(&self, mut on_progress: F) -> anyhow::Result<DownloadStats>
    where
        F: FnMut(i32, i32, i32, i32, &str),
    {
        let meta_repo = SyncMetaRepo::new(&self.pool);
        let tier_str = meta_repo.get("adj_factor_tier").await?.unwrap_or_else(|| "L3".to_string());
        let tier = match tier_str.as_str() {
            "L0" => AdjFactorTier::L0,
            "L1" => AdjFactorTier::L1,
            "L2" => AdjFactorTier::L2,
            _ => AdjFactorTier::L3,
        };

        let symbols = list_day_symbols(std::path::Path::new(&self.config.paths.tdx_data_dir))?;
        let total = symbols.len() as i32;
        let mut stats = DownloadStats {
            done: 0,
            skipped: 0,
            failed: 0,
            total,
            failures: Vec::new(),
        };

        let reader = DailyBarReader::default();
        let now = now_iso();

        for (idx, (market, symbol)) in symbols.iter().enumerate() {
            let market_val = *market as i32;
            let progress_msg = format!("计算 {}#{}", market.dir_name(), symbol);
            on_progress(stats.done, stats.skipped, stats.failed, total, &progress_msg);

            let result = self.sync_symbol(*market, symbol, tier, &reader, &now).await;

            match result {
                Ok(updated) => {
                    if updated {
                        stats.done += 1;
                    } else {
                        stats.skipped += 1;
                    }
                }
                Err(e) => {
                    stats.failed += 1;
                    stats.failures.push(format!("{}#{}: {}", market.dir_name(), symbol, e));
                    let _ = self.alerts.error(
                        "adj_factor",
                        &format!("{}#{} 因子计算失败", market.dir_name(), symbol),
                        Some(&e.to_string()),
                    ).await;
                }
            }

            if idx % 100 == 0 {
                on_progress(stats.done, stats.skipped, stats.failed, total, "进行中...");
            }
        }

        on_progress(stats.done, stats.skipped, stats.failed, total, "完成");
        Ok(stats)
    }

    async fn sync_symbol(
        &self,
        market: Market,
        symbol: &str,
        tier: AdjFactorTier,
        reader: &DailyBarReader,
        now_str: &str,
    ) -> anyhow::Result<bool> {
        let market_i = market as i32;
        let path = std::path::Path::new(&self.config.paths.tdx_data_dir)
            .join(market.dir_name())
            .join("lday")
            .join(format!("{}#{}.day", market.dir_name(), symbol));

        if !path.exists() {
            return Ok(false); // No data file, skip
        }

        let bars = reader.read_file(&path)?;
        if bars.is_empty() {
            return Ok(false);
        }

        let adj_repo = AdjFactorRepo::new(&self.pool);

        // Try Tushare first if L0/L1/L2
        let mut tushare_factors = None;
        if tier != AdjFactorTier::L3 && self.config.tushare.enabled && !self.config.tushare.token.is_empty() {
            let client = crate::tushare::TushareClient::new(&self.config.tushare.token, &self.config.tushare.base_url);
            let ts_code = format!("{}.{}", symbol, if market == Market::Sh { "SH" } else { "SZ" });
            
            // Let's call Tushare
            let start_date = bars.first().unwrap().date.format("%Y-%m-%d").to_string();
            let end_date = bars.last().unwrap().date.format("%Y-%m-%d").to_string();
            if let Ok(factors) = client.fetch_adj_factors(&ts_code, &start_date, &end_date).await {
                tushare_factors = Some(factors);
            }
        }

        // L3 local calculation logic always runs for L3, and acts as validation for Tushare
        let xdxr_repo = XdxrRepo::new(&self.pool);
        let events = xdxr_repo.list_for_symbol(market_i, symbol).await?;

        // Sort events by ex_date
        let mut events_map = std::collections::HashMap::new();
        for ev in events {
            events_map.insert(ev.ex_date.clone(), ev);
        }

        // Calculate factors going backwards from the latest bar
        let mut local_factors = Vec::with_capacity(bars.len());
        let mut current_factor = 1.0;

        // Iterate backwards
        for i in (0..bars.len()).rev() {
            let bar = &bars[i];
            let date_str = bar.date.format("%Y-%m-%d").to_string();

            // If we are not at the latest bar, check if the *next* day was an ex-date
            if i < bars.len() - 1 {
                let next_bar = &bars[i + 1];
                let next_date_str = next_bar.date.format("%Y-%m-%d").to_string();

                if let Some(ev) = events_map.get(&next_date_str) {
                    let p_close = bar.close;
                    if p_close > 0.0 {
                        // Formula: P_ex = (P_close - D + P_rights * R_rights) / (1 + R_bonus + R_rights)
                        // factor multiplier = P_ex / P_close
                        let d = ev.fenhong;
                        let r_bonus = ev.songzhuangu;
                        let r_rights = ev.peigu;
                        let p_rights = ev.peigujia;

                        let p_ex = (p_close - d + p_rights * r_rights) / (1.0 + r_bonus + r_rights);
                        let multiplier = p_ex / p_close;
                        current_factor *= multiplier;
                    }
                }
            }

            local_factors.push((date_str, current_factor));
        }

        // Reverse to make it chronological
        local_factors.reverse();

        if let Some(ts_factors) = tushare_factors {
            // Map Tushare factors for validation
            let mut ts_map = std::collections::HashMap::new();
            for f in &ts_factors {
                ts_map.insert(f.trade_date.clone(), f.adj_factor);
            }

            // Write Tushare factors, and perform cross-validation
            for (date_str, local_val) in local_factors {
                let ts_val = ts_map.get(&date_str).cloned();
                
                let (final_val, source, confidence) = if let Some(ts_f) = ts_val {
                    let diff = (ts_f - local_val).abs();
                    let diff_pct = if local_val > 0.0 { (diff / local_val) * 100.0 } else { 0.0 };
                    
                    let status = if diff_pct > self.config.adj_factor.conflict_threshold_pct {
                        // Cross-validation conflict!
                        let _ = adj_repo.upsert_validation(&FactorValidationRow {
                            market: market_i,
                            symbol: symbol.to_string(),
                            trade_date: date_str.clone(),
                            tushare_value: Some(ts_f),
                            local_value: Some(local_val),
                            diff_pct: Some(diff_pct),
                            status: "mismatch".to_string(),
                            checked_at: now_str.to_string(),
                        }).await;

                        let _ = self.alerts.warn(
                            "validation",
                            &format!("{}#{} 在 {} 存在因子校验偏差 ({:.2}%)", market.dir_name(), symbol, date_str, diff_pct),
                            Some(&format!("Tushare: {}, 本地: {}", ts_f, local_val)),
                        ).await;
                        
                        "conflict"
                    } else {
                        "normal"
                    };

                    (ts_f, "tushare".to_string(), status.to_string())
                } else {
                    (local_val, "local_xdxr".to_string(), "normal".to_string())
                };

                adj_repo.upsert(&AdjFactorRow {
                    market: market_i,
                    symbol: symbol.to_string(),
                    trade_date: date_str,
                    adj_factor: final_val,
                    data_source: source,
                    confidence,
                    updated_at: now_str.to_string(),
                }).await?;
            }
        } else {
            // Write local factors
            for (date_str, local_val) in local_factors {
                adj_repo.upsert(&AdjFactorRow {
                    market: market_i,
                    symbol: symbol.to_string(),
                    trade_date: date_str,
                    adj_factor: local_val,
                    data_source: "local_xdxr".to_string(),
                    confidence: "normal".to_string(),
                    updated_at: now_str.to_string(),
                }).await?;
            }
        }

        Ok(true)
    }
}
