use anyhow::{anyhow, Result};
use crate::net::HttpClient;
use crate::platform::{ListEntry, DetailView, CommentView};
use crate::platform::zhihu::{cookie, html, model, sign::{self, ZhihuSigner}};

pub struct ZhihuClient {
    cookie: String,
    d_c0: String,
    signer: ZhihuSigner,
    http: HttpClient,
}

impl ZhihuClient {
    pub fn new(cookie: String) -> Result<Self> {
        let d_c0 = cookie::field(&cookie, "d_c0")
            .ok_or_else(|| anyhow!("cookie missing d_c0 — re-copy it from the browser"))?;
        Ok(Self { cookie, d_c0, signer: ZhihuSigner::new()?, http: HttpClient::new()? })
    }

    async fn get(&self, path: &str) -> Result<String> {
        let sig = self.signer.sign(&sign::build_sign_input(path, &self.d_c0))?;
        self.http.signed_get(path, &self.cookie, &sig).await
    }

    pub async fn hot_list(&self) -> Result<Vec<ListEntry>> {
        let body = self.get("/api/v3/feed/topstory/hot-lists/total?limit=50&desktop=true").await?;
        let resp: model::HotListResponse = serde_json::from_str(&body)?;
        Ok(resp.data.into_iter().filter_map(|it| {
            let title = it.target.title_area.text;
            if title.is_empty() { return None; }
            let open_token = question_id_from_url(&it.target.link.url);
            Some(ListEntry { title, subtitle: it.target.excerpt_area.text, open_token, detail: None })
        }).collect())
    }

    pub async fn search(&self, query: &str) -> Result<Vec<ListEntry>> {
        let enc = urlencode(query);
        let path = format!("/api/v4/search_v3?t=general&q={enc}&offset=0&limit=20");
        let body = self.get(&path).await?;
        let resp: model::SearchResponse = serde_json::from_str(&body)?;
        Ok(resp.data.into_iter()
            .filter(|it| it.kind == "search_result")
            .filter_map(|it| {
                let obj = it.object?;
                // Prefer the highlighted title, fall back to the question name.
                let raw_title = it.highlight.map(|h| h.title).filter(|t| !t.is_empty())
                    .or_else(|| obj.question.as_ref().map(|q| q.name.clone()).filter(|t| !t.is_empty()))
                    .unwrap_or(obj.title);
                let title = strip_em(&raw_title);
                if title.is_empty() { return None; }
                let open_token = obj.question.map(|q| q.id).filter(|id| !id.is_empty());
                Some(ListEntry { title, subtitle: String::new(), open_token, detail: None })
            }).collect())
    }

    pub async fn answers(&self, question_id: &str) -> Result<Vec<DetailView>> {
        let path = format!(
            "/api/v4/questions/{question_id}/feeds?include=data%5B%2A%5D.content&limit=10&offset=0"
        );
        let body = self.get(&path).await?;
        let resp: model::AnswersResponse = serde_json::from_str(&body)?;
        Ok(resp.data.into_iter()
            // Skip non-answer feed items (ads/related questions) that have no real id.
            .filter(|f| !f.target.id.is_empty())
            .map(|f| {
                // Body keeps the inline 【图N】 markers; the clickable image list is
                // rendered by the UI from locally-downloaded paths, not baked in here.
                let (body, images) = html::to_text_and_images(&f.target.content);
                DetailView {
                    author: f.target.author.name,
                    voteup: f.target.voteup_count,
                    body,
                    images,
                    answer_id: f.target.id,
                }
            }).collect())
    }

    pub async fn recommend(&self, cursor: Option<&str>) -> Result<(Vec<ListEntry>, Option<String>)> {
        let body = self.get(&recommend_path(cursor)).await?;
        let resp: model::RecommendResponse = serde_json::from_str(&body)?;
        let next = resp.paging.and_then(|p| if p.is_end { None } else { p.next });
        let entries = resp.data.into_iter().filter_map(|item| {
            let target = item.target;
            match target.kind.as_str() {
                "answer" => {
                    let q = target.question?;
                    if q.title.is_empty() { return None; }
                    let open_token = if q.id.is_empty() { None } else { Some(q.id) };
                    let detail = if target.content.is_empty() {
                        None
                    } else {
                        let (body, images) = html::to_text_and_images(&target.content);
                        Some(DetailView {
                            author: target.author.name,
                            voteup: target.voteup_count,
                            body, images,
                            answer_id: target.id,
                        })
                    };
                    Some(ListEntry { title: q.title, subtitle: target.excerpt, open_token, detail })
                }
                "article" => {
                    let title = target.title;
                    if title.is_empty() { return None; }
                    Some(ListEntry { title, subtitle: target.excerpt, open_token: None, detail: None })
                }
                _ => None,
            }
        }).collect();
        Ok((entries, next))
    }

