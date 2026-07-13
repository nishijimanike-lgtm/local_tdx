use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Market {
    Sz = 0,
    Sh = 1,
    Bj = 2,
}

impl Market {
    pub fn dir_name(self) -> &'static str {
        match self {
            Market::Sz => "sz",
            Market::Sh => "sh",
            Market::Bj => "bj",
        }
    }

    pub fn from_dir(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "sz" => Some(Market::Sz),
            "sh" => Some(Market::Sh),
            "bj" => Some(Market::Bj),
            _ => None,
        }
    }
}

pub fn market_dir_name(market: Market) -> &'static str {
    market.dir_name()
}

pub fn parse_day_filename(name: &str) -> Option<(Market, String)> {
    let stem = name.strip_suffix(".day")?;
    let (prefix, code) = stem.split_once('#')?;
    let market = Market::from_dir(prefix)?;
    Ok((market, code.to_string()))
}

pub fn list_day_symbols(tdx_data_dir: &std::path::Path) -> anyhow::Result<Vec<(Market, String)>> {
    let mut symbols = Vec::new();
    for market in [Market::Sh, Market::Sz, Market::Bj] {
        let dir = tdx_data_dir.join(market.dir_name()).join("lday");
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("day") {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if let Some((m, code)) = parse_day_filename(name) {
                    symbols.push((m, code));
                }
            }
        }
    }
    Ok(symbols)
}
