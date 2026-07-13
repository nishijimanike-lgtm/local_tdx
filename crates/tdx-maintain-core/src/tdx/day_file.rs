use chrono::NaiveDate;
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

    pub fn last_date(&self, path: &Path) -> anyhow::Result<Option<NaiveDate>> {
        let bars = self.read_file(path)?;
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
}
