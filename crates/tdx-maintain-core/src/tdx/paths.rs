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
    if stem.len() < 3 {
        return None;
    }
    // Try splitting by '#' first (Qlib format e.g. sh#600000)
    if let Some((prefix, code)) = stem.split_once('#') {
        let market = Market::from_dir(prefix)?;
        return Some((market, code.to_string()));
    }
    // Otherwise split by first 2 characters (Standard format e.g. sh600000)
    let (prefix, code) = stem.split_at(2);
    let market = Market::from_dir(prefix)?;
    Some((market, code.to_string()))
}

pub fn get_day_filename(market: Market, symbol: &str, base_dir: &std::path::Path) -> String {
    let standard_name = format!("{}{}.day", market.dir_name(), symbol);
    let hash_name = format!("{}#{}.day", market.dir_name(), symbol);
    let path = base_dir.join(market.dir_name()).join("lday").join(&standard_name);
    if path.exists() {
        standard_name
    } else {
        hash_name
    }
}

pub fn list_day_symbols(tdx_data_dir: &std::path::Path) -> anyhow::Result<Vec<(Market, String)>> {
    let mut symbols = Vec::new();
    let base_dir = if tdx_data_dir.ends_with("vipdoc") {
        tdx_data_dir.to_path_buf()
    } else {
        tdx_data_dir.join("vipdoc")
    };
    for market in [Market::Sh, Market::Sz, Market::Bj] {
        let dir = base_dir.join(market.dir_name()).join("lday");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_dir_names() {
        assert_eq!(Market::Sh.dir_name(), "sh");
        assert_eq!(Market::Sz.dir_name(), "sz");
        assert_eq!(Market::Bj.dir_name(), "bj");
    }

    #[test]
    fn test_from_dir_valid() {
        assert_eq!(Market::from_dir("sh"), Some(Market::Sh));
        assert_eq!(Market::from_dir("SZ"), Some(Market::Sz));
        assert_eq!(Market::from_dir("Bj"), Some(Market::Bj));
    }

    #[test]
    fn test_from_dir_invalid() {
        assert_eq!(Market::from_dir("hk"), None);
        assert_eq!(Market::from_dir(""), None);
        assert_eq!(Market::from_dir("shanghai"), None);
    }

    #[test]
    fn test_parse_day_filename_standard() {
        let result = parse_day_filename("sh600000.day");
        assert!(result.is_some());
        let (market, code) = result.unwrap();
        assert_eq!(market, Market::Sh);
        assert_eq!(code, "600000");
    }

    #[test]
    fn test_parse_day_filename_hash() {
        let result = parse_day_filename("sz#000001.day");
        assert!(result.is_some());
        let (market, code) = result.unwrap();
        assert_eq!(market, Market::Sz);
        assert_eq!(code, "000001");
    }

    #[test]
    fn test_parse_day_filename_invalid() {
        assert_eq!(parse_day_filename("invalid"), None);
        assert_eq!(parse_day_filename("xx123456.day"), None);
        assert_eq!(parse_day_filename("sh12"), None); // too short
    }

    #[test]
    fn test_get_day_filename_standard_exists() {
        let dir = std::env::temp_dir().join("tdx_test_gdf");
        let lday = dir.join("sh").join("lday");
        std::fs::create_dir_all(&lday).unwrap();
        std::fs::File::create(lday.join("sh000001.day")).unwrap();

        let name = get_day_filename(Market::Sh, "000001", &dir);
        assert_eq!(name, "sh000001.day");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_get_day_filename_hash_fallback() {
        let dir = std::env::temp_dir().join("tdx_test_gdf2");
        let lday = dir.join("sh").join("lday");
        std::fs::create_dir_all(&lday).unwrap();
        std::fs::File::create(lday.join("sh#000001.day")).unwrap();

        let name = get_day_filename(Market::Sh, "000001", &dir);
        assert_eq!(name, "sh#000001.day");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
