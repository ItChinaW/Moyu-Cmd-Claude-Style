use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;
use crate::net::HttpClient;
use crate::platform::{ListEntry, DetailView};

const BASE: &str = "https://bbs.nga.cn";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";
pub const DEFAULT_FID: &str = "-7";

// NGA's lite=xml declares encoding="GB18030" (a superset of GBK); decode with
// GB18030 so rare chars outside GBK don't mojibake. The decoder also accepts GBK.
fn gbk_to_string(bytes: &[u8]) -> String { let (c, _, _) = encoding_rs::GB18030.decode(bytes); c.into_owned() }

/// Drop every BBCode token: [tag], [/tag], smileys [s:..:..]. Keeps inner text.
fn clean_bbcode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0u32;
    for ch in s.chars() {
        match ch {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

/// Scan [img]..[/img], strip leading "./", absolutize to the NGA attachment CDN
/// unless the path is already an absolute http(s) URL.
fn extract_nga_images(s: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find("[img]") {
        let after = &rest[open + 5..];
        let Some(close) = after.find("[/img]") else { break };
        let raw = after[..close].trim();
        if !raw.is_empty() {
            let url = if raw.starts_with("http://") || raw.starts_with("https://") {
                raw.to_string()
            } else {
                let p = raw.trim_start_matches("./").trim_start_matches('/');
                format!("https://img.nga.178.com/attachments/{p}")
            };
            urls.push(url);
        }
        rest = &after[close + 6..];
    }
    urls
}

/// Collect <item> rows under <__T>, reading <tid>/<subject>/<replies>.
fn parse_list(xml: &str) -> Vec<ListEntry> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut rows = Vec::new();
    let mut in_item = false;
    let mut cur = String::new(); // current leaf tag name
    let (mut tid, mut subject, mut replies) = (String::new(), String::new(), String::new());
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "item" {
                    in_item = true;
                    tid.clear(); subject.clear(); replies.clear();
                } else if in_item {
                    cur = name;
                }
            }
            Ok(Event::Text(t)) if in_item => {
                let txt = t.unescape().unwrap_or_default().to_string();
                match cur.as_str() {
                    "tid" => tid.push_str(&txt),
                    "subject" => subject.push_str(&txt),
                    "replies" => replies.push_str(&txt),
                    _ => {}
                }
            }
            Ok(Event::CData(t)) if in_item => {
                let txt = String::from_utf8_lossy(t.as_ref()).to_string();
                match cur.as_str() {
                    "tid" => tid.push_str(&txt),
                    "subject" => subject.push_str(&txt),
                    "replies" => replies.push_str(&txt),
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "item" {
                    in_item = false;
                    if !tid.is_empty() {
                        let r = replies.trim();
                        let title = format!("[{}] {}", if r.is_empty() { "0" } else { r }, subject.trim());
                        rows.push(ListEntry {
                            title,
                            subtitle: String::new(),
                            open_token: Some(format!("/read.php?tid={}", tid.trim())),
                            detail: None,
                        });
                    }
                } else if in_item {
                    cur.clear();
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    rows
}

/// Collect every <item>'s <content>, clean BBCode, concat floors into one body
/// (main post first with no marker; replies separated by "── #N ──"), gather images
/// and append 【图N】 markers. Returns a single-element Vec (thread = one detail).
fn parse_detail(xml: &str) -> Vec<DetailView> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut floors: Vec<String> = Vec::new();
    let mut in_item = false;
    let mut in_content = false;
    let mut content = String::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "item" {
                    in_item = true;
                    content.clear();
                } else if in_item && name == "content" {
                    in_content = true;
                }
            }
            Ok(Event::Text(t)) if in_content => {
                content.push_str(&t.unescape().unwrap_or_default());
            }
            Ok(Event::CData(t)) if in_content => {
                content.push_str(&String::from_utf8_lossy(t.as_ref()));
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "content" {
                    in_content = false;
                } else if name == "item" {
                    in_item = false;
                    floors.push(content.clone());
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    let mut images: Vec<String> = Vec::new();
    let mut body = String::new();
    for (idx, raw) in floors.iter().enumerate() {
        images.extend(extract_nga_images(raw));
        let text = clean_bbcode(raw);
        if idx == 0 {
            body.push_str(text.trim());
        } else {
            body.push_str(&format!("\n\n── #{idx} ──\n{}", text.trim()));
        }
    }
    for n in 1..=images.len() {
        body.push_str(&format!("\n【图{n}】"));
    }

    vec![DetailView {
        author: String::new(),
        voteup: floors.len() as i64,
        body: body.trim().to_string(),
        images,
        answer_id: String::new(),
    }]
}

pub async fn list(http: &HttpClient, cookie: &str, page: u32) -> Result<Vec<ListEntry>> {
    if cookie.is_empty() {
        return Ok(vec![ListEntry { title: "NGA 未配置 cookie(回车去配置)".into(),
            subtitle: String::new(), open_token: None, detail: None }]);
    }
    let p = if page == 0 { 1 } else { page };
    let url = format!("{BASE}/thread.php?fid={DEFAULT_FID}&page={p}&lite=xml");
    let bytes = http.get_bytes(&url, &[("cookie", cookie), ("user-agent", UA)]).await?;
    Ok(parse_list(&gbk_to_string(&bytes)))
}

pub async fn detail(http: &HttpClient, cookie: &str, token: &str) -> Result<Vec<DetailView>> {
    let tid = token.split("tid=").nth(1).map(|s| s.split('&').next().unwrap_or(s)).unwrap_or("");
    if tid.is_empty() { anyhow::bail!("无法解析 NGA tid"); }
    let url = format!("{BASE}/read.php?tid={tid}&lite=xml");
    let bytes = http.get_bytes(&url, &[("cookie", cookie), ("user-agent", UA)]).await?;
    let mut dvs = parse_detail(&gbk_to_string(&bytes));
    if let Some(d) = dvs.first_mut() { d.answer_id = tid.to_string(); }
    Ok(dvs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_gbk_bytes() {
        let bytes = [0xD6u8, 0xD0, 0xCE, 0xC4]; // GBK for "中文"
        assert_eq!(gbk_to_string(&bytes), "中文");
    }

    #[test]
    fn parses_nga_list() {
        let xml = include_str!("../../../tests/fixtures/nga_list.xml");
        let rows = parse_list(xml);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].title.starts_with("[42] "));
        assert_eq!(rows[0].open_token.as_deref(), Some("/read.php?tid=40000001"));
    }

    #[test]
    fn cleans_nga_bbcode() {
        let out = clean_bbcode("看[b]这里[/b][quote]引用[/quote][s:ac:茶]结束");
        assert!(!out.contains("[b]") && !out.contains("[/quote]") && !out.contains("[s:"));
        assert!(out.contains("看") && out.contains("结束"));
    }

    #[test]
    fn extracts_nga_image_urls() {
        let urls = extract_nga_images("x[img]./mon/aa.jpg[/img]y[img]/bb.png[/img]");
        assert_eq!(urls.len(), 2);
        assert!(urls[0].starts_with("https://img.nga.178.com/attachments/"));
        assert!(urls[0].ends_with("mon/aa.jpg"));
    }

    #[test]
    fn parses_nga_detail_concats_floors() {
        let xml = include_str!("../../../tests/fixtures/nga_topic.xml");
        let dvs = parse_detail(xml);
        assert_eq!(dvs.len(), 1, "thread = single detail");
        assert!(!dvs[0].body.is_empty());
        assert!(dvs[0].body.contains("── #1 ──"), "reply floor marker present");
        assert!(dvs[0].body.contains("【图1】"), "image marker present");
        assert_eq!(dvs[0].images.len(), 1);
    }

    #[tokio::test]
    #[ignore = "live network; needs real cookie"]
    async fn live_nga_list() {
        let cfg = crate::config::Config::load().unwrap();
        if cfg.nga.cookie.is_empty() { eprintln!("skip live_nga_list: no nga cookie"); return; }
        let c = HttpClient::new().unwrap();
        let rows = list(&c, &cfg.nga.cookie, 1).await.unwrap();
        assert!(!rows.is_empty());
        eprintln!("nga[0] = {} ({:?})", rows[0].title, rows[0].open_token);
    }

    #[tokio::test]
    #[ignore = "live network; needs real cookie"]
    async fn live_nga_detail() {
        let cfg = crate::config::Config::load().unwrap();
        if cfg.nga.cookie.is_empty() { eprintln!("skip live_nga_detail: no nga cookie"); return; }
        let c = HttpClient::new().unwrap();
        let rows = list(&c, &cfg.nga.cookie, 1).await.unwrap();
        let token = rows.iter().find_map(|r| r.open_token.clone()).expect("a token");
        let dvs = detail(&c, &cfg.nga.cookie, &token).await.unwrap();
        assert!(!dvs.is_empty());
        eprintln!("nga detail floors={} body_len={}", dvs[0].voteup, dvs[0].body.len());
    }
}
