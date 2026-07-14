use crate::config::AppConfig;
use crate::db::models::now_iso;
use crate::db::repos::SyncMetaRepo;
use crate::alert::AlertEngine;
use crate::tdx::{list_day_symbols, DailyBarReader, get_day_filename};
use crate::downloader::DownloadStats;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Local data structure for adjustment factor rows.
/// This is NOT a database model — factors are stored in Parquet files.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdjFactorRow {
    pub market: i32,
    pub symbol: String,
    pub trade_date: String,
    pub adj_factor: f64,
    pub data_source: String,
    pub confidence: String,
    pub updated_at: String,
}

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
        let _tier_str = meta_repo.get("adj_factor_tier").await?.unwrap_or_else(|| "L3".to_string());

        let base_path = std::path::Path::new(&self.config.paths.tdx_data_dir);
        let base_dir = if base_path.ends_with("vipdoc") {
            base_path.to_path_buf()
        } else {
            base_path.join("vipdoc")
        };

        let base_path_clone = base_path.to_path_buf();
        let symbols = tokio::task::spawn_blocking(move || {
            list_day_symbols(&base_path_clone)
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking cancelled: {e}"))??;
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

        // Collect spawned Parquet write handles for background execution
        let mut parquet_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
        let parquet_alerts = self.alerts.clone();

        for (idx, (market, symbol)) in symbols.iter().enumerate() {
            let market_i = *market as i32;

            // Resolve the correct .day file path
            let filename = get_day_filename(*market, symbol, &base_dir);
            let path = base_dir
                .join(market.dir_name())
                .join("lday")
                .join(filename);

            if !path.exists() {
                stats.skipped += 1;
                continue;
            }

            // Read .day file (blocking I/O offloaded to spawn_blocking)
            let bars = match reader.read_file_async(&path).await {
                Ok(b) if b.is_empty() => { stats.skipped += 1; continue; }
                Ok(b) => b,
                Err(e) => {
                    stats.failed += 1;
                    stats.failures.push(format!("{}#{}: {}", market.dir_name(), symbol, e));
                    continue;
                }
            };
            // Fetch all XDXR events for this symbol
            let xdxr_repo = crate::db::repos::XdxrRepo::new(&self.pool);
            let mut xdxr_events = xdxr_repo.list_for_symbol(market_i, symbol).await.unwrap_or_default();
            xdxr_events.sort_by(|a, b| a.ex_date.cmp(&b.ex_date));

            let mut rows = Vec::new();
            if !bars.is_empty() {
                let mut cumulative_factor = 1.0;
                
                rows.push(AdjFactorRow {
                    market: market_i,
                    symbol: symbol.clone(),
                    trade_date: bars[0].date.format("%Y-%m-%d").to_string(),
                    adj_factor: cumulative_factor,
                    data_source: "local_xdxr".to_string(),
                    confidence: "normal".to_string(),
                    updated_at: now.clone(),
                });

                for t in 1..bars.len() {
                    let prev_bar = &bars[t - 1];
                    let curr_bar = &bars[t];
                    let prev_date_str = prev_bar.date.format("%Y-%m-%d").to_string();
                    let curr_date_str = curr_bar.date.format("%Y-%m-%d").to_string();

                    // Find events falling in (prev_date, curr_date]
                    let active_events: Vec<&crate::db::models::XdxrEventRow> = xdxr_events
                        .iter()
                        .filter(|e| e.ex_date > prev_date_str && e.ex_date <= curr_date_str)
                        .collect();

                    if !active_events.is_empty() {
                        let mut d = 0.0;
                        let mut r_bonus = 0.0;
                        let mut r_placement = 0.0;
                        let mut p_placement = 0.0;

                        for e in active_events {
                            d += e.fenhong;
                            r_bonus += e.songzhuangu;
                            r_placement += e.peigu;
                            if e.peigujia > 0.0 {
                                p_placement = e.peigujia;
                            }
                        }

                        let p_prev = prev_bar.close;
                        if p_prev > 0.0 {
                            let numerator = p_prev - d + p_placement * r_placement;
                            let denominator = p_prev * (1.0 + r_bonus + r_placement);
                            let r_t = numerator / denominator;
                            if r_t > 0.0 && r_t.is_finite() {
                                cumulative_factor /= r_t;
                            }
                        }
                    }

                    rows.push(AdjFactorRow {
                        market: market_i,
                        symbol: symbol.clone(),
                        trade_date: curr_date_str,
                        adj_factor: cumulative_factor,
                        data_source: "local_xdxr".to_string(),
                        confidence: "normal".to_string(),
                        updated_at: now.clone(),
                    });
                }
            }

            // Spawn Parquet write as a background blocking task directly
            let parquet_base = std::path::Path::new(&self.config.paths.parquet_dir).to_path_buf();
            let market_name = market.dir_name().to_string();
            let symbol_clone = symbol.clone();
            let alerts_ref = parquet_alerts.clone();
            let rows_clone = rows;

            let handle = tokio::task::spawn_blocking(move || {
                let parquet_path = parquet_base
                    .join(&market_name)
                    .join(format!("{}.parquet", symbol_clone));

                if let Err(e) = write_parquet_file(&parquet_path, &rows_clone) {
                    // Log the error asynchronously via alerts
                    let market_clone = market_name.clone();
                    let symbol_clone2 = symbol_clone.clone();
                    tokio::runtime::Handle::current().block_on(async move {
                        let _ = alerts_ref.warn(
                            "adj_factor",
                            &format!("{}#{} Parquet 写入失败", market_clone, symbol_clone2),
                            Some(&e.to_string()),
                        ).await;
                    });
                }
            });

            parquet_handles.push(handle);
            stats.done += 1;

            // Report progress every 100 symbols (drain completed handles to limit memory)
            if idx % 100 == 0 {
                parquet_handles.retain(|h| !h.is_finished());
                let msg = format!("计算 {}#{}", market.dir_name(), symbol);
                on_progress(stats.done, stats.skipped, stats.failed, total, &msg);
            }
        }

        // Drain remaining Parquet write handles
        for handle in parquet_handles {
            let _ = handle.await;
        }

        on_progress(stats.done, stats.skipped, stats.failed, total, "完成");
        Ok(stats)
    }
}

