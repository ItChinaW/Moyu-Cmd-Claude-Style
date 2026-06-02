pub mod state;
pub mod command;
pub mod runner;
use state::Screen;
use crate::platform::{ListEntry, DetailView, CommentView};
use std::collections::HashSet;

/// Dedup key for a recommend row — answer id if known (each card is one answer),
/// else question id, else the title.
fn entry_key(e: &ListEntry) -> String {
    if let Some(d) = &e.detail {
        if !d.answer_id.is_empty() {
            return format!("a:{}", d.answer_id);
        }
    }
    if let Some(q) = &e.question_id {
        return format!("q:{q}");
    }
    format!("t:{}", e.title)
}

#[derive(Debug, Clone, PartialEq)]
pub enum ListSource {
    Recommend,
    Hot,
    Search(String),
}

pub struct App {
    stack: Vec<Screen>,
    pub list: Vec<ListEntry>,
    list_cursor: usize,
    pub details: Vec<DetailView>,
    pub detail_idx: usize,
    pub detail_scroll: u16,
    pub comments: Vec<CommentView>,
    pub comment_scroll: u16,
    pub command: String,
    pub loading: bool,
    pub error: Option<String>,
    pub should_quit: bool,
    pub cookie: String,
    pub list_source: ListSource,
    /// Boss key: when true the UI shows an innocuous fake screen and swallows input.
    pub boss_mode: bool,
    /// Camouflage: interleave dim Claude-Code-style decoy lines into the answer body.
    pub camouflage: bool,
    /// Local file paths of the current answer's downloaded images, index-aligned with
    /// `current_detail().images`. Empty string = that image failed to download.
    pub image_paths: Vec<String>,
    /// answer_id that `image_paths` belong to — guards against stale async results.
    pub image_owner: String,
    /// Recommend rows already shown this session, so refresh never repeats them.
    seen: HashSet<String>,
}

impl App {
    pub fn new() -> Self {
        Self {
            stack: vec![Screen::Root],
            list: Vec::new(), list_cursor: 0,
            details: Vec::new(), detail_idx: 0, detail_scroll: 0,
            comments: Vec::new(), comment_scroll: 0,
            command: String::new(), loading: false, error: None,
            should_quit: false, cookie: String::new(),
            list_source: ListSource::Recommend,
            boss_mode: false,
            camouflage: true,
            image_paths: Vec::new(),
            image_owner: String::new(),
            seen: HashSet::new(),
        }
    }

    /// Apply a recommend batch: drop rows already seen this session, record the rest,
    /// and show only the fresh ones. If the batch is entirely seen (server returned
    /// the same page), keep the current list rather than blanking the screen.
    pub fn apply_recommend(&mut self, items: Vec<ListEntry>) {
        let fresh: Vec<ListEntry> = items
            .into_iter()
            .filter(|e| self.seen.insert(entry_key(e)))
            .collect();
        if !fresh.is_empty() {
            self.list = fresh;
            self.list_cursor = 0;
        }
    }

    pub fn screen(&self) -> &Screen { self.stack.last().unwrap() }
    pub fn push(&mut self, s: Screen) { self.stack.push(s); }
    pub fn back(&mut self) { if self.stack.len() > 1 { self.stack.pop(); } }
    pub fn replace(&mut self, s: Screen) { self.stack.pop(); self.stack.push(s); }

    pub fn set_list(&mut self, items: Vec<ListEntry>) { self.list = items; self.list_cursor = 0; }
    pub fn list_cursor(&self) -> usize { self.list_cursor }
    pub fn cursor_down(&mut self) {
        if !self.list.is_empty() && self.list_cursor + 1 < self.list.len() { self.list_cursor += 1; }
    }
    pub fn cursor_up(&mut self) { self.list_cursor = self.list_cursor.saturating_sub(1); }
    pub fn selected_entry(&self) -> Option<&ListEntry> { self.list.get(self.list_cursor) }
    pub fn current_detail(&self) -> Option<&DetailView> { self.details.get(self.detail_idx) }
}
