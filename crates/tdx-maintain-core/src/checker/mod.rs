use crate::config::AppConfig;
use crate::db::repos::CalendarRepo;
use crate::tdx::{list_day_symbols, DailyBarReader, Market};
use chrono::{FixedOffset, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};
use rustdx::tcp::Tdx as _;

// ── Benchmark securities used to probe TDX server for the latest trading date ──
// Use index codes for SH/SZ (more reliable, always up-to-date) and a stock for BJ.
// IndexKline is used for indices; Kline for stocks.
const BENCHMARK_SECURITIES: &[(Market, &str, bool)] = &[
    (Market::Sh, "000001", true),  // 上证指数 (index)
    (Market::Sz, "399001", true),  // 深证成指 (index)
    (Market::Bj, "830799", false), // 艾融软件 (stock)
];

// ── Output types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreshnessReport {
    pub checked_at: String,
    pub server_reachable: bool,
    pub needs_update: bool,
    pub markets: Vec<MarketFreshness>,
    pub summary: FreshnessSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketFreshness {
    pub market: String,
    pub server_latest_date: Option<String>,
    pub local_latest_date: Option<String>,
    pub days_behind: Option<i64>,
    pub total_stocks: usize,
    pub up_to_date_stocks: usize,
    pub behind_stocks: usize,
    pub missing_stocks: usize,
    pub status: String, // "up_to_date" | "behind" | "no_data" | "server_unreachable"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreshnessSummary {
    pub total_stocks: usize,
    pub up_to_date: usize,
    pub behind: usize,
    pub missing: usize,
}

// ── Service ───────────────────────────────────────────────────────────────

/// Try a complete TDX K-line query: connect + fetch + parse.
///
/// When `addr` is `Some`, uses `Tcp::new_with_ip()`, otherwise `Tcp::new()`.
/// When `is_index` is true, uses `IndexKline` (for index codes like 000001/399001);
/// otherwise uses `Kline` (for stock codes).
/// Returns the latest date from the K-line data, or an error string.
fn try_full_tdx_query(
    market_code: u16,
    symbol: &str,
    addr: Option<std::net::SocketAddr>,
    is_index: bool,
) -> Result<Option<NaiveDate>, String> {
    let mut tcp = if let Some(a) = addr {
        rustdx::tcp::Tcp::new_with_ip(&a).map_err(|e| format!("new_with_ip({a}): {e}"))?
    } else {
        rustdx::tcp::Tcp::new().map_err(|e| format!("new(): {e}"))?
    };

    // Only need the latest bar for freshness; large counts are slow and fragile.
    // IndexKline for indices (extra 4 bytes/bar in response), Kline for stocks.
    let data: Vec<rustdx::tcp::stock::KlineData> = if is_index {
        let mut kline = rustdx::tcp::stock::IndexKline::new(market_code, symbol, 9, 0, 10);
        kline
            .recv_parsed(&mut tcp)
            .map_err(|e| format!("recv_parsed(index): {e}"))?
            .to_vec()
    } else {
        let mut kline = rustdx::tcp::stock::Kline::new(market_code, symbol, 9, 0, 10);
        kline
            .recv_parsed(&mut tcp)
            .map_err(|e| format!("recv_parsed: {e}"))?
            .to_vec()
    };

    if data.is_empty() {
        return Err("empty kline response".into());
    }

    // Pick the max valid calendar date (skip garbage / zero-filled bars).
    let last_date = data
        .iter()
        .filter_map(|k| {
            let y = k.dt.year as i32;
            let m = k.dt.month as u32;
            let d = k.dt.day as u32;
            if (1990..=2100).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d) {
                NaiveDate::from_ymd_opt(y, m, d)
            } else {
                None
            }
        })
        .max();

    last_date
        .map(Some)
        .ok_or_else(|| "no valid kline dates in response".into())
}

pub struct DataFreshnessChecker {
    config: Arc<AppConfig>,
    pool: SqlitePool,
}

impl DataFreshnessChecker {
    pub fn new(config: Arc<AppConfig>, pool: SqlitePool) -> Self {
        Self { config, pool }
    }