/// Helper function to write a symbol's factor rows into a Parquet file with ZSTD compression
fn write_parquet_file(path: &std::path::Path, rows: &[AdjFactorRow]) -> anyhow::Result<()> {
    use arrow::array::{StringArray, Float64Array, Int32Array};
    use arrow::record_batch::RecordBatch;
    use arrow::datatypes::{Schema, Field, DataType};
    use parquet::arrow::arrow_writer::ArrowWriter;
    use parquet::file::properties::WriterProperties;

    let schema = Arc::new(Schema::new(vec![
        Field::new("market", DataType::Int32, false),
        Field::new("symbol", DataType::Utf8, false),
        Field::new("trade_date", DataType::Utf8, false),
        Field::new("adj_factor", DataType::Float64, false),
        Field::new("data_source", DataType::Utf8, false),
        Field::new("confidence", DataType::Utf8, false),
        Field::new("updated_at", DataType::Utf8, false),
    ]));

    let markets: Vec<i32> = rows.iter().map(|r| r.market).collect();
    let symbols: Vec<String> = rows.iter().map(|r| r.symbol.clone()).collect();
    let trade_dates: Vec<String> = rows.iter().map(|r| r.trade_date.clone()).collect();
    let adj_factors: Vec<f64> = rows.iter().map(|r| r.adj_factor).collect();
    let data_sources: Vec<String> = rows.iter().map(|r| r.data_source.clone()).collect();
    let confidences: Vec<String> = rows.iter().map(|r| r.confidence.clone()).collect();
    let updated_ats: Vec<String> = rows.iter().map(|r| r.updated_at.clone()).collect();

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int32Array::from(markets)),
            Arc::new(StringArray::from(symbols)),
            Arc::new(StringArray::from(trade_dates)),
            Arc::new(Float64Array::from(adj_factors)),
            Arc::new(StringArray::from(data_sources)),
            Arc::new(StringArray::from(confidences)),
            Arc::new(StringArray::from(updated_ats)),
        ],
    )?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::File::create(path)?;
    let props = WriterProperties::builder()
        .set_compression(parquet::basic::Compression::ZSTD(parquet::basic::ZstdLevel::default()))
        .build();
    let mut writer = ArrowWriter::try_new(file, schema, Some(props))?;
    writer.write(&batch)?;
    writer.close()?;

    Ok(())
}

