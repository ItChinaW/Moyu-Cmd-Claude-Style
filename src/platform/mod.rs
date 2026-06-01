pub mod zhihu;

/// A row in a list screen (hot list or search results).
#[derive(Debug, Clone)]
pub struct ListEntry {
    pub title: String,
    pub subtitle: String,
    pub question_id: Option<String>,
}

/// A single answer rendered for the detail screen.
#[derive(Debug, Clone)]
pub struct DetailView {
    pub author: String,
    pub voteup: i64,
    pub body: String,      // already HTML-converted to text
    pub images: Vec<String>, // ordered image URLs extracted from the answer
    pub answer_id: String,
}

/// A comment for the comments screen.
#[derive(Debug, Clone)]
pub struct CommentView {
    pub author: String,
    pub body: String,
    pub like_count: i64,
    pub child_count: i64,
}
