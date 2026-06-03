use anyhow::Result;
use scraper::{Html, Selector};
use crate::net::HttpClient;
use crate::platform::{ListEntry, DetailView, html};

const BASE: &str = "https://www.v2ex.com";

// V2EX's homepage has no `?p=2` paging and `/recent` needs login, so "load more"
// rotates through the public homepage tabs (each a different node category, all
// no-cookie). Index 0 ("all") is the default first page; `r` advances the index
// and session-dedup keeps only topics not yet shown. "hot" is omitted on purpose:
// it's a subset of "all" (~90% overlap), so it'd add almost nothing; the category
// tabs each contribute ~35-50 fresh topics instead.
const TABS: [&str; 8] = ["all", "tech", "creative", "play", "apple", "jobs", "deals", "qna"];

fn parse_list(html_str: &str) -> Vec<ListEntry> {
    let doc = Html::parse_document(html_str);
    let link_sel = Selector::parse("#Main .box .cell.item .topic-link").unwrap();
    let mut rows = Vec::new();
    for a in doc.select(&link_sel) {
        let title = a.text().collect::<String>().trim().to_string();
        let href = a.value().attr("href").unwrap_or("").trim();
        let token = href.split(['#', '?']).next().unwrap_or("").to_string();
        if title.is_empty() || token.is_empty() { continue; }
        rows.push(ListEntry { title, subtitle: String::new(), open_token: Some(token), detail: None });
    }
    rows
}

fn parse_detail(html_str: &str) -> DetailView {
    let doc = Html::parse_document(html_str);
    let main_sel = Selector::parse("#Main").unwrap();
    let inner = doc.select(&main_sel).next().map(|m| m.inner_html()).unwrap_or_default();
    let (body, images) = html::to_text_and_images(&inner);
    DetailView { author: String::new(), voteup: 0, body, images, answer_id: String::new() }
}

/// Fetch one homepage tab. `tab` is a rotating index into `TABS` (wraps), so
/// successive "load more" calls walk through the categories.
pub async fn list(http: &HttpClient, tab: usize) -> Result<Vec<ListEntry>> {
    let t = TABS[tab % TABS.len()];
    let html_str = http.get_text(&format!("{BASE}/?tab={t}"), &[]).await?;
    Ok(parse_list(&html_str))
}

pub async fn detail(http: &HttpClient, token: &str) -> Result<Vec<DetailView>> {
    let url = if token.starts_with("http") { token.to_string() } else { format!("{BASE}{token}") };
    let html_str = http.get_text(&url, &[]).await?;
    let mut dv = parse_detail(&html_str);
    dv.answer_id = token.to_string();
    Ok(vec![dv])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v2ex_list() {
        let html = include_str!("../../../tests/fixtures/v2ex_list.html");
        let rows = parse_list(html);
        assert!(!rows.is_empty(), "should parse topic rows");
        let r = &rows[0];
        assert!(!r.title.is_empty());
        assert!(r.open_token.as_deref().unwrap().starts_with("/t/"));
        assert!(r.detail.is_none());
    }

    #[test]
    fn parses_v2ex_detail() {
        let html = include_str!("../../../tests/fixtures/v2ex_topic.html");
        let dv = parse_detail(html);
        assert!(!dv.body.is_empty());
    }

    #[tokio::test]
    #[ignore = "live network"]
    async fn live_v2ex_list() {
        let c = HttpClient::new().unwrap();
        let rows = list(&c, 0).await.unwrap();
        assert!(!rows.is_empty());
        eprintln!("v2ex[0] = {} ({:?})", rows[0].title, rows[0].open_token);
    }
}