    pub async fn comments(&self, answer_id: &str) -> Result<Vec<CommentView>> {
        let path = format!(
            "/api/v4/comment_v5/answers/{answer_id}/root_comment?order_by=score&limit=100"
        );
        let body = self.get(&path).await?;
        let resp: model::CommentResponse = serde_json::from_str(&body)?;
        Ok(resp.data.into_iter().map(|c| CommentView {
            author: c.author.name,
            body: html::to_text(&c.content),
            like_count: c.like_count,
            child_count: c.child_comment_count,
        }).collect())
    }
}

/// Path+query for a recommend request: the server-provided `next` URL's path when
/// paging, else the default first-page path.
fn recommend_path(cursor: Option<&str>) -> String {
    match cursor {
        Some(next) if next.starts_with("http") => {
            match next.find("/api/") {
                Some(i) => next[i..].to_string(),
                None => next.to_string(),
            }
        }
        _ => "/api/v3/feed/topstory/recommend?action=down&ad_interval=-1&desktop=true".to_string(),
    }
}

/// Extract a numeric question id from a Zhihu URL like
/// `https://www.zhihu.com/question/123456`. Returns None for non-question links
/// (articles, zvideo, etc.) which can't be opened as a question feed.
fn question_id_from_url(url: &str) -> Option<String> {
    let idx = url.find("/question/")? + "/question/".len();
    let rest = &url[idx..];
    let id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if id.is_empty() { None } else { Some(id) }
}

fn urlencode(s: &str) -> String {
    s.bytes().map(|b| match b {
        b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}

fn strip_em(s: &str) -> String { s.replace("<em>", "").replace("</em>", "") }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommend_path_uses_cursor_when_present() {
        assert_eq!(
            recommend_path(None),
            "/api/v3/feed/topstory/recommend?action=down&ad_interval=-1&desktop=true"
        );
        let next = "https://www.zhihu.com/api/v3/feed/topstory/recommend?session_token=abc&after_id=5&action=down&desktop=true";
        assert_eq!(
            recommend_path(Some(next)),
            "/api/v3/feed/topstory/recommend?session_token=abc&after_id=5&action=down&desktop=true"
        );
    }

    #[test]
    fn extracts_question_id_from_url() {
        assert_eq!(question_id_from_url("https://www.zhihu.com/question/659652777").as_deref(), Some("659652777"));
        assert_eq!(question_id_from_url("https://www.zhihu.com/p/12345"), None);
    }

    #[test]
    fn strips_em_tags() {
        assert_eq!(strip_em("有没有<em>程序员</em>"), "有没有程序员");
    }

    // ---- Live integration tests (ignored by default; run explicitly) ----
    // Cookie comes from $ZHIHU_COOKIE, else from the saved config file.
    fn live_cookie() -> Option<String> {
        if let Ok(c) = std::env::var("ZHIHU_COOKIE") {
            if !c.is_empty() { return Some(c); }
        }
        let cfg = crate::config::Config::load().ok()?;
        if cfg.zhihu.cookie.is_empty() { None } else { Some(cfg.zhihu.cookie) }
    }

    #[tokio::test]
    #[ignore = "live network; run with --ignored"]
    async fn live_hot_list_and_answers_and_comments() {
        let Some(cookie) = live_cookie() else { eprintln!("skip: no cookie"); return; };
        let client = ZhihuClient::new(cookie).expect("client");
        let hot = client.hot_list().await.expect("hot list");
        assert!(!hot.is_empty(), "hot list should not be empty");
        eprintln!("hot[0] = {} (qid={:?})", hot[0].title, hot[0].open_token);

        // find a hot entry that has a question id, fetch its answers
        let qid = hot.iter().find_map(|e| e.open_token.clone()).expect("a question id in hot list");
        let answers = client.answers(&qid).await.expect("answers");
        eprintln!("got {} answers for q{}", answers.len(), qid);
        for (i, a) in answers.iter().enumerate() {
            if !a.images.is_empty() {
                eprintln!("answer[{}] has {} image(s): {:?}", i, a.images.len(), &a.images[..a.images.len().min(2)]);
            }
        }
        if let Some(a) = answers.first() {
            assert!(!a.answer_id.is_empty());
            let comments = client.comments(&a.answer_id).await.expect("comments");
            eprintln!("got {} comments for answer {}", comments.len(), a.answer_id);
        }
    }

    #[tokio::test]
    #[ignore = "live network; run with --ignored"]
    async fn live_recommend() {
        let Some(cookie) = live_cookie() else { eprintln!("skip: no cookie"); return; };
        let client = ZhihuClient::new(cookie).expect("client");
        let (results, _next) = client.recommend(None).await.expect("recommend");
        assert!(!results.is_empty(), "recommend should return entries");
        for e in &results {
            eprintln!("recommend: {} (qid={:?})", e.title, e.open_token);
        }
    }

    #[tokio::test]
    #[ignore = "live network; run with --ignored"]
    async fn live_search() {
        let Some(cookie) = live_cookie() else { eprintln!("skip: no cookie"); return; };
        let client = ZhihuClient::new(cookie).expect("client");
        let results = client.search("程序员").await.expect("search");
        assert!(!results.is_empty(), "search should return results");
        eprintln!("search[0] = {} (qid={:?})", results[0].title, results[0].open_token);
    }
}
