use anyhow::Result;

use crate::config::{Config, StockWatchItem};
use crate::net::HttpClient;
use crate::platform::{DetailView, ListEntry};

const SINA_REFERER: &str = "https://finance.sina.com.cn";

#[derive(Debug, Clone, PartialEq)]
pub struct QuoteItem {
    pub symbol: String,
    pub name: String,
    pub price: f64,
    pub change: f64,
    pub change_percent: f64,
    pub previous_close: f64,
    pub extended_price: Option<f64>,
    pub extended_change_percent: Option<f64>,
    pub extended_source_ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Market {
    AShare,
    Us,
}

impl Market {
    pub fn detect(code: &str) -> Self {
        if code.chars().all(|c| c.is_ascii_digit()) {
            Market::AShare
        } else {
            Market::Us
        }
    }
}

pub fn normalize_code(code: &str) -> String {
    let trimmed = code.trim();
    if Market::detect(trimmed) == Market::AShare {
        trimmed.to_string()
    } else {
        trimmed.to_uppercase()
    }
}

fn to_sina_symbol(symbol: &str) -> String {
    let s = symbol.to_lowercase();
    if s.starts_with("sh") || s.starts_with("sz") || s.starts_with("bj") || s.starts_with("gb_") {
        return s;
    }
    if symbol.starts_with('.') {
        return format!("gb_{}", symbol.trim_start_matches('.').to_lowercase());
    }
    if !symbol.chars().all(|c| c.is_ascii_digit()) {
        return format!("gb_{}", symbol.to_lowercase());
    }
    let is_sh = symbol.starts_with('6') || symbol.starts_with('5') || symbol.starts_with("11");
    format!("{}{}", if is_sh { "sh" } else { "sz" }, symbol)
}

fn parse_sina_line(raw_symbol: &str, line: &str) -> Option<QuoteItem> {
    let caps = line.match_indices('"').collect::<Vec<_>>();
    if caps.len() < 2 {
        return None;
    }
    let start = caps[0].0 + 1;
    let end = caps[1].0;
    let data = &line[start..end];
    let parts: Vec<&str> = data.split(',').collect();
    let market = Market::detect(raw_symbol);
    match market {
        Market::AShare => {
            let name = parts.first()?.trim().to_string();
            let prev_close = parts.get(2)?.parse::<f64>().ok()?;
            let mut price = parts.get(3)?.parse::<f64>().ok()?;
            if price == 0.0 && prev_close > 0.0 {
                price = prev_close;
            }
            if name.is_empty() || price <= 0.0 {
                return None;
            }
            let change = price - prev_close;
            let change_percent = if prev_close > 0.0 { change / prev_close * 100.0 } else { 0.0 };
            Some(QuoteItem {
                symbol: normalize_code(raw_symbol),
                name,
                price,
                change,
                change_percent,
                previous_close: prev_close,
                extended_price: None,
                extended_change_percent: None,
                extended_source_ready: true,
            })
        }
        Market::Us => {
            let name = parts.first()?.trim().to_string();
            let price = parts.get(1)?.parse::<f64>().ok()?;
            let change_percent = parts.get(2)?.parse::<f64>().ok()?;
            let change = parts.get(4)?.parse::<f64>().ok()?;
            if name.is_empty() || price <= 0.0 {
                return None;
            }
            Some(QuoteItem {
                symbol: normalize_code(raw_symbol),
                name,
                price,
                change,
                change_percent,
                previous_close: price - change,
                extended_price: None,
                extended_change_percent: None,
                extended_source_ready: true,
            })
        }
    }
}

pub async fn fetch_quotes(http: &HttpClient, items: &[StockWatchItem], _force: bool) -> Result<Vec<QuoteItem>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let symbols: Vec<String> = items.iter().map(|i| normalize_code(&i.code)).collect();
    let sina_symbols = symbols.iter().map(|s| to_sina_symbol(s)).collect::<Vec<_>>().join(",");
    let url = format!("https://hq.sinajs.cn/list={sina_symbols}");
    let bytes = http
        .get_bytes(&url, &[("referer", SINA_REFERER), ("user-agent", crate::net::USER_AGENT)])
        .await?;
    let (text, _, _) = encoding_rs::GBK.decode(&bytes);
    let mut quotes = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let Some(code) = symbols.get(idx) else { continue };
        if let Some(mut q) = parse_sina_line(code, line) {
            if q.name == code.as_str() {
                if let Some(saved) = items.iter().find(|i| normalize_code(&i.code) == *code) {
                    if !saved.name.is_empty() {
                        q.name = saved.name.clone();
                    }
                }
            }
            quotes.push(q);
        } else if Market::detect(code) == Market::Us {
            quotes.push(QuoteItem {
                symbol: code.clone(),
                name: code.clone(),
                price: 0.0,
                change: 0.0,
                change_percent: 0.0,
                previous_close: 0.0,
                extended_price: None,
                extended_change_percent: None,
                extended_source_ready: false,
            });
        }
    }
    Ok(quotes)
}

fn fmt_pct(pct: f64) -> String {
    format!("{:+.2}%", pct)
}

fn fmt_price(v: f64) -> String {
    format!("{v:.3}")
}

