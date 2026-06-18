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
    if let Some(q) = &e.open_token {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockView {
    Watchlist,
    Market,
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
    pub active_platform: crate::platform::Platform,
    /// When a cookie-gated platform was requested without a stored cookie, the
    /// platform the pending Login screen should connect once a cookie is entered.
    pub pending_login_platform: Option<crate::platform::Platform>,
    /// Cursor on the Root platform picker (index into `Platform::ALL`).
    pub root_cursor: usize,
    pub stock_force_refresh: bool,
    pub stock_view: StockView,
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
            active_platform: crate::platform::Platform::Zhihu,
            pending_login_platform: None,
            root_cursor: 0,
            stock_force_refresh: false,
            stock_view: StockView::Watchlist,
        }
    }

    /// Platform highlighted on the Root picker.
    pub fn picked_platform(&self) -> crate::platform::Platform {
        crate::platform::Platform::ALL[self.root_cursor]
    }
    pub fn root_cursor_up(&mut self) { self.root_cursor = self.root_cursor.saturating_sub(1); }
    pub fn root_cursor_down(&mut self) {
        if self.root_cursor + 1 < crate::platform::Platform::ALL.len() { self.root_cursor += 1; }
    }

    /// Apply a list batch: drop rows already seen this session, record the rest,
    /// and show only the fresh ones. If the batch is entirely seen (server returned
    /// the same page), keep the current list rather than blanking the screen.
    pub fn apply_list_deduped(&mut self, items: Vec<ListEntry>) {
        let fresh: Vec<ListEntry> = items
            .into_iter()
            .filter(|e| self.seen.insert(entry_key(e)))
            .collect();
        if !fresh.is_empty() {
            self.list = fresh;
            self.list_cursor = 0;
        }
    }

    pub fn replace_list(&mut self, items: Vec<ListEntry>) {
        self.list = items;
        self.list_cursor = 0;
    }

    pub fn prepare_stream_list(&mut self, count: usize) {
        self.list = (0..count)
            .map(|_| ListEntry {
                title: "加载中...".into(),
                subtitle: String::new(),
                open_token: None,
                detail: None,
            })
            .collect();
        self.list_cursor = 0;
    }

    pub fn set_list_entry(&mut self, index: usize, entry: ListEntry) {
        if index >= self.list.len() {
            self.list.resize_with(index + 1, || ListEntry {
                title: "加载中...".into(),
                subtitle: String::new(),
                open_token: None,
                detail: None,
            });
        }
        self.list[index] = entry;
    }

    /// "Load more": append rows not yet seen this session onto the current list,
    /// leaving the cursor where it is. Used by forum boards (NGA, …) whose hot/
    /// active feeds re-sort each request — replacing the list would shrink it to
    /// the few newly-bumped threads, so we grow it instead.
    pub fn extend_list_deduped(&mut self, items: Vec<ListEntry>) {
        for e in items {
            if self.seen.insert(entry_key(&e)) {
                self.list.push(e);
            }
        }
    }

    pub fn screen(&self) -> &Screen { self.stack.last().unwrap() }
    pub fn push(&mut self, s: Screen) { self.stack.push(s); }
    pub fn back(&mut self) { if self.stack.len() > 1 { self.stack.pop(); } }
    pub fn replace(&mut self, s: Screen) { self.stack.pop(); self.stack.push(s); }

    /// Switch the active platform: reset dedup memory and current list so the new
    /// platform starts clean.
    pub fn switch_platform(&mut self, p: crate::platform::Platform) {
        if self.active_platform != p {
            self.active_platform = p;
            self.seen.clear();
            self.list.clear();
            self.list_cursor = 0;
        }
    }

    #[cfg(test)]
    pub fn set_list(&mut self, items: Vec<ListEntry>) { self.list = items; self.list_cursor = 0; }
    pub fn list_cursor(&self) -> usize { self.list_cursor }
    pub fn cursor_down(&mut self) {
        if !self.list.is_empty() && self.list_cursor + 1 < self.list.len() { self.list_cursor += 1; }
    }
    pub fn cursor_up(&mut self) { self.list_cursor = self.list_cursor.saturating_sub(1); }
    pub fn selected_entry(&self) -> Option<&ListEntry> { self.list.get(self.list_cursor) }
    pub fn current_detail(&self) -> Option<&DetailView> { self.details.get(self.detail_idx) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_platform_is_zhihu() {
        let app = App::new();
        assert_eq!(app.active_platform, crate::platform::Platform::Zhihu);
    }

    fn e(title: &str, tok: &str) -> ListEntry {
        ListEntry { title: title.into(), subtitle: String::new(), open_token: Some(tok.into()), detail: None }
    }

    #[test]
    fn extend_list_deduped_appends_only_fresh() {
        let mut app = App::new();
        app.apply_list_deduped(vec![e("a", "1"), e("b", "2")]);
        app.list_cursor = 1;
        // Load-more with one seen ("2") + two new ("3","4"): list grows, cursor kept.
        app.extend_list_deduped(vec![e("b2", "2"), e("c", "3"), e("d", "4")]);
        assert_eq!(app.list.len(), 4, "two fresh appended, seen dropped");
        assert_eq!(app.list[2].title, "c");
        assert_eq!(app.list[3].title, "d");
        assert_eq!(app.list_cursor, 1, "cursor must not move on append");
    }
}