    /// Run a full data-freshness check.
    ///
    /// 1. Try to get the latest trading date from the SQLite trade_calendar (no TCP needed).
    /// 2. If the calendar is empty/stale, fall back to probing TDX servers via TCP.
    /// 3. Scan every local `.day` file, recording its last bar date.
    /// 4. Compare local vs remote and produce a report.
    pub async fn check(&self) -> anyhow::Result<FreshnessReport> {
        let beijing_tz = FixedOffset::east_opt(8 * 3600).unwrap();
        let now = chrono::Utc::now()
            .with_timezone(&beijing_tz)
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();

        // ── Primary: use trade_calendar from SQLite (no TCP needed) ──
        let mut server_dates = self.query_calendar_dates().await;
        let server_reachable = server_dates.values().any(|d| d.is_some());

        // ── Fallback: if calendar gave us nothing, try TCP ──
        if !server_reachable {
            info!("Checker: trade_calendar yielded no dates, falling back to TCP probe");
            let tcp_dates = self.query_server_dates().await;
            for (market, date) in tcp_dates {
                server_dates.entry(market).or_insert(date);
            }
        }

        let server_reachable = server_dates.values().any(|d| d.is_some());

        let local_inventory = self.scan_local_dates().await?;

        let mut markets: Vec<MarketFreshness> = Vec::new();
        let mut needs_update = false;

        for market in &[Market::Sh, Market::Sz, Market::Bj] {
            let mf = build_market_freshness(
                *market,
                server_dates.get(&market.dir_name().to_string()),
                &local_inventory,
            );
            if mf.status == "behind" || mf.status == "no_data" {
                needs_update = true;
            }
            markets.push(mf);
        }

        let total_stocks: usize = markets.iter().map(|m| m.total_stocks).sum();
        let up_to_date: usize = markets.iter().map(|m| m.up_to_date_stocks).sum();
        let behind: usize = markets.iter().map(|m| m.behind_stocks).sum();
        let missing: usize = markets.iter().map(|m| m.missing_stocks).sum();

        Ok(FreshnessReport {
            checked_at: now,
            server_reachable,
            needs_update,
            markets,
            summary: FreshnessSummary {
                total_stocks,
                up_to_date,
                behind,
                missing,
            },
        })
    }

    // ── helpers ───────────────────────────────────────────────────────────