/// Helper function to read a symbol's factor rows from a Parquet file
pub fn read_parquet_file(path: &std::path::Path) -> anyhow::Result<Vec<AdjFactorRow>> {
    use arrow::record_batch::RecordBatch;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    let file = std::fs::File::open(path)
        .map_err(|e| anyhow::anyhow!("无法打开 Parquet 文件 {}: {e}", path.display()))?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.build()?;

    let mut rows = Vec::new();
    for batch_result in reader {
        let batch: RecordBatch = batch_result?;
        let market_arr = batch
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::Int32Array>()
            .ok_or_else(|| anyhow::anyhow!("无法读取 market 列"))?;
        let symbol_arr = batch
            .column(1)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .ok_or_else(|| anyhow::anyhow!("无法读取 symbol 列"))?;
        let trade_date_arr = batch
            .column(2)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .ok_or_else(|| anyhow::anyhow!("无法读取 trade_date 列"))?;
        let adj_factor_arr = batch
            .column(3)
            .as_any()
            .downcast_ref::<arrow::array::Float64Array>()
            .ok_or_else(|| anyhow::anyhow!("无法读取 adj_factor 列"))?;
        let data_source_arr = batch
            .column(4)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .ok_or_else(|| anyhow::anyhow!("无法读取 data_source 列"))?;
        let confidence_arr = batch
            .column(5)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .ok_or_else(|| anyhow::anyhow!("无法读取 confidence 列"))?;
        let updated_at_arr = batch
            .column(6)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .ok_or_else(|| anyhow::anyhow!("无法读取 updated_at 列"))?;

        for i in 0..batch.num_rows() {
            rows.push(AdjFactorRow {
                market: market_arr.value(i),
                symbol: symbol_arr.value(i).to_string(),
                trade_date: trade_date_arr.value(i).to_string(),
                adj_factor: adj_factor_arr.value(i),
                data_source: data_source_arr.value(i).to_string(),
                confidence: confidence_arr.value(i).to_string(),
                updated_at: updated_at_arr.value(i).to_string(),
            });
        }
    }

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_row(market: i32, symbol: &str, date: &str, factor: f64) -> AdjFactorRow {
        AdjFactorRow {
            market,
            symbol: symbol.to_string(),
            trade_date: date.to_string(),
            adj_factor: factor,
            data_source: "test".to_string(),
            confidence: "high".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_parquet_write_read_roundtrip() {
        let dir = std::env::temp_dir().join("tdx_test_pq");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sh").join("000001.parquet");

        let rows = vec![
            make_row(1, "000001", "2024-01-02", 1.0),
            make_row(1, "000001", "2024-01-03", 1.05),
            make_row(1, "000001", "2024-01-04", 0.98),
        ];

        write_parquet_file(&path, &rows).unwrap();
        let read_back = read_parquet_file(&path).unwrap();

        assert_eq!(read_back.len(), 3);
        assert_eq!(read_back[0].symbol, "000001");
        assert!((read_back[1].adj_factor - 1.05).abs() < 0.001);
        assert_eq!(read_back[2].trade_date, "2024-01-04");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parquet_read_nonexistent_file() {
        let result = read_parquet_file(std::path::Path::new("/nonexistent/file.parquet"));
        assert!(result.is_err());
    }

    #[test]
    fn test_parquet_empty_write() {
        let dir = std::env::temp_dir().join("tdx_test_pq_empty");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.parquet");

        write_parquet_file(&path, &[]).unwrap();
        let read_back = read_parquet_file(&path).unwrap();
        assert_eq!(read_back.len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_adj_factor_row_serde() {
        let row = make_row(1, "600000", "2024-06-15", 1.5);
        let json = serde_json::to_string(&row).unwrap();
        let deser: AdjFactorRow = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.market, 1);
        assert!((deser.adj_factor - 1.5).abs() < 0.001);
    }
}
