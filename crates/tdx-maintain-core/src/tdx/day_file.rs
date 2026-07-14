use chrono::{NaiveDate, Datelike};
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DailyBar {
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub amount: f64,
    pub volume: u32,
}

pub struct DailyBarReader {
    coefficient: f64,
}

impl Default for DailyBarReader {
    fn default() -> Self {
        Self { coefficient: 0.01 }
    }
}

impl DailyBarReader {
    pub fn new(coefficient: f64) -> Self {
        Self { coefficient }
    }

    pub fn read_file(&self, path: &Path) -> anyhow::Result<Vec<DailyBar>> {
        let mut file = std::fs::File::open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        if buf.len() % 32 != 0 {
            anyhow::bail!("invalid .day file size: {}", path.display());
        }

        let mut bars = Vec::with_capacity(buf.len() / 32);
        for chunk in buf.chunks_exact(32) {
            let date_raw = u32::from_le_bytes(chunk[0..4].try_into()?);
            let open = i32::from_le_bytes(chunk[4..8].try_into()?);
            let high = i32::from_le_bytes(chunk[8..12].try_into()?);
            let low = i32::from_le_bytes(chunk[12..16].try_into()?);
            let close = i32::from_le_bytes(chunk[16..20].try_into()?);
            let amount = f32::from_le_bytes(chunk[20..24].try_into()?);
            let volume = u32::from_le_bytes(chunk[24..28].try_into()?);

            let year = (date_raw / 10000) as i32;
            let month = ((date_raw % 10000) / 100) as u32;
            let day = (date_raw % 100) as u32;
            let date = NaiveDate::from_ymd_opt(year, month, day)
                .ok_or_else(|| anyhow::anyhow!("invalid date in day file: {date_raw}"))?;

            bars.push(DailyBar {
                date,
                open: open as f64 * self.coefficient,
                high: high as f64 * self.coefficient,
                low: low as f64 * self.coefficient,
                close: close as f64 * self.coefficient,
                amount: amount as f64,
                volume,
            });
        }
        bars.sort_by_key(|b| b.date);
        Ok(bars)
    }

    /// Async wrapper that runs blocking file I/O off the tokio worker thread.
    pub async fn read_file_async(&self, path: &Path) -> anyhow::Result<Vec<DailyBar>> {
        let coefficient = self.coefficient;
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let reader = DailyBarReader::new(coefficient);
            reader.read_file(&path)
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking cancelled: {e}"))?
    }

    pub fn last_date(&self, path: &Path) -> anyhow::Result<Option<NaiveDate>> {
        let bars = self.read_file(path)?;
        Ok(bars.last().map(|b| b.date))
    }

    /// Async wrapper for last_date.
    pub async fn last_date_async(&self, path: &Path) -> anyhow::Result<Option<NaiveDate>> {
        let bars = self.read_file_async(path).await?;
        Ok(bars.last().map(|b| b.date))
    }

    pub fn dates(&self, path: &Path) -> anyhow::Result<Vec<NaiveDate>> {
        Ok(self.read_file(path)?.into_iter().map(|b| b.date).collect())
    }
}

pub struct DailyBarWriter {
    coefficient: f64,
}

impl Default for DailyBarWriter {
    fn default() -> Self {
        Self { coefficient: 0.01 }
    }
}

impl DailyBarWriter {
    pub fn new(coefficient: f64) -> Self {
        Self { coefficient }
    }

