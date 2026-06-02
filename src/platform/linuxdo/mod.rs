use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;
use crate::net::HttpClient;
use crate::platform::{ListEntry, DetailView, html};

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36";

struct RssItem { title: String, link: String, description: String }

fn parse_items(rss: &str) -> Vec<RssItem> {
    let mut reader = Reader::from_str(rss);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut items = Vec::new();
    let mut in_item = false;
    let mut cur = String::new();
    let (mut title, mut link, mut desc) = (String::new(), String::new(), String::new());
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "item" { in_item = true; title.clear(); link.clear(); desc.clear(); }
                cur = name;
            }
            Ok(Event::Text(t)) if in_item => {
                let s = t.unescape().unwrap_or_default().to_string();
                match cur.as_str() { "title" => title.push_str(&s), "link" => link.push_str(&s), "description" => desc.push_str(&s), _ => {} }
            }
            Ok(Event::CData(t)) if in_item => {
                let s = String::from_utf8_lossy(t.as_ref()).to_string();
                match cur.as_str() { "title" => title.push_str(&s), "link" => link.push_str(&s), "description" => desc.push_str(&s), _ => {} }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "item" && in_item {
                    items.push(RssItem { title: title.clone(), link: link.clone(), description: desc.clone() });
                    in_item = false;
                }
                cur.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    items
}

fn parse_list(rss: &str) -> Vec<ListEntry> {
    parse_items(rss).into_iter().filter_map(|it| {
        if it.title.is_empty() || it.link.is_empty() { return None; }
        Some(ListEntry { title: it.title, subtitle: String::new(), open_token: Some(it.link), detail: None })
    }).collect()
}

fn clean_desc(d: &str) -> String {
    let mut s = d.to_string();
    for pat in ["<p><a href=\"\">阅读完整话题</a></p>", "阅读完整话题"] {
        s = s.replace(pat, "");
    }
    s
}

fn parse_detail(rss: &str) -> Vec<DetailView> {
    let items = parse_items(rss);
    if items.is_empty() {
        return vec![DetailView { author: String::new(), voteup: 0, body: String::new(), images: vec![], answer_id: String::new() }];
    }
    let mut body = String::new();
    let mut images = Vec::new();
    let main = &items[items.len() - 1];
    let (mt, mi) = html::to_text_and_images(&clean_desc(&main.description));
    body.push_str(&mt);
    images.extend(mi);
    for (idx, it) in items[..items.len() - 1].iter().rev().enumerate() {
        let (t, im) = html::to_text_and_images(&clean_desc(&it.description));
        body.push_str(&format!("\n\n── #{} ──\n{}", idx + 1, t));
        images.extend(im);
    }
    vec![DetailView { author: String::new(), voteup: (items.len() as i64 - 1).max(0), body: body.trim().to_string(), images, answer_id: String::new() }]
}

fn header_pairs(cookie: &str) -> Vec<(&'static str, String)> {
    vec![("user-agent", UA.to_string()), ("cookie", cookie.to_string()),
         ("accept", "application/xml,text/html;q=0.9,*/*;q=0.8".to_string())]
}

pub async fn list(http: &HttpClient, cookie: &str) -> Result<Vec<ListEntry>> {
    if cookie.is_empty() {
        return Ok(vec![ListEntry { title: "Linux.do 未配置 cookie(回车去配置)".into(),
            subtitle: String::new(), open_token: None, detail: None }]);
    }
    let h = header_pairs(cookie);
    let hr: Vec<(&str, &str)> = h.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let rss = http.get_text("https://linux.do/latest.rss", &hr).await?;
    Ok(parse_list(&rss))
}

pub async fn detail(http: &HttpClient, cookie: &str, token: &str) -> Result<Vec<DetailView>> {
    let url = if token.ends_with(".rss") { token.to_string() } else { format!("{}.rss", token.trim_end_matches('/')) };
    let h = header_pairs(cookie);
    let hr: Vec<(&str, &str)> = h.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let rss = http.get_text(&url, &hr).await?;
    let mut dvs = parse_detail(&rss);
    if let Some(d) = dvs.first_mut() { d.answer_id = token.to_string(); }
    Ok(dvs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linuxdo_list() {
        let rss = include_str!("../../../tests/fixtures/linuxdo_latest.rss");
        let rows = parse_list(rss);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "话题一");
        assert_eq!(rows[0].open_token.as_deref(), Some("https://linux.do/t/topic/1001"));
    }

    #[test]
    fn parses_linuxdo_detail_main_first_then_replies() {
        let rss = include_str!("../../../tests/fixtures/linuxdo_topic.rss");
        let dvs = parse_detail(rss);
        assert_eq!(dvs.len(), 1);
        let body = &dvs[0].body;
        assert!(body.contains("主楼正文"));
        assert!(body.contains("第一条回复") && body.contains("第二条回复"));
        let op = body.find("主楼正文").unwrap();
        let r1 = body.find("第一条回复").unwrap();
        let r2 = body.find("第二条回复").unwrap();
        assert!(op < r1 && r1 < r2, "order: main, then reply1, then reply2");
        assert!(body.contains("── #1 ──"));
        assert!(!body.contains("阅读完整话题"), "trailing link stripped");
    }

    #[tokio::test]
    #[ignore = "live network; needs real cookie"]
    async fn live_linuxdo_list() {
        let cfg = crate::config::Config::load().unwrap();
        if cfg.linuxdo.cookie.is_empty() { eprintln!("skip live_linuxdo_list: no linuxdo cookie"); return; }
        let c = HttpClient::new().unwrap();
        let rows = list(&c, &cfg.linuxdo.cookie).await.unwrap();
        assert!(!rows.is_empty());
        eprintln!("linuxdo[0] = {} ({:?})", rows[0].title, rows[0].open_token);
    }

    #[tokio::test]
    #[ignore = "live network; needs real cookie"]
    async fn live_linuxdo_detail() {
        let cfg = crate::config::Config::load().unwrap();
        if cfg.linuxdo.cookie.is_empty() { eprintln!("skip live_linuxdo_detail: no linuxdo cookie"); return; }
        let c = HttpClient::new().unwrap();
        let rows = list(&c, &cfg.linuxdo.cookie).await.unwrap();
        let token = rows.iter().find_map(|r| r.open_token.as_deref()).unwrap_or("https://linux.do/t/topic/1");
        let dvs = detail(&c, &cfg.linuxdo.cookie, token).await.unwrap();
        assert!(!dvs.is_empty());
        eprintln!("linuxdo detail body len={}", dvs[0].body.len());
    }
}
