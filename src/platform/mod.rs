pub mod html;
pub mod zhihu;
pub mod v2ex;
pub mod hupu;
pub mod nga;
pub mod linuxdo;
pub mod tieba;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform { Zhihu, V2ex, Hupu, Nga, LinuxDo, Tieba }

impl Platform {
    /// All platforms, in landing-page picker order.
    pub const ALL: [Platform; 6] = [
        Platform::Zhihu,
        Platform::V2ex,
        Platform::Hupu,
        Platform::Nga,
        Platform::LinuxDo,
        Platform::Tieba,
    ];

    /// Human label shown in the (non-camouflaged) status line.
    pub fn label(self) -> &'static str {
        match self {
            Platform::Zhihu => "知乎",
            Platform::V2ex => "V2EX",
            Platform::Hupu => "虎扑",
            Platform::Nga => "NGA",
            Platform::LinuxDo => "Linux.do",
            Platform::Tieba => "贴吧",
        }
    }
    /// Whether this platform needs a user-supplied cookie.
    pub fn needs_cookie(self) -> bool {
        matches!(self, Platform::Zhihu | Platform::Nga | Platform::LinuxDo | Platform::Tieba)
    }
}

/// A row in a list screen (hot list or search results).
#[derive(Debug, Clone)]
pub struct ListEntry {
    pub title: String,
    pub subtitle: String,
    /// Token used to open this row: Zhihu question id, or a forum topic URL/tid.
    pub open_token: Option<String>,
    /// The exact answer this row previewed, when the feed already carried it
    /// (recommend cards do). Opening shows this directly so the body matches the
    /// subtitle; `None` falls back to fetching the question's answer feed.
    pub detail: Option<DetailView>,
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