pub fn quote_to_entry(q: &QuoteItem) -> ListEntry {
    let title = match Market::detect(&q.symbol) {
        Market::AShare => format!("{}（{}）{} 【{}】", q.name, q.symbol, fmt_price(q.price), fmt_pct(q.change_percent)),
        Market::Us => {
            if q.price <= 0.0 && q.extended_price.is_none() {
                return ListEntry {
                    title: q.symbol.clone(),
                    subtitle: "等待 WS 数据...".into(),
                    open_token: Some(q.symbol.clone()),
                    detail: Some(DetailView {
                        author: String::new(),
                        voteup: 0,
                        body: format!("{}\n代码: {}\n等待 WS 数据...", q.symbol, q.symbol),
                        images: Vec::new(),
                        answer_id: q.symbol.clone(),
                    }),
                };
            }
            let ext = q.extended_price.unwrap_or(q.price);
            let ext_pct = q.extended_change_percent.unwrap_or(q.change_percent);
            format!(
                "{} ({:.3} {}) {} 【{}】",
                q.symbol,
                ext,
                fmt_pct(ext_pct),
                fmt_price(q.price),
                fmt_pct(q.change_percent)
            )
        }
    };
    let subtitle = match Market::detect(&q.symbol) {
        Market::AShare => q.name.clone(),
        Market::Us => String::new(),
    };
    let body = format!(
        "{}\n代码: {}\n现价: {}\n涨跌额: {:+.3}\n涨跌幅: {}\n昨收: {}\n{}\n{}",
        q.name,
        q.symbol,
        fmt_price(q.price),
        q.change,
        fmt_pct(q.change_percent),
        fmt_price(q.previous_close),
        if let (Some(ext_price), Some(ext_pct)) = (q.extended_price, q.extended_change_percent) {
            format!("盘前/盘后: {} ({})", fmt_price(ext_price), fmt_pct(ext_pct))
        } else {
            "盘前/盘后: 无".to_string()
        },
        if !q.extended_source_ready && Market::detect(&q.symbol) == Market::Us {
            "夜盘抓取未就绪，当前已回退普通行情接口".to_string()
        } else {
            String::new()
        }
    );
    ListEntry {
        title,
        subtitle,
        open_token: Some(q.symbol.clone()),
        detail: Some(DetailView {
            author: String::new(),
            voteup: 0,
            body,
            images: Vec::new(),
            answer_id: q.symbol.clone(),
        }),
    }
}

pub fn load_watchlist() -> Result<Vec<StockWatchItem>> {
    let cfg = Config::load()?;
    Ok(cfg.stock.watchlist)
}

pub fn save_watchlist(items: Vec<StockWatchItem>) -> Result<()> {
    let mut cfg = Config::load()?;
    cfg.stock.watchlist = items;
    cfg.save()
}

pub fn add_watch_many(codes: &[String]) -> Result<Vec<StockWatchItem>> {
    let mut items = load_watchlist()?;
    let mut changed = false;
    for code in codes {
        let code = normalize_code(code);
        if !items.iter().any(|i| normalize_code(&i.code) == code) {
            items.push(StockWatchItem { code, name: String::new() });
            changed = true;
        }
    }
    if changed {
        save_watchlist(items.clone())?;
    }
    Ok(items)
}

pub fn delete_watch(code: &str) -> Result<Vec<StockWatchItem>> {
    let code = normalize_code(code);
    let mut items = load_watchlist()?;
    items.retain(|i| normalize_code(&i.code) != code);
    save_watchlist(items.clone())?;
    Ok(items)
}

pub fn sync_names(items: &mut [StockWatchItem], quotes: &[QuoteItem]) -> bool {
    let mut changed = false;
    for item in items.iter_mut() {
        let code = normalize_code(&item.code);
        if let Some(q) = quotes.iter().find(|q| q.symbol == code) {
            if item.name != q.name {
                item.name = q.name.clone();
                changed = true;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_codes_by_market() {
        assert_eq!(normalize_code("spcx"), "SPCX");
        assert_eq!(normalize_code("159941"), "159941");
    }

    #[test]
    fn parses_a_share_line() {
        let line = r#"var hq_str_sz159941="纳指ETF,1.690,1.698,1.664,1.706,1.658,1.663,1.664,31234567,52123456.000,12300,1.663,45600,1.662,2026-06-18,15:00:00,00";"#;
        let q = parse_sina_line("159941", line).unwrap();
        assert_eq!(q.name, "纳指ETF");
        assert_eq!(q.symbol, "159941");
        assert_eq!(q.price, 1.664);
    }

    #[test]
    fn parses_us_line() {
        let line = r#"var hq_str_gb_spcx="SPCX,191.820,-4.95,2026-06-17 16:00:00,-9.970,198.000,192.330,189.700,10234,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,SPCX";"#;
        let q = parse_sina_line("SPCX", line).unwrap();
        assert_eq!(q.symbol, "SPCX");
        assert_eq!(q.price, 191.820);
        assert_eq!(q.change_percent, -4.95);
    }

    #[test]
    fn formats_entries() {
        let a = QuoteItem {
            symbol: "159941".into(),
            name: "纳指ETF".into(),
            price: 1.664,
            change: -0.034,
            change_percent: -2.0,
            previous_close: 1.698,
            extended_price: None,
            extended_change_percent: None,
            extended_source_ready: true,
        };
        assert!(quote_to_entry(&a).title.contains("纳指ETF（159941）1.664 【-2.00%】"));

        let us = QuoteItem {
            symbol: "SPCX".into(),
            name: "SPCX".into(),
            price: 191.820,
            change: -9.97,
            change_percent: -4.95,
            previous_close: 201.79,
            extended_price: Some(191.450),
            extended_change_percent: Some(-0.2),
            extended_source_ready: true,
        };
        assert!(quote_to_entry(&us).title.contains("SPCX (191.450 -0.20%) 191.820 【-4.95%】"));

    }
}