    /// Query the trade_calendar in SQLite for the latest trading day ≤ today.
    ///
    /// All Chinese stock markets (SH, SZ, BJ) share the same trading calendar,
    /// so one query covers all three. This completely avoids TCP kline queries.
    async fn query_calendar_dates(&self) -> BTreeMap<String, Option<NaiveDate>> {
        let exchange = self.config.calendar.exchange.clone();
        let today = Local::now().format("%Y-%m-%d").to_string();

        let repo = CalendarRepo::new(&self.pool);
        let latest = repo.latest_trading_day_on_or_before(&exchange, &today).await;

        match latest {
            Ok(Some(date_str)) => {
                match NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") {
                    Ok(date) => {
                        info!(
                            "Checker: trade_calendar latest trading day = {} (exchange={})",
                            date, exchange
                        );
                        // Same date for all three markets
                        let mut map = BTreeMap::new();
                        map.insert(Market::Sh.dir_name().to_string(), Some(date));
                        map.insert(Market::Sz.dir_name().to_string(), Some(date));
                        map.insert(Market::Bj.dir_name().to_string(), Some(date));
                        map
                    }
                    Err(e) => {
                        warn!("Checker: failed to parse calendar date '{}': {}", date_str, e);
                        BTreeMap::new()
                    }
                }
            }
            Ok(None) => {
                info!("Checker: trade_calendar is empty for exchange={}", exchange);
                BTreeMap::new()
            }
            Err(e) => {
                warn!("Checker: trade_calendar query failed: {}", e);
                BTreeMap::new()
            }
        }
    }

    async fn query_server_dates(&self) -> BTreeMap<String, Option<NaiveDate>> {
        let tdx_dir = self.config.paths.tdx_data_dir.clone();

        tokio::task::spawn_blocking(move || {
            use std::net::SocketAddr;

            let servers = crate::tdx_servers::get_server_candidates(Path::new(&tdx_dir));
            let server_addrs: Vec<SocketAddr> = servers.iter().map(|s| s.addr).collect();

            let mut map: BTreeMap<String, Option<NaiveDate>> = BTreeMap::new();
            let mut cached_addr: Option<SocketAddr> = None;

            for &(market, symbol, is_index) in BENCHMARK_SECURITIES {
                let market_code = crate::downloader::market_to_rustdx(market);
                let sym = symbol.to_string();
                let mdir = market.dir_name().to_string();

                let mut date: Option<NaiveDate> = None;

                // ── Step 1: try cached server if available ──
                if let Some(addr) = cached_addr {
                    match try_full_tdx_query(market_code, &sym, Some(addr), is_index) {
                        Ok(d) => {
                            info!("Checker: {:?} {} via cached {} = {:?}", market, symbol, addr, d);
                            date = d;
                        }
                        Err(e) => {
                            info!("Checker: cached server {} failed for {:?}: {}, trying others", addr, market, e);
                            cached_addr = None;
                        }
                    }
                }

                // ── Step 2: try each connect.cfg server ──
                if date.is_none() {
                    for addr in &server_addrs {
                        match try_full_tdx_query(market_code, &sym, Some(*addr), is_index) {
                            Ok(d) => {
                                info!("Checker: {:?} {} via {} = {:?}", market, symbol, addr, d);
                                cached_addr = Some(*addr);
                                date = d;
                                break;
                            }
                            Err(e) => {
                                debug!("Checker: {} unreachable: {}", addr, e);
                            }
                        }
                    }
                }

                // ── Step 3: fallback to rustdx default servers ──
                if date.is_none() {
                    match try_full_tdx_query(market_code, &sym, None, is_index) {
                        Ok(d) => {
                            info!("Checker: {:?} {} via rustdx default = {:?}", market, symbol, d);
                            date = d;
                        }
                        Err(e) => {
                            warn!("Checker: all servers failed for {:?} {}: {}", market, symbol, e);
                        }
                    }
                }

                map.insert(mdir, date);
            }

            if map.values().all(|d| d.is_none()) {
                warn!(
                    "Checker: all TDX servers unreachable (tried {} connect.cfg addrs + rustdx defaults)",
                    server_addrs.len(),
                );
            }

            map
        })
        .await
        .unwrap_or_else(|e| {
            warn!("Checker: spawn_blocking panicked/ cancelled: {}", e);
            BTreeMap::new()
        })
    }

    async fn scan_local_dates(&self) -> anyhow::Result<BTreeMap<String, (Market, NaiveDate)>> {
        let tdx_dir = self.config.paths.tdx_data_dir.clone();

        let inventory = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let symbols = list_day_symbols(Path::new(&tdx_dir))?;
            let reader = DailyBarReader::default();
            let mut map: BTreeMap<String, (Market, NaiveDate)> = BTreeMap::new();

            let base_dir = if tdx_dir.ends_with("vipdoc") {
                Path::new(&tdx_dir).to_path_buf()
            } else {
                Path::new(&tdx_dir).join("vipdoc")
            };

            for (market, symbol) in symbols {
                let filename = crate::tdx::get_day_filename(market, &symbol, &base_dir);
                let path = base_dir
                    .join(market.dir_name())
                    .join("lday")
                    .join(&filename);

                if !path.exists() {
                    continue;
                }

                if let Ok(bars) = reader.read_file(&path) {
                    if let Some(last) = bars.last() {
                        let key = format!("{}/{}", market.dir_name(), symbol);
                        map.insert(key, (market, last.date));
                    }
                }
            }

            Ok(map)
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking cancelled: {e}"))??;

        Ok(inventory)
    }
}

// ── Free function: build per-market freshness ─────────────────────────────