    pub fn write_file(&self, path: &Path, bars: &[DailyBar]) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(path)?;
        for bar in bars {
            let date_raw = bar.date.year() as u32 * 10000
                + bar.date.month() * 100
                + bar.date.day();
            let open = (bar.open / self.coefficient).round() as i32;
            let high = (bar.high / self.coefficient).round() as i32;
            let low = (bar.low / self.coefficient).round() as i32;
            let close = (bar.close / self.coefficient).round() as i32;
            let amount = bar.amount as f32;
            let volume = bar.volume;

            let mut record = [0u8; 32];
            record[0..4].copy_from_slice(&date_raw.to_le_bytes());
            record[4..8].copy_from_slice(&open.to_le_bytes());
            record[8..12].copy_from_slice(&high.to_le_bytes());
            record[12..16].copy_from_slice(&low.to_le_bytes());
            record[16..20].copy_from_slice(&close.to_le_bytes());
            record[20..24].copy_from_slice(&amount.to_le_bytes());
            record[24..28].copy_from_slice(&volume.to_le_bytes());
            file.write_all(&record)?;
        }
        Ok(())
    }

    /// Async wrapper that runs blocking file I/O off the tokio worker thread.
    pub async fn write_file_async(&self, path: &Path, bars: &[DailyBar]) -> anyhow::Result<()> {
        let coefficient = self.coefficient;
        let path = path.to_path_buf();
        let bars = bars.to_vec();
        tokio::task::spawn_blocking(move || {
            let writer = DailyBarWriter::new(coefficient);
            writer.write_file(&path, &bars)
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking cancelled: {e}"))?
    }

    pub fn append_file(&self, path: &Path, new_bars: &[DailyBar]) -> anyhow::Result<()> {
        let reader = DailyBarReader::new(self.coefficient);
        let mut existing = if path.exists() {
            reader.read_file(path)?
        } else {
            Vec::new()
        };
        for bar in new_bars {
            if !existing.iter().any(|b| b.date == bar.date) {
                existing.push(bar.clone());
            }
        }
        existing.sort_by_key(|b| b.date);
        self.write_file(path, &existing)
    }

    /// Async wrapper for append_file (reads the existing file off-thread too).
    pub async fn append_file_async(&self, path: &Path, new_bars: &[DailyBar]) -> anyhow::Result<()> {
        let coefficient = self.coefficient;
        let path = path.to_path_buf();
        let new_bars = new_bars.to_vec();
        tokio::task::spawn_blocking(move || {
            let writer = DailyBarWriter::new(coefficient);
            writer.append_file(&path, &new_bars)
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking cancelled: {e}"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_bar(year: i32, month: u32, day: u32, close: f64) -> DailyBar {
        DailyBar {
            date: NaiveDate::from_ymd_opt(year, month, day).unwrap(),
            open: close - 0.5,
            high: close + 0.3,
            low: close - 0.8,
            close,
            amount: 1_000_000.0,
            volume: 50000,
        }
    }

    #[test]
    fn test_write_read_roundtrip() {
        let writer = DailyBarWriter::default();
        let reader = DailyBarReader::default();
        let dir = std::env::temp_dir().join("tdx_test_roundtrip");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("sh000001.day");

        let bars = vec![
            make_bar(2024, 1, 2, 10.5),
            make_bar(2024, 1, 3, 11.0),
            make_bar(2024, 1, 4, 10.8),
        ];
        writer.write_file(&path, &bars).unwrap();
        let read_back = reader.read_file(&path).unwrap();

        assert_eq!(read_back.len(), 3);
        assert_eq!(read_back[0].date, bars[0].date);
        assert!((read_back[0].close - bars[0].close).abs() < 0.001);
        assert_eq!(read_back[2].volume, bars[2].volume);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_read_empty() {
        let writer = DailyBarWriter::default();
        let reader = DailyBarReader::default();
        let dir = std::env::temp_dir().join("tdx_test_empty");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("sh000001.day");

        writer.write_file(&path, &[]).unwrap();
        let read_back = reader.read_file(&path).unwrap();
        assert_eq!(read_back.len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_read_invalid_size() {
        let reader = DailyBarReader::default();
        let dir = std::env::temp_dir().join("tdx_test_invalid");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("bad.day");

        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&[0u8; 31]).unwrap(); // not divisible by 32

        let result = reader.read_file(&path);
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_reader_sorts_by_date() {
        let writer = DailyBarWriter::default();
        let reader = DailyBarReader::default();
        let dir = std::env::temp_dir().join("tdx_test_sort");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("sh000001.day");

        // Write in reverse order
        let bars = vec![
            make_bar(2024, 1, 5, 12.0),
            make_bar(2024, 1, 2, 10.0),
            make_bar(2024, 1, 3, 11.0),
        ];
        writer.write_file(&path, &bars).unwrap();
        let read_back = reader.read_file(&path).unwrap();

        assert_eq!(read_back.len(), 3);
        assert!(read_back[0].date < read_back[1].date);
        assert!(read_back[1].date < read_back[2].date);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_append_file_no_duplicates() {
        let writer = DailyBarWriter::default();
        let reader = DailyBarReader::default();
        let dir = std::env::temp_dir().join("tdx_test_append");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("sh000001.day");

        let initial = vec![make_bar(2024, 1, 2, 10.0)];
        writer.write_file(&path, &initial).unwrap();

        let update = vec![
            make_bar(2024, 1, 2, 99.0), // same date, should not duplicate
            make_bar(2024, 1, 3, 11.0),
        ];
        writer.append_file(&path, &update).unwrap();

        let result = reader.read_file(&path).unwrap();
        assert_eq!(result.len(), 2, "should have 2 unique dates");
        assert!((result[0].close - 10.0).abs() < 0.001, "existing bar should not be overwritten");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_last_date() {
        let writer = DailyBarWriter::default();
        let reader = DailyBarReader::default();
        let dir = std::env::temp_dir().join("tdx_test_last");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("sh000001.day");

        let bars = vec![
            make_bar(2024, 1, 2, 10.0),
            make_bar(2024, 6, 15, 15.0),
        ];
        writer.write_file(&path, &bars).unwrap();

        let last = reader.last_date(&path).unwrap();
        assert_eq!(last, Some(NaiveDate::from_ymd_opt(2024, 6, 15).unwrap()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_file_creates_parent_dir() {
        let writer = DailyBarWriter::default();
        let reader = DailyBarReader::default();
        let dir = std::env::temp_dir().join("tdx_test_parent");
        let path = dir.join("sub").join("deep").join("sh000001.day");

        let bars = vec![make_bar(2024, 1, 2, 10.0)];
        writer.write_file(&path, &bars).unwrap();
        assert!(path.exists());
        let result = reader.read_file(&path).unwrap();
        assert_eq!(result.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
