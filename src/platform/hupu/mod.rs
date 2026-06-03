use anyhow::Result;
use scraper::{Html, Selector};
use crate::net::HttpClient;
use crate::platform::{ListEntry, DetailView, html};

const BASE: &str = "https://bbs.hupu.com";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/80.0.3987.132 Safari/537.36";

// Hupu's list pages have no real paging (`?page=2` returns page 1 again), so
// "load more" rotates through the `all-*` aggregation boards (each a different
// category, all no-cookie). Index 0 is the default; `r` advances it and session-
// dedup keeps only fresh threads — each board contributes ~60 new ones.
const BOARDS: [&str; 8] = [
    "all-gambia", "all-nba", "all-digital", "all-soccer", "all-ent", "all-game", "all-cba", "all-selling",
];

fn parse_list(html_str: &str) -> Vec<ListEntry> {
    let doc = Html::parse_document(html_str);
    let item_sel = Selector::parse(".text-list-model .list-item a").unwrap();
    let title_sel = Selector::parse(".t-title").unwrap();
    let mut rows = Vec::new();
    for a in doc.select(&item_sel) {
        let title = a.select(&title_sel).next()
            .map(|t| t.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let mut href = a.value().attr("href").unwrap_or("").trim().to_string();
        if !href.is_empty() && !href.starts_with("http") && !href.starts_with('/') {
            href = format!("/{href}");
        }
        if title.is_empty() || href.is_empty() { continue; }
        rows.push(ListEntry { title, subtitle: String::new(), open_token: Some(href), detail: None });
    }
    rows
}

fn strip_css_mangle(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '_' && chars.peek() == Some(&'_') {
            chars.next(); // consume 2nd '_'
            while matches!(chars.peek(), Some(ch) if ch.is_ascii_alphanumeric() || *ch == '_') {
                chars.next();
            }
            match chars.peek() {
                Some('"') => { out.push('"'); chars.next(); }
                Some(' ') => { out.push(' '); chars.next(); }
                _ => {}
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn parse_detail(html_str: &str) -> DetailView {
    let cleaned = strip_css_mangle(html_str);
    let doc = Html::parse_document(&cleaned);
    let sel = Selector::parse(".index_bbs-post-web-body-left-wrapper").unwrap();
    let inner = doc.select(&sel).next().map(|m| m.inner_html()).unwrap_or_default();
    let (body, images) = html::to_text_and_images(&inner);
    DetailView { author: String::new(), voteup: 0, body, images, answer_id: String::new() }
}

/// Fetch one aggregation board. `board` is a rotating index into `BOARDS` (wraps),
/// so successive "load more" calls walk through the categories.
pub async fn list(http: &HttpClient, board: usize) -> Result<Vec<ListEntry>> {
    let b = BOARDS[board % BOARDS.len()];
    let html_str = http.get_text(&format!("{BASE}/{b}"), &[("user-agent", UA)]).await?;
    Ok(parse_list(&html_str))
}

pub async fn detail(http: &HttpClient, token: &str) -> Result<Vec<DetailView>> {
    let mut url = if token.starts_with("http") { token.to_string() } else { format!("{BASE}{token}") };
    url = url.trim_end_matches(".html").to_string() + ".html";
    let html_str = http.get_text(&url, &[("user-agent", UA)]).await?;
    let mut dv = parse_detail(&html_str);
    dv.answer_id = token.to_string();
    Ok(vec![dv])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hupu_list() {
        let html = include_str!("../../../tests/fixtures/hupu_list.html");
        let rows = parse_list(html);
        assert!(!rows.is_empty());
        assert!(!rows[0].title.is_empty());
        assert!(rows[0].open_token.is_some());
    }

    #[test]
    fn parses_hupu_detail_strips_css_mangle() {
        let html = include_str!("../../../tests/fixtures/hupu_topic.html");
        let dv = parse_detail(html);
        assert!(!dv.body.is_empty());
    }

    #[test]
    fn de_mangle_is_utf8_safe() {
        // Chinese text must survive; __token" patterns collapse to the delimiter.
        let s = "中文__abc\"后面__def 结束";
        let out = strip_css_mangle(s);
        assert!(out.contains("中文") && out.contains("后面") && out.contains("结束"));
        assert!(!out.contains("__abc") && !out.contains("__def"));
    }

    #[tokio::test]
    #[ignore = "live network"]
    async fn live_hupu_list() {
        let c = HttpClient::new().unwrap();
        let rows = list(&c, 0).await.unwrap();
        assert!(!rows.is_empty());
        eprintln!("hupu[0] = {} ({:?})", rows[0].title, rows[0].open_token);
    }
}
