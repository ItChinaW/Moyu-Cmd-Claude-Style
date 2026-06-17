use anyhow::Result;
use scraper::{Html, Selector};
use serde::Deserialize;

use crate::net::HttpClient;
use crate::platform::{DetailView, ListEntry, html};

const BASE: &str = "https://tieba.baidu.com";
const MOBILE_BASE: &str = "https://tieba.baidu.com/mo/q";
const UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";

#[derive(Debug, Default, Deserialize)]
struct HotMessageResponse {
    #[serde(default)]
    no: i64,
    #[serde(default)]
    error: String,
    #[serde(default)]
    data: HotMessageData,
}

#[derive(Debug, Default, Deserialize)]
struct HotMessageData {
    #[serde(default)]
    info: Vec<FeedItem>,
}

#[derive(Debug, Default, Deserialize)]
struct FeedItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    fname: String,
    #[serde(default)]
    forum_name: String,
    #[serde(default)]
    forumname: String,
    #[serde(default)]
    thread_id: String,
    #[serde(default)]
    tid: String,
    #[serde(default)]
    detail_url: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    abs: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    content: String,
}

fn forum_name(it: &FeedItem) -> String {
    for s in [&it.fname, &it.forum_name, &it.forumname] {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    String::new()
}

fn thread_title(it: &FeedItem) -> String {
    for s in [&it.title, &it.summary, &it.abs, &it.content] {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    String::new()
}

fn thread_id(it: &FeedItem) -> String {
    for s in [&it.thread_id, &it.tid] {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    String::new()
}

fn detail_url(it: &FeedItem) -> String {
    for s in [&it.detail_url, &it.url] {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return absolutize(trimmed);
        }
    }
    let tid = thread_id(it);
    if tid.is_empty() {
        String::new()
    } else {
        format!("{BASE}/p/{tid}")
    }
}

fn absolutize(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else if url.starts_with('/') {
        format!("{BASE}{url}")
    } else {
        format!("{BASE}/{url}")
    }
}

fn parse_list(json: &str) -> Result<Vec<ListEntry>> {
    let resp: HotMessageResponse = serde_json::from_str(json)?;
    if resp.no != 0 {
        anyhow::bail!(
            "贴吧首页推送接口返回异常: {}",
            if resp.error.is_empty() { format!("no={}", resp.no) } else { resp.error }
        );
    }
    let mut out = Vec::new();
    for it in resp.data.info {
        let forum = forum_name(&it);
        let title = thread_title(&it);
        let url = detail_url(&it);
        if forum.is_empty() || title.is_empty() || url.is_empty() {
            continue;
        }
        out.push(ListEntry {
            title: format!("《{}》- {}", forum, title),
            subtitle: String::new(),
            open_token: Some(url),
            detail: None,
        });
    }
    Ok(out)
}

fn parse_detail(html_str: &str, token: &str) -> DetailView {
    let doc = Html::parse_document(html_str);
    let selectors = [
        ".i",
        ".s_post",
        ".content",
        "#pblist .list",
        ".pb_content",
    ];
    let mut body = String::new();
    let mut images = Vec::new();
    for css in selectors {
        let sel = Selector::parse(css).unwrap();
        let nodes: Vec<_> = doc.select(&sel).collect();
        if nodes.is_empty() {
            continue;
        }
        for (idx, node) in nodes.iter().enumerate() {
            let (text, mut imgs) = html::to_text_and_images(&node.inner_html());
            let text = text.trim();
            if text.is_empty() && imgs.is_empty() {
                continue;
            }
            if !body.is_empty() {
                body.push_str(&format!("\n\n── #{} ──\n", idx + 1));
            }
            body.push_str(text);
            images.append(&mut imgs);
        }
        if !body.trim().is_empty() {
            break;
        }
    }
    if body.trim().is_empty() {
        body = format!("贴吧原帖链接：{token}\n\n当前环境下贴吧正文页容易触发安全验证，列表采集正常时可先复制链接到浏览器查看。");
    }
    DetailView {
        author: String::new(),
        voteup: 0,
        body: body.trim().to_string(),
        images,
        answer_id: token.to_string(),
    }
}

fn header_pairs(cookie: &str) -> Vec<(&'static str, String)> {
    vec![
        ("user-agent", UA.to_string()),
        ("cookie", cookie.to_string()),
        ("accept", "application/json,text/plain,*/*".to_string()),
        ("referer", format!("{BASE}/")),
    ]
}

pub async fn list(http: &HttpClient, cookie: &str, _page: u32) -> Result<Vec<ListEntry>> {
    if cookie.is_empty() {
        return Ok(vec![ListEntry {
            title: "贴吧 未配置 cookie(回车去配置)".into(),
            subtitle: String::new(),
            open_token: None,
            detail: None,
        }]);
    }
    let h = header_pairs(cookie);
    let hr: Vec<(&str, &str)> = h.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let json = http
        .get_text(&format!("{MOBILE_BASE}/newmoindex"), &hr)
        .await?;
    parse_list(&json)
}

pub async fn detail(http: &HttpClient, cookie: &str, token: &str) -> Result<Vec<DetailView>> {
    let h = header_pairs(cookie);
    let hr: Vec<(&str, &str)> = h.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let mobile_url = if let Some(tid) = token.split("/p/").nth(1) {
        let tid = tid.split(['?', '#', '/']).next().unwrap_or("");
        if tid.is_empty() {
            token.to_string()
        } else {
            format!("{MOBILE_BASE}/m?kz={tid}&word=&has_received=1")
        }
    } else {
        token.to_string()
    };
    let html = http.get_text(&mobile_url, &hr).await.unwrap_or_default();
    Ok(vec![parse_detail(&html, token)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tieba_list() {
        let raw = r#"{
          "no": 0,
          "data": {
            "info": [
              {
                "title": "今天也要努力",
                "fname": "Rust吧",
                "thread_id": "123",
                "detail_url": "https://tieba.baidu.com/p/123"
              },
              {
                "title": "  ",
                "fname": "空标题吧",
                "thread_id": "456",
                "detail_url": "https://tieba.baidu.com/p/456"
              }
            ]
          }
        }"#;
        let rows = parse_list(raw).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "《Rust吧》- 今天也要努力");
        assert_eq!(rows[0].open_token.as_deref(), Some("https://tieba.baidu.com/p/123"));
    }

    #[test]
    fn parse_list_bubbles_api_error() {
        let raw = r#"{"no":1,"error":"not logined!","data":{}}"#;
        let err = parse_list(raw).unwrap_err().to_string();
        assert!(err.contains("not logined"));
    }

    #[test]
    fn parses_tieba_detail_and_falls_back() {
        let html = r#"
        <html><body>
          <div class="i"><p>主楼正文</p><img src="https://img/a.jpg" /></div>
          <div class="i"><p>回复一</p></div>
        </body></html>
        "#;
        let dv = parse_detail(html, "https://tieba.baidu.com/p/123");
        assert!(dv.body.contains("主楼正文"));
        assert!(dv.body.contains("回复一"));
        assert_eq!(dv.images.len(), 1);

        let fallback = parse_detail("<html></html>", "https://tieba.baidu.com/p/123");
        assert!(fallback.body.contains("贴吧原帖链接"));
    }
}
