//! Qlib binary data dump module.
//!
//! Converts local TDX .day files + Parquet adj_factor data into
//! the Qlib binary format for training, backtesting, and prediction.
//!
//! Reference: D:\gp\investment_data\dump_qlib_bin.py
//!
//! Output directory structure:
//! ```text
//! {qlib_dir}/
//! ├── calendars/
//! │   └── day.txt              # trading calendar (YYYY-MM-DD per line)
//! ├── features/
//! │   └── {symbol_lower}/       # e.g. sh600519/
//! │       ├── open.day.bin
//! │       ├── high.day.bin
//! │       ├── low.day.bin
//! │       ├── close.day.bin
//! │       ├── volume.day.bin
//! │       ├── amount.day.bin
//! │       ├── vwap.day.bin
//! │       └── factor.day.bin
//! └── instruments/
//!     └── all.txt               # SYMBOL\tstart_date\tend_date per line
//! ```
//!
//! Binary format (.day.bin):
//! - Little-endian float32 (f32)
//! - First element: date_index (the stock's start date position in global calendar)
//! - Remaining elements: field values aligned to the global calendar
//! - NaN fill rules:
//!   - price fields (open/high/low/close/vwap): forward-fill then backward-fill
//!   - volume/amount: fill with 0.0
//!   - factor: fill with 1.0

use crate::adj_factor::read_parquet_file;
use crate::config::AppConfig;
use crate::db::models::TradeCalendarRow;
use crate::tdx::{get_day_filename, list_day_symbols, DailyBarReader, Market};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

// ── A-share code prefix filters ──────────────────────────────────────────────
const SH_A_PREFIXES: &[&str] = &["60", "68"];
const SZ_A_PREFIXES: &[&str] = &["00", "30"];
const BJ_A_PREFIXES: &[&str] = &["83", "87", "88", "43", "920"];

/// Stock identification for Qlib output.
#[derive(Debug, Clone)]
struct StockId {
    market: Market,
    symbol: String,
    /// Qlib-style uppercase symbol, e.g. "SH600519", "SZ000001", "BJ430001"
    qlib_symbol: String,
    /// Lowercase directory name, e.g. "sh600519"
    dir_name: String,
}

/// Result statistics after a dump run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DumpStats {
    pub total_files: usize,
    pub a_stock_count: usize,
    pub processed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub failures: Vec<String>,
    pub calendar_days: usize,
    pub output_dir: String,
    pub elapsed_secs: f64,
}

/// Progress information emitted during a dump run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DumpProgress {
    pub processed: usize,
    pub total: usize,
    pub current_symbol: String,
    pub message: String,
}

/// Thread-safe shared state for tracking a Qlib dump run.
///
/// Used by the HTTP server to expose progress to the frontend via polling.
#[derive(Clone)]
pub struct QlibProgressState {
    inner: Arc<std::sync::Mutex<QlibProgressInner>>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct QlibProgressInner {
    running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<DumpProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stats: Option<DumpStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl QlibProgressState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(std::sync::Mutex::new(QlibProgressInner {
                running: false,
                progress: None,
                stats: None,
                error: None,
            })),
        }
    }

    /// Check whether a dump is currently running.
    pub fn is_running(&self) -> bool {
        self.inner.lock().unwrap().running
    }

    /// Begin a new dump run. Returns false if one is already running.
    pub fn start(&self, total: usize) -> bool {
        let mut guard = self.inner.lock().unwrap();
        if guard.running {
            return false;
        }
        guard.running = true;
        guard.progress = Some(DumpProgress {
            processed: 0,
            total,
            current_symbol: String::new(),
            message: "初始化...".to_string(),
        });
        guard.stats = None;
        guard.error = None;
        true
    }

    /// Update progress from a dump callback.
    pub fn update(&self, processed: usize, total: usize, symbol: &str, message: &str) {
        let mut guard = self.inner.lock().unwrap();
        guard.running = true; // safety: ensure running flag is set
        guard.progress = Some(DumpProgress {
            processed,
            total,
            current_symbol: symbol.to_string(),
            message: message.to_string(),
        });
    }

    /// Mark the dump as completed with success stats.
    pub fn complete(&self, stats: DumpStats) {
        let mut guard = self.inner.lock().unwrap();
        guard.running = false;
        guard.stats = Some(stats);
        guard.progress = None;
        guard.error = None;
    }

    /// Mark the dump as failed.
    pub fn fail(&self, error: String) {
        let mut guard = self.inner.lock().unwrap();
        guard.running = false;
        guard.error = Some(error);
        guard.progress = None;
    }

    /// Read current progress for polling.
    pub fn snapshot(&self) -> Option<serde_json::Value> {
        let guard = self.inner.lock().unwrap();
        if guard.running || guard.stats.is_some() || guard.error.is_some() {
            Some(serde_json::to_value(&*guard).unwrap_or(serde_json::Value::Null))
        } else {
            None
        }
    }
}

