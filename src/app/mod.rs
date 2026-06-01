pub mod state;
pub mod command;
pub mod runner;
use state::Screen;
use crate::platform::{ListEntry, DetailView, CommentView};

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