pub fn build_market_freshness(
    market: Market,
    server_date: Option<&Option<NaiveDate>>,
    inventory: &BTreeMap<String, (Market, NaiveDate)>,
) -> MarketFreshness {
    let market_str = market.dir_name().to_string();
    let server_latest = server_date.and_then(|d| *d);
    let server_date_str = server_latest.map(|d| d.format("%Y-%m-%d").to_string());

    let prefix = format!("{}/", market_str);
    let market_stocks: Vec<(&String, Option<NaiveDate>)> = inventory
        .iter()
        .filter(|(k, _)| k.starts_with(&prefix))
        .map(|(k, (_, d))| (k, Some(*d)))
        .collect();

    let total_stocks = market_stocks.len();

    let local_latest: Option<NaiveDate> = market_stocks
        .iter()
        .filter_map(|(_, d)| *d)
        .max();

    let local_date_str = local_latest.map(|d| d.format("%Y-%m-%d").to_string());

    let days_behind = match (server_latest, local_latest) {
        (Some(s), Some(l)) => Some((s - l).num_days()),
        _ => None,
    };

    let mut up_to_date_stocks = 0usize;
    let mut behind_stocks = 0usize;
    let mut missing_stocks = 0usize;

    if let Some(server_d) = server_latest {
        for (_, local_d) in &market_stocks {
            match local_d {
                Some(d) if *d >= server_d => up_to_date_stocks += 1,
                Some(_) => behind_stocks += 1,
                None => missing_stocks += 1,
            }
        }
    } else {
        // Server unreachable — can't judge, treat as unknown (all count as up-to-date)
        up_to_date_stocks = market_stocks.len();
    }

    let status = match (server_latest, local_latest) {
        (None, _) => "server_unreachable".to_string(),
        (Some(_), None) => "no_data".to_string(),
        (Some(s), Some(l)) => {
            if l >= s {
                "up_to_date".to_string()
            } else {
                "behind".to_string()
            }
        }
    };

    MarketFreshness {
        market: market_str,
        server_latest_date: server_date_str,
        local_latest_date: local_date_str,
        days_behind,
        total_stocks,
        up_to_date_stocks,
        behind_stocks,
        missing_stocks,
        status,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_up_to_date_no_gap() {
        let server = Some(NaiveDate::from_ymd_opt(2026, 7, 15).unwrap());
        let server_ref: Option<NaiveDate> = server;
        let mut inv: BTreeMap<String, (Market, NaiveDate)> = BTreeMap::new();
        inv.insert(
            "sh/600000".to_string(),
            (Market::Sh, NaiveDate::from_ymd_opt(2026, 7, 15).unwrap()),
        );
        inv.insert(
            "sh/600001".to_string(),
            (Market::Sh, NaiveDate::from_ymd_opt(2026, 7, 15).unwrap()),
        );

        let mf = build_market_freshness(Market::Sh, Some(&server_ref), &inv);
        assert_eq!(mf.status, "up_to_date");
        assert_eq!(mf.days_behind, Some(0));
        assert_eq!(mf.total_stocks, 2);
        assert_eq!(mf.up_to_date_stocks, 2);
        assert_eq!(mf.behind_stocks, 0);
    }

    #[test]
    fn test_behind_by_days() {
        let server = Some(NaiveDate::from_ymd_opt(2026, 7, 15).unwrap());
        let server_ref: Option<NaiveDate> = server;
        let mut inv: BTreeMap<String, (Market, NaiveDate)> = BTreeMap::new();
        inv.insert(
            "sz/000001".to_string(),
            (Market::Sz, NaiveDate::from_ymd_opt(2026, 7, 10).unwrap()),
        );

        let mf = build_market_freshness(Market::Sz, Some(&server_ref), &inv);
        assert_eq!(mf.status, "behind");
        assert_eq!(mf.days_behind, Some(5));
        assert_eq!(mf.up_to_date_stocks, 0);
        assert_eq!(mf.behind_stocks, 1);
    }

    #[test]
    fn test_no_local_data() {
        let server = Some(NaiveDate::from_ymd_opt(2026, 7, 15).unwrap());
        let server_ref: Option<NaiveDate> = server;
        let inv: BTreeMap<String, (Market, NaiveDate)> = BTreeMap::new();

        let mf = build_market_freshness(Market::Bj, Some(&server_ref), &inv);
        assert_eq!(mf.status, "no_data");
        assert_eq!(mf.total_stocks, 0);
    }

    #[test]
    fn test_server_unreachable() {
        let inv: BTreeMap<String, (Market, NaiveDate)> = BTreeMap::new();
        let mf = build_market_freshness(Market::Sh, None, &inv);
        assert_eq!(mf.status, "server_unreachable");
        assert_eq!(mf.server_latest_date, None);
    }

    #[test]
    fn test_mixed_stocks() {
        // server=2026-07-15, some stocks up to date, some behind
        let server = Some(NaiveDate::from_ymd_opt(2026, 7, 15).unwrap());
        let server_ref: Option<NaiveDate> = server;
        let mut inv: BTreeMap<String, (Market, NaiveDate)> = BTreeMap::new();
        inv.insert(
            "sh/600000".to_string(),
            (Market::Sh, NaiveDate::from_ymd_opt(2026, 7, 15).unwrap()),
        );
        inv.insert(
            "sh/600001".to_string(),
            (Market::Sh, NaiveDate::from_ymd_opt(2026, 7, 14).unwrap()),
        );
        inv.insert(
            "sh/600002".to_string(),
            (Market::Sh, NaiveDate::from_ymd_opt(2026, 7, 15).unwrap()),
        );

        let mf = build_market_freshness(Market::Sh, Some(&server_ref), &inv);
        assert_eq!(mf.status, "up_to_date"); // latest local >= server
        assert_eq!(mf.days_behind, Some(0));
        assert_eq!(mf.total_stocks, 3);
        assert_eq!(mf.up_to_date_stocks, 2);
        assert_eq!(mf.behind_stocks, 1);
    }
}