// ── A-share symbol helpers ──────────────────────────────────────────────────

/// Check whether a TDX symbol code belongs to an A-share stock.
fn is_a_stock(market: Market, code: &str) -> bool {
    if code.len() < 2 {
        return false;
    }
    let prefix = &code[..2];
    match market {
        Market::Sh => SH_A_PREFIXES.contains(&prefix),
        Market::Sz => SZ_A_PREFIXES.contains(&prefix),
        Market::Bj => {
            // BJ codes: 83xxxx, 87xxxx, 88xxxx, 43xxxx, 920xxx
            if code.len() >= 2 {
                BJ_A_PREFIXES.iter().any(|p| code.starts_with(p))
            } else {
                false
            }
        }
    }
}

/// Convert TDX market+symbol to Qlib-style symbol and directory name.
fn make_stock_id(market: Market, symbol: &str) -> Option<StockId> {
    if !is_a_stock(market, symbol) {
        return None;
    }
    let qlib_market = match market {
        Market::Sh => "SH",
        Market::Sz => "SZ",
        Market::Bj => "BJ",
    };
    let qlib_symbol = format!("{}{}", qlib_market, symbol);
    let dir_name = format!("{}{}", market.dir_name(), symbol);
    Some(StockId {
        market,
        symbol: symbol.to_string(),
        qlib_symbol,
        dir_name,
    })
}

// ── QlibDumper service ─────────────────────────────────────────────────────

pub struct QlibDumper {
    pool: SqlitePool,
    config: Arc<AppConfig>,
}

impl QlibDumper {
    pub fn new(pool: SqlitePool, config: Arc<AppConfig>) -> Self {
        Self { pool, config }
    }

