use serde::Deserialize;

// ---------- Hot list ----------
// data[].target = { title_area:{text}, excerpt_area:{text}, link:{url} }
#[derive(Debug, Deserialize)]
pub struct HotListResponse {
    pub data: Vec<HotItem>,
}

#[derive(Debug, Deserialize)]
pub struct HotItem {
    pub target: HotTarget,
}

#[derive(Debug, Default, Deserialize)]
pub struct HotTarget {
    #[serde(default)]
    pub title_area: TextArea,
    #[serde(default)]
    pub excerpt_area: TextArea,
    #[serde(default)]
    pub link: LinkArea,
}

#[derive(Debug, Default, Deserialize)]
pub struct TextArea {
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct LinkArea {
    #[serde(default)]
    pub url: String,
}

// ---------- Recommend feed ----------
// GET /api/v3/feed/topstory/recommend
// data[].target = answer | article
//   answer: { type:"answer", question:{id:String, title}, excerpt, author:{name}, voteup_count }
//   article: { type:"article", title, excerpt }
#[derive(Debug, Deserialize)]
pub struct RecommendResponse {
    pub data: Vec<RecommendItem>,
}

#[derive(Debug, Deserialize)]
pub struct RecommendItem {
    pub target: RecommendTarget,
}

#[derive(Debug, Default, Deserialize)]
pub struct RecommendTarget {
    #[serde(rename = "type", default)]
    pub kind: String,
    /// Populated for articles.
    #[serde(default)]
    pub title: String,
    /// Populated for answers and articles.
    #[serde(default)]
    pub excerpt: String,
    /// Populated for answers; contains the question title and id.
    #[serde(default)]
    pub question: Option<RecommendQuestion>,
    /// Answer id — lets us open exactly the answer this card previewed.
    #[serde(default)]
    pub id: String,
    /// Full answer HTML (answers carry it inline in the recommend feed).
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub voteup_count: i64,
    #[serde(default)]
    pub author: Author,
}

#[derive(Debug, Deserialize)]
pub struct RecommendQuestion {
    /// Numeric string (e.g. "665563068").
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
}

// ---------- Search ----------
// data[] = { type:"search_result"|"hot_timing"|..., highlight:{title}?, object:{...}? }
// only type=="search_result" carries a real answer object.
// object.question.id is a STRING. titles contain <em>..</em> tags.
#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    pub data: Vec<SearchItem>,
}

#[derive(Debug, Deserialize)]
pub struct SearchItem {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub highlight: Option<Highlight>,
    #[serde(default)]
    pub object: Option<SearchObject>,
}

#[derive(Debug, Deserialize)]
pub struct Highlight {
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct SearchObject {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub question: Option<SearchQuestion>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuestion {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
}

// ---------- Answers ----------
// data[].target = { id:String, content:HTML, voteup_count:i64, author:{name} }
// NOTE: id is a JSON string (large integer) in the real fixture.
#[derive(Debug, Deserialize)]
pub struct AnswersResponse {
    pub data: Vec<AnswerFeed>,
}

#[derive(Debug, Deserialize)]
pub struct AnswerFeed {
    pub target: Answer,
}

#[derive(Debug, Deserialize)]
pub struct Answer {
    pub id: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub voteup_count: i64,
    #[serde(default)]
    pub author: Author,
}

#[derive(Debug, Default, Deserialize)]
pub struct Author {
    #[serde(default)]
    pub name: String,
}

// ---------- Comments ----------
// data[] = { id:String, content:HTML, like_count:i64, child_comment_count:i64, author:{name} }
// NOTE: id is a JSON string in the real fixture.
// NOTE: author.name is FLAT (not author.member.name).
#[derive(Debug, Deserialize)]
pub struct CommentResponse {
    pub data: Vec<Comment>,
}

#[derive(Debug, Deserialize)]
pub struct Comment {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub like_count: i64,
    #[serde(default)]
    pub child_comment_count: i64,
    #[serde(default)]
    pub author: CommentAuthor,
}

#[derive(Debug, Default, Deserialize)]
pub struct CommentAuthor {
    #[serde(default)]
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_recommend() {
        let raw = include_str!("../../../tests/fixtures/recommend.json");
        let r: RecommendResponse = serde_json::from_str(raw).expect("parse recommend");
        assert!(!r.data.is_empty(), "recommend must have items");
        let first_answer = r.data.iter().find(|it| it.target.kind == "answer")
            .expect("at least one answer item");
        let q = first_answer.target.question.as_ref().expect("answer must have a question");
        assert!(!q.title.is_empty(), "question must have a title");
        assert!(!q.id.is_empty(), "question must have an id");
    }

    #[test]
    fn parses_recommend_answer_inline_content() {
        // The recommend feed carries the answer's own id/content/author/votes inline,
        // which we use to open exactly the previewed answer.
        let raw = r#"{"data":[{"target":{"type":"answer","id":"555","content":"<p>正文</p>","voteup_count":12,"excerpt":"摘要","author":{"name":"老王"},"question":{"id":"100","title":"标题"}}}]}"#;
        let r: RecommendResponse = serde_json::from_str(raw).expect("parse");
        let t = &r.data[0].target;
        assert_eq!(t.id, "555");
        assert_eq!(t.content, "<p>正文</p>");
        assert_eq!(t.voteup_count, 12);
        assert_eq!(t.author.name, "老王");
    }

    #[test]
    fn parses_hot_list() {
        let raw = include_str!("../../../tests/fixtures/hot_list.json");
        let r: HotListResponse = serde_json::from_str(raw).expect("parse hot_list");
        assert!(!r.data.is_empty());
        assert!(
            !r.data[0].target.title_area.text.is_empty(),
            "hot item must have a title"
        );
        assert!(
            r.data[0].target.link.url.contains("zhihu.com"),
            "hot item must have a link url"
        );
    }

    #[test]
    fn parses_search() {
        let raw = include_str!("../../../tests/fixtures/search.json");
        let r: SearchResponse = serde_json::from_str(raw).expect("parse search");
        let results: Vec<_> = r.data.iter().filter(|i| i.kind == "search_result").collect();
        assert!(!results.is_empty(), "expected some search_result items");
        assert!(results[0].object.is_some(), "search_result must carry an object");
    }

    #[test]
    fn parses_answers() {
        let raw = include_str!("../../../tests/fixtures/answers.json");
        let r: AnswersResponse = serde_json::from_str(raw).expect("parse answers");
        assert!(!r.data.is_empty());
        assert!(
            !r.data[0].target.content.is_empty(),
            "answer must have content"
        );
        assert!(!r.data[0].target.id.is_empty());
    }

    #[test]
    fn parses_comments() {
        let raw = include_str!("../../../tests/fixtures/comments.json");
        let r: CommentResponse = serde_json::from_str(raw).expect("parse comments");
        assert!(!r.data.is_empty());
        assert!(!r.data[0].content.is_empty(), "comment must have content");
        assert!(
            !r.data[0].author.name.is_empty(),
            "comment must have an author name"
        );
    }
}