    /// Load the trading calendar from SQLite.
    /// Returns sorted list of open trading dates (YYYY-MM-DD strings).
    async fn load_calendar(&self) -> anyhow::Result<Vec<String>> {
        let rows = sqlx::query_as::<_, TradeCalendarRow>(
            "SELECT exchange, trade_date, is_open, source, updated_at
             FROM trade_calendar
             WHERE is_open = 1
             ORDER BY trade_date ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        let dates: Vec<String> = rows.into_iter().map(|r| r.trade_date).collect();
        tracing::info!("Loaded {} trading calendar days", dates.len());
        Ok(dates)
    }

    /// Write `calendars/day.txt`.
    fn write_calendar(&self, qlib_dir: &Path, dates: &[String]) -> anyhow::Result<()> {
        let cal_dir = qlib_dir.join("calendars");
        std::fs::create_dir_all(&cal_dir)?;
        let path = cal_dir.join("day.txt");
        let mut f = std::fs::File::create(&path)?;
        for d in dates {
            writeln!(f, "{}", d)?;
        }
        tracing::info!("Wrote calendar with {} days to {}", dates.len(), path.display());
        Ok(())
    }

    /// Scan all .day files and filter to A-stock only.
    async fn scan_a_stocks(&self, base_dir: &Path) -> anyhow::Result<Vec<StockId>> {
        let base_clone = base_dir.to_path_buf();
        let raw = tokio::task::spawn_blocking(move || list_day_symbols(&base_clone))
            .await
            .map_err(|e| anyhow::anyhow!("spawn_blocking cancelled: {e}"))??;

        let total_files = raw.len();
        let stocks: Vec<StockId> = raw
            .into_iter()
            .filter_map(|(m, s)| make_stock_id(m, &s))
            .collect();

        tracing::info!("Found {} A-stock .day files out of {} total", stocks.len(), total_files);
        Ok(stocks)
    }

    /// Process a single stock: read .day + adj_factor, compute adjusted values,
    /// and write 8 .day.bin files.
    fn process_stock(
        stock: &StockId,
        base_dir: &Path,
        calendar: &[String],
        calendar_set: &HashMap<String, usize>,
        features_dir: &Path,
        parquet_dir: &Path,
    ) -> anyhow::Result<Option<String>> {
        // ── 1. Read .day file ──────────────────────────────────────────
        let filename = get_day_filename(stock.market, &stock.symbol, base_dir);
        let day_path = base_dir
            .join(stock.market.dir_name())
            .join("lday")
            .join(&filename);

        if !day_path.exists() {
            return Ok(None);
        }

        let reader = DailyBarReader::default();
        let bars = reader.read_file(&day_path)?;
        if bars.is_empty() {
            return Ok(None);
        }

        // ── 2. Load adj_factor from Parquet ────────────────────────────
        let parquet_path = parquet_dir
            .join(stock.market.dir_name())
            .join(format!("{}.parquet", stock.symbol));

        let factor_map: HashMap<String, f64> = if parquet_path.exists() {
            match read_parquet_file(&parquet_path) {
                Ok(rows) => rows
                    .into_iter()
                    .map(|r| (r.trade_date, r.adj_factor))
                    .collect(),
                Err(e) => {
                    tracing::warn!("Failed to read parquet for {}: {e}", stock.qlib_symbol);
                    HashMap::new()
                }
            }
        } else {
            HashMap::new()
        };

        // ── 3. Compute Qlib-format adjusted values ─────────────────────
        // Qlib convention: backward-adjusted (后复权) prices
        //   adj_price  = price_raw * cum_factor_hfq
        //   adj_volume = volume_raw_shares / 100.0 / cum_factor_hfq  (→ hands, adjusted)
        //   adj_amount = amount_raw_yuan / 1000.0                     (→ thousands of yuan)
        //   vwap       = (amount_yuan / volume_shares) * factor       (adjusted)
        //   factor     = cum_factor_hfq

        // Build per-date adjusted data map
        let mut date_data: HashMap<String, AdjustedData> = HashMap::new();

        for bar in &bars {
            let date_str = bar.date.format("%Y-%m-%d").to_string();
            // Only include dates present in the calendar
            if !calendar_set.contains_key(&date_str) {
                continue;
            }

            let factor = factor_map.get(&date_str).copied().unwrap_or(1.0);

            // Backward-adjusted prices
            let close_adj = bar.close * factor;
            let open_adj = bar.open * factor;
            let high_adj = bar.high * factor;
            let low_adj = bar.low * factor;

            // Volume: shares → hands × 100 shares (divide by 100), then adjust by factor
            let volume_adj = if factor > 0.0 {
                (bar.volume as f64 / 100.0) / factor
            } else {
                0.0
            };

            // Amount: yuan → thousands of yuan (divide by 1000)
            let amount_adj = bar.amount / 1000.0;

            // VWAP: raw price per share × factor
            let vwap_raw = if bar.volume > 0 {
                bar.amount / bar.volume as f64
            } else {
                bar.close
            };
            let vwap_adj = vwap_raw * factor;

            date_data.insert(
                date_str.clone(),
                AdjustedData {
                    open: open_adj as f32,
                    high: high_adj as f32,
                    low: low_adj as f32,
                    close: close_adj as f32,
                    volume: volume_adj as f32,
                    amount: amount_adj as f32,
                    vwap: vwap_adj as f32,
                    factor: factor as f32,
                },
            );
        }

        if date_data.is_empty() {
            return Ok(None);
        }

        // ── 4. Align to calendar, fill NaN ────────────────────────────
        // Find the first and last calendar date for this stock
        let first_date = bars
            .iter()
            .find_map(|b| {
                let d = b.date.format("%Y-%m-%d").to_string();
                if calendar_set.contains_key(&d) { Some(d) } else { None }
            })
            .ok_or_else(|| anyhow::anyhow!("No calendar-aligned dates for {}", stock.qlib_symbol))?;

        let last_date = bars
            .iter()
            .rev()
            .find_map(|b| {
                let d = b.date.format("%Y-%m-%d").to_string();
                if calendar_set.contains_key(&d) { Some(d) } else { None }
            })
            .ok_or_else(|| anyhow::anyhow!("No calendar-aligned dates for {}", stock.qlib_symbol))?;

        let start_idx = *calendar_set.get(&first_date).unwrap();
        let end_idx = *calendar_set.get(&last_date).unwrap();

        // Extract the calendar slice for this stock
        let sub_calendar = &calendar[start_idx..=end_idx];

        // Build aligned arrays with NaN fill
        let n = sub_calendar.len();
        let mut open_arr = vec![0.0f32; n];
        let mut high_arr = vec![0.0f32; n];
        let mut low_arr = vec![0.0f32; n];
        let mut close_arr = vec![0.0f32; n];
        let mut volume_arr = vec![0.0f32; n];
        let mut amount_arr = vec![0.0f32; n];
        let mut vwap_arr = vec![0.0f32; n];
        let mut factor_arr = vec![1.0f32; n]; // default 1.0 for factor

        let mut has_value = vec![false; n];

        for (i, date) in sub_calendar.iter().enumerate() {
            if let Some(data) = date_data.get(date) {
                open_arr[i] = data.open;
                high_arr[i] = data.high;
                low_arr[i] = data.low;
                close_arr[i] = data.close;
                volume_arr[i] = data.volume;
                amount_arr[i] = data.amount;
                vwap_arr[i] = data.vwap;
                factor_arr[i] = data.factor;
                has_value[i] = true;
            }
        }

        // Forward-fill + backward-fill for price fields
        ffill_bfill(&mut open_arr, &has_value);
        ffill_bfill(&mut high_arr, &has_value);
        ffill_bfill(&mut low_arr, &has_value);
        ffill_bfill(&mut close_arr, &has_value);
        ffill_bfill(&mut vwap_arr, &has_value);

        // Volume and amount: fill 0 (already default)
        // Factor: fill 1.0 (already default)

        // ── 5. Write .day.bin files ────────────────────────────────────
        let feat_dir = features_dir.join(&stock.dir_name);
        std::fs::create_dir_all(&feat_dir)?;

        write_bin_file(&feat_dir.join("open.day.bin"), start_idx as f32, &open_arr)?;
        write_bin_file(&feat_dir.join("high.day.bin"), start_idx as f32, &high_arr)?;
        write_bin_file(&feat_dir.join("low.day.bin"), start_idx as f32, &low_arr)?;
        write_bin_file(&feat_dir.join("close.day.bin"), start_idx as f32, &close_arr)?;
        write_bin_file(&feat_dir.join("volume.day.bin"), start_idx as f32, &volume_arr)?;
        write_bin_file(&feat_dir.join("amount.day.bin"), start_idx as f32, &amount_arr)?;
        write_bin_file(&feat_dir.join("vwap.day.bin"), start_idx as f32, &vwap_arr)?;
        write_bin_file(&feat_dir.join("factor.day.bin"), start_idx as f32, &factor_arr)?;

        // ── 6. Return instrument line ──────────────────────────────────
        Ok(Some(format!(
            "{}\t{}\t{}",
            stock.qlib_symbol, first_date, last_date
        )))
    }

    /// Write `instruments/all.txt`.
    fn write_instruments(&self, qlib_dir: &Path, lines: &[String]) -> anyhow::Result<()> {
        let inst_dir = qlib_dir.join("instruments");
        std::fs::create_dir_all(&inst_dir)?;
        let path = inst_dir.join("all.txt");
        let mut sorted: Vec<String> = lines.to_vec();
        sorted.sort();
        let mut f = std::fs::File::create(&path)?;
        for line in &sorted {
            writeln!(f, "{}", line)?;
        }
        tracing::info!("Wrote {} instruments to {}", sorted.len(), path.display());
        Ok(())
    }

    /// Main dump entry point.
    ///
    /// Reads TDX .day files, Parquet adj_factor data, and writes Qlib binary format.
    ///
    /// `qlib_dir` - output directory for the Qlib binary data.
    /// `progress_cb` - callback for progress reporting: (processed, total, current_symbol, message).
    pub async fn dump<F>(
        &self,
        qlib_dir: &Path,
        progress_cb: F,
    ) -> anyhow::Result<DumpStats>
    where
        F: Fn(usize, usize, &str, &str) + Send + Sync,
    {
        let start = std::time::Instant::now();

        // Clean and create output directories
        if qlib_dir.exists() {
            tracing::info!("Cleaning existing qlib output directory: {}", qlib_dir.display());
            let _ = std::fs::remove_dir_all(qlib_dir);
        }
        std::fs::create_dir_all(qlib_dir)?;
        std::fs::create_dir_all(qlib_dir.join("features"))?;

        // 1. Load trading calendar
        let calendar = self.load_calendar().await?;
        let calendar_set: HashMap<String, usize> = calendar
            .iter()
            .enumerate()
            .map(|(i, d)| (d.clone(), i))
            .collect();

        // 2. Write calendars/day.txt
        self.write_calendar(qlib_dir, &calendar)?;

        // 3. Scan A-stock .day files
        let tdx_base = Path::new(&self.config.paths.tdx_data_dir);
        let base_dir = if tdx_base.ends_with("vipdoc") {
            tdx_base.to_path_buf()
        } else {
            tdx_base.join("vipdoc")
        };

        let stocks = self.scan_a_stocks(&base_dir).await?;
        let total = stocks.len();
        progress_cb(0, total, "", "开始处理股票数据...");

        let features_dir = qlib_dir.join("features");
        let parquet_dir = Path::new(&self.config.paths.parquet_dir);
        let mut instruments = Vec::new();
        let mut skipped = 0usize;
        let mut failed = 0usize;
        let mut failures: Vec<String> = Vec::new();

        // Process each stock sequentially (or could use rayon parallel)
        for (i, stock) in stocks.iter().enumerate() {
            progress_cb(i + 1, total, &stock.qlib_symbol, "处理中...");

            match Self::process_stock(
                stock,
                &base_dir,
                &calendar,
                &calendar_set,
                &features_dir,
                parquet_dir,
            ) {
                Ok(Some(line)) => {
                    instruments.push(line);
                }
                Ok(None) => {
                    skipped += 1;
                }
                Err(e) => {
                    failed += 1;
                    failures.push(format!("{}: {e}", stock.qlib_symbol));
                    tracing::error!("Failed to process {}: {e}", stock.qlib_symbol);
                }
            }
        }

        // 4. Write instruments/all.txt
        self.write_instruments(qlib_dir, &instruments)?;

        let elapsed = start.elapsed().as_secs_f64();
        let stats = DumpStats {
            total_files: stocks.len(),
            a_stock_count: stocks.len(),
            processed: instruments.len(),
            skipped,
            failed,
            failures,
            calendar_days: calendar.len(),
            output_dir: qlib_dir.display().to_string(),
            elapsed_secs: elapsed,
        };

        progress_cb(total, total, "", &format!(
            "完成! 处理 {}/{}, 跳过 {}, 失败 {}, 耗时 {:.1}s",
            stats.processed, total, skipped, failed, elapsed
        ));

        tracing::info!("Qlib dump complete: {:?}", stats);
        Ok(stats)
    }
}

// ── Helper types ────────────────────────────────────────────────────────────

/// Computed adjusted data for a single trading day.
#[derive(Debug, Clone, Copy, Default)]
struct AdjustedData {
    open: f32,
    high: f32,
    low: f32,
    close: f32,
    volume: f32,
    amount: f32,
    vwap: f32,
    factor: f32,
}

/// Forward-fill then backward-fill NaN values in an array.
/// `has_value[i]` indicates whether the original value at position i was valid.
/// Mimics pandas `.ffill().bfill()`: ffill first, then bfill only remaining gaps.
fn ffill_bfill(arr: &mut [f32], has_value: &[bool]) {
    let n = arr.len();
    // Step 1: forward fill — carry last valid value forward
    let mut last_valid: Option<f32> = None;
    for i in 0..n {
        if has_value[i] {
            last_valid = Some(arr[i]);
        } else if let Some(v) = last_valid {
            arr[i] = v;
        }
    }
    // Step 2: backward fill — fill remaining gaps at the beginning.
    // After ffill, only positions BEFORE the first valid value are still 0.0.
    // We track this by checking: no valid value existed before position i.
    let mut next_valid: Option<f32> = None;
    for i in (0..n).rev() {
        if has_value[i] {
            next_valid = Some(arr[i]);
        } else if let Some(v) = next_valid {
            let had_value_before = has_value[..i].iter().any(|&h| h);
            if !had_value_before {
                arr[i] = v;
            }
        }
    }
}

/// Write a Qlib .day.bin file.
///
/// Binary format: [date_index: f32 LE][values...: f32 LE]
fn write_bin_file(path: &Path, date_index: f32, values: &[f32]) -> anyhow::Result<()> {
    let mut data = Vec::with_capacity(4 + values.len() * 4);
    data.extend_from_slice(&date_index.to_le_bytes());
    for v in values {
        data.extend_from_slice(&v.to_le_bytes());
    }
    let mut f = std::fs::File::create(path)?;
    f.write_all(&data)?;
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_a_stock_sh() {
        assert!(is_a_stock(Market::Sh, "600519"));
        assert!(is_a_stock(Market::Sh, "688001"));
        assert!(!is_a_stock(Market::Sh, "000001"));
        assert!(!is_a_stock(Market::Sh, "900901"));
    }

    #[test]
    fn test_is_a_stock_sz() {
        assert!(is_a_stock(Market::Sz, "000001"));
        assert!(is_a_stock(Market::Sz, "300750"));
        assert!(!is_a_stock(Market::Sz, "600519"));
        assert!(!is_a_stock(Market::Sz, "200001"));
    }

    #[test]
    fn test_is_a_stock_bj() {
        assert!(is_a_stock(Market::Bj, "830001"));
        assert!(is_a_stock(Market::Bj, "430001"));
        assert!(is_a_stock(Market::Bj, "920001"));
        assert!(!is_a_stock(Market::Bj, "600519"));
    }

    #[test]
    fn test_make_stock_id() {
        let id = make_stock_id(Market::Sh, "600519").unwrap();
        assert_eq!(id.qlib_symbol, "SH600519");
        assert_eq!(id.dir_name, "sh600519");

        let id = make_stock_id(Market::Sz, "000001").unwrap();
        assert_eq!(id.qlib_symbol, "SZ000001");
        assert_eq!(id.dir_name, "sz000001");

        let id = make_stock_id(Market::Bj, "830001").unwrap();
        assert_eq!(id.qlib_symbol, "BJ830001");
        assert_eq!(id.dir_name, "bj830001");

        assert!(make_stock_id(Market::Sh, "900901").is_none());
    }

    #[test]
    fn test_ffill_bfill() {
        let mut arr = [0.0f32, 0.0, 0.0, 0.0, 0.0];
        let has_value = [true, false, true, false, false];

        // Set initial values only at valid positions
        arr[0] = 1.0;
        arr[2] = 3.0;
        // arr[1], arr[3], arr[4] are 0.0 but marked as false

        ffill_bfill(&mut arr, &has_value);

        assert_eq!(arr[0], 1.0); // original
        assert_eq!(arr[1], 1.0); // forward fill from 0
        assert_eq!(arr[2], 3.0); // original
        assert_eq!(arr[3], 3.0); // forward fill from 2
        assert_eq!(arr[4], 3.0); // forward fill from 2
    }

    #[test]
    fn test_ffill_bfill_all_valid() {
        let mut arr = [1.0f32, 2.0, 3.0];
        let has_value = [true, true, true];
        ffill_bfill(&mut arr, &has_value);
        assert_eq!(arr, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_write_bin_file() {
        let dir = std::env::temp_dir().join("tdx_test_qlib_bin");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.day.bin");

        let values = vec![10.0f32, 20.0, 30.0];
        write_bin_file(&path, 5.0f32, &values).unwrap();

        // Read back and verify
        let data = std::fs::read(&path).unwrap();
        assert_eq!(data.len(), 4 * (1 + values.len())); // 4 bytes per f32

        let read_date_idx = f32::from_le_bytes(data[0..4].try_into().unwrap());
        assert_eq!(read_date_idx, 5.0);

        for (i, v) in values.iter().enumerate() {
            let read_val = f32::from_le_bytes(data[(4 + i * 4)..(8 + i * 4)].try_into().unwrap());
            assert_eq!(read_val, *v);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
