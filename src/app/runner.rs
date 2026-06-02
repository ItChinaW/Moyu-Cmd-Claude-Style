use anyhow::Result;
use crate::app::{App, ListSource};
use crate::app::command::{self, Command};
use crate::app::state::Screen;
use crate::platform::{ListEntry, DetailView, CommentView};
use crate::platform::zhihu::client::ZhihuClient;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum Request {
    Connect(String),     // cookie
    Recommend,
    HotList,
    Search(String),
    Answers(String),     // question id
    Comments(String),    // answer id
    FetchImages { answer_id: String, urls: Vec<String> },
}

pub enum Update {
    Connected { cookie: String, list: Vec<ListEntry> },
    ConnectFailed(String),
    List(Vec<ListEntry>),
    Details(Vec<DetailView>),
    Comments(Vec<CommentView>),
    ImagesReady { answer_id: String, paths: Vec<String> },
    Error(String),
}

/// Worker thread: owns the `!Send` ZhihuClient, serves requests on its own runtime.
fn spawn_worker(mut rx: mpsc::UnboundedReceiver<Request>, tx: mpsc::UnboundedSender<Update>) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all().build().expect("worker runtime");
        rt.block_on(async move {
            let mut client: Option<ZhihuClient> = None;
            while let Some(req) = rx.recv().await {
                let upd = handle(&mut client, req).await;
                if tx.send(upd).is_err() { break; }
            }
        });
    });
}

async fn handle(client: &mut Option<ZhihuClient>, req: Request) -> Update {
    match req {
        Request::Connect(cookie) => match ZhihuClient::new(cookie.clone()) {
            Ok(c) => match c.recommend().await {
                Ok(list) => { *client = Some(c); Update::Connected { cookie, list } }
                Err(e) => Update::ConnectFailed(e.to_string()),
            },
            Err(e) => Update::ConnectFailed(e.to_string()),
        },
        Request::Recommend => match client {
            Some(c) => match c.recommend().await {
                Ok(v) => Update::List(v), Err(e) => Update::Error(e.to_string()) },
            None => Update::Error("未登录".into()),
        },
        Request::HotList => match client {
            Some(c) => match c.hot_list().await {
                Ok(v) => Update::List(v), Err(e) => Update::Error(e.to_string()) },
            None => Update::Error("未登录".into()),
        },
        Request::Search(q) => match client {
            Some(c) => match c.search(&q).await {
                Ok(v) => Update::List(v), Err(e) => Update::Error(e.to_string()) },
            None => Update::Error("未登录".into()),
        },
        Request::Answers(id) => match client {
            Some(c) => match c.answers(&id).await {
                Ok(v) => Update::Details(v), Err(e) => Update::Error(e.to_string()) },
            None => Update::Error("未登录".into()),
        },
        Request::Comments(id) => match client {
            Some(c) => match c.comments(&id).await {
                Ok(v) => Update::Comments(v), Err(e) => Update::Error(e.to_string()) },
            None => Update::Error("未登录".into()),
        },
        Request::FetchImages { answer_id, urls } => {
            let paths = match client {
                Some(c) => download_images(c, &urls).await,
                None => Vec::new(),
            };
            Update::ImagesReady { answer_id, paths }
        }
    }
}

/// Local cache dir for the *currently viewed* answer's images. Wiped on each new
/// fetch so the disk only ever holds one article's images.
fn image_cache_dir() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("touch-fish")
        .join("img")
}

/// File extension from a URL, defaulting to `jpg` for anything unusual.
fn url_ext(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = path.rsplit('.').next().unwrap_or("jpg");
    if !ext.is_empty() && ext.len() <= 4 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        ext.to_ascii_lowercase()
    } else {
        "jpg".into()
    }
}

/// Clear the cache dir, then download every URL into it. Returns local paths
/// index-aligned with `urls`; a failed download yields an empty string in its slot.
async fn download_images(client: &ZhihuClient, urls: &[String]) -> Vec<String> {
    let dir = image_cache_dir();
    let _ = std::fs::remove_dir_all(&dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return Vec::new();
    }
    let mut paths = Vec::with_capacity(urls.len());
    for (i, url) in urls.iter().enumerate() {
        let file = dir.join(format!("{:02}.{}", i + 1, url_ext(url)));
        let ok = match client.download_image(url).await {
            Ok(bytes) => std::fs::write(&file, &bytes).is_ok(),
            Err(_) => false,
        };
        paths.push(if ok { file.to_string_lossy().into_owned() } else { String::new() });
    }
    paths
}

pub async fn run_app(cookie: String) -> Result<()> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stderr(), crossterm::terminal::LeaveAlternateScreen);
        original_hook(info);
    }));
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let mut term = Terminal::new(CrosstermBackend::new(stdout))?;

    let (req_tx, req_rx) = mpsc::unbounded_channel::<Request>();
    let (upd_tx, mut upd_rx) = mpsc::unbounded_channel::<Update>();
    spawn_worker(req_rx, upd_tx);

    let mut app = App::new();
    app.cookie = cookie.clone();
    if cookie.is_empty() {
        app.replace(Screen::Login);
    } else {
        app.loading = true;
        let _ = req_tx.send(Request::Connect(cookie));
    }

    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(120));

    let result = loop {
        if let Err(e) = term.draw(|f| crate::ui::draw(f, &app)) { break Err(e.into()); }
        if app.should_quit { break Ok(()); }
        tokio::select! {
            maybe_ev = events.next() => {
                match maybe_ev {
                    Some(Ok(Event::Key(k))) if k.kind == KeyEventKind::Press => {
                        handle_key(&mut app, k.code, &req_tx);
                    }
                    Some(Err(e)) => break Err(e.into()),
                    _ => {}
                }
            }
            Some(upd) = upd_rx.recv() => apply_update(&mut app, upd, &req_tx),
            _ = tick.tick() => {}
        }
    };

    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(term.backend_mut(), crossterm::terminal::LeaveAlternateScreen);
    result
}

/// Drop any stale image paths and request a fresh download for the answer now on
/// screen. New paths arrive asynchronously via `Update::ImagesReady`.
fn request_images(app: &mut App, req_tx: &mpsc::UnboundedSender<Request>) {
    app.image_paths.clear();
    app.image_owner.clear();
    if let Some(d) = app.current_detail() {
        if !d.images.is_empty() {
            let _ = req_tx.send(Request::FetchImages {
                answer_id: d.answer_id.clone(),
                urls: d.images.clone(),
            });
        }
    }
}

fn apply_update(app: &mut App, upd: Update, req_tx: &mpsc::UnboundedSender<Request>) {
    app.loading = false;
    match upd {
        Update::Connected { cookie, list } => {
            app.cookie = cookie.clone();
            // Persist the validated cookie so next launch skips login.
            let mut cfg = crate::config::Config::load().unwrap_or_default();
            cfg.zhihu.cookie = cookie;
            let _ = cfg.save();
            app.error = None;
            // Initial feed is the recommend stream — dedup against the session.
            app.apply_recommend(list);
            match app.screen() {
                Screen::List => {}
                Screen::Login => app.replace(Screen::List),
                _ => app.push(Screen::List),
            }
        }
        Update::List(list) => {
            app.error = None;
            if app.list_source == ListSource::Recommend {
                app.apply_recommend(list);
            } else {
                app.set_list(list);
            }
            match app.screen() {
                Screen::List => {}
                Screen::Login => app.replace(Screen::List),
                _ => app.push(Screen::List),
            }
        }
        Update::Details(d) => {
            app.error = None; app.details = d; app.detail_idx = 0; app.detail_scroll = 0;
            app.push(Screen::Detail);
            request_images(app, req_tx);
        }
        Update::ImagesReady { answer_id, paths } => {
            // Adopt only if these still belong to the answer currently on screen.
            if app.current_detail().map(|d| d.answer_id.as_str()) == Some(answer_id.as_str()) {
                app.image_paths = paths;
                app.image_owner = answer_id;
            }
        }
        Update::Comments(c) => {
            app.error = None; app.comments = c; app.comment_scroll = 0;
            app.push(Screen::Comments);
        }
        Update::ConnectFailed(e) => { app.error = Some(e); app.replace(Screen::Login); }
        Update::Error(e) => { app.error = Some(e); }
    }
}

fn handle_key(app: &mut App, code: KeyCode, req_tx: &mpsc::UnboundedSender<Request>) {
    // Boss key: the backtick key toggles a fake innocuous screen. Works on every
    // screen and even while typing; while hidden, all other keys are swallowed.
    // Not advertised on-screen. Accepts the Chinese-IME variants too (see is_boss_key).
    if is_boss_key(code) {
        app.boss_mode = !app.boss_mode;
        return;
    }
    if app.boss_mode {
        return;
    }
    match code {
        KeyCode::Char(c) => {
            app.error = None;
            let on_login = *app.screen() == Screen::Login;
            if on_login || c == '/' || !app.command.is_empty() {
                app.command.push(c);
            } else if c == 'q' {
                app.should_quit = true;
            } else if c == 'r' && *app.screen() == Screen::List {
                if !app.cookie.is_empty() {
                    refresh(app, req_tx);
                }
            } else if c == 'c'
                && matches!(app.screen(), Screen::Detail | Screen::List)
            {
                app.camouflage = !app.camouflage;
            } else if *app.screen() == Screen::Detail {
                if c == 'n' && app.detail_idx + 1 < app.details.len() {
                    app.detail_idx += 1;
                    app.detail_scroll = 0;
                    request_images(app, req_tx);
                } else if c == 'p' && app.detail_idx > 0 {
                    app.detail_idx -= 1;
                    app.detail_scroll = 0;
                    request_images(app, req_tx);
                } else if let Some(path) = image_path_for_digit(app, c) {
                    // Open in the surrounding editor (tab), like clicking the path.
                    open_image(&path);
                }
            }
        }
        KeyCode::Backspace => { app.command.pop(); }
        KeyCode::Esc => { app.command.clear(); app.back(); }
        KeyCode::Enter => {
            if *app.screen() == Screen::Login {
                let cookie = std::mem::take(&mut app.command);
                if !cookie.is_empty() {
                    app.cookie = cookie.clone();
                    app.loading = true;
                    let _ = req_tx.send(Request::Connect(cookie));
                }
            } else if !app.command.is_empty() {
                let cmd = command::parse(&std::mem::take(&mut app.command));
                dispatch_command(app, cmd, req_tx);
            } else {
                open_selection(app, req_tx);
            }
        }
        KeyCode::Up => match app.screen() {
            Screen::List => app.cursor_up(),
            Screen::Detail => app.detail_scroll = app.detail_scroll.saturating_sub(1),
            Screen::Comments => app.comment_scroll = app.comment_scroll.saturating_sub(1),
            _ => {}
        },
        KeyCode::Down => match app.screen() {
            Screen::List => app.cursor_down(),
            Screen::Detail => app.detail_scroll = app.detail_scroll.saturating_add(1),
            Screen::Comments => app.comment_scroll = app.comment_scroll.saturating_add(1),
            _ => {}
        },
        KeyCode::Left => app.back(),
        KeyCode::Right | KeyCode::Tab if *app.screen() == Screen::Detail => {
            // clone the id first so the immutable borrow ends before we mutate `app`
            if let Some(aid) = app.current_detail().map(|d| d.answer_id.clone()) {
                app.loading = true;
                let _ = req_tx.send(Request::Comments(aid));
            }
        }
        _ => {}
    }
}

/// The boss key is the physical backtick/tilde key (left of `1`). Input methods emit
/// different characters for it: English `` ` ``, while Chinese IMEs produce a middle dot
/// (`·` on macOS Pinyin, `・`/`•` on others). Accept all so it works in any input mode.
fn is_boss_key(code: KeyCode) -> bool {
    matches!(code, KeyCode::Char('`' | '·' | '・' | '•'))
}

fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
}

/// Open a local image in the surrounding editor (VS Code / Cursor / Windsurf / …)
/// so it lands in an editor tab — same as clicking the path — instead of the OS
/// default viewer (Preview). Falls back to the OS default if no editor CLI is found.
fn open_image(path: &str) {
    // Hint which editor we're inside from VS Code-family env vars / the macOS bundle id.
    let hint = std::env::var("VSCODE_GIT_ASKPASS_MAIN")
        .or_else(|_| std::env::var("VSCODE_GIT_ASKPASS_NODE"))
        .or_else(|_| std::env::var("__CFBundleIdentifier"))
        .unwrap_or_default()
        .to_lowercase();
    let preferred = if hint.contains("cursor") {
        "cursor"
    } else if hint.contains("windsurf") {
        "windsurf"
    } else if hint.contains("insiders") {
        "code-insiders"
    } else {
        "code"
    };
    let mut tried: Vec<&str> = Vec::new();
    for cli in std::iter::once(preferred).chain(["code", "cursor", "windsurf", "code-insiders"]) {
        if tried.contains(&cli) {
            continue;
        }
        tried.push(cli);
        // `-r` reuses the current window so it opens as a tab, not a new window.
        if std::process::Command::new(cli).arg("-r").arg(path).spawn().is_ok() {
            return;
        }
    }
    open_url(path); // no editor CLI on PATH → OS default app
}

/// Pure helper: given the current detail view and a digit character, return the
/// corresponding image URL (1-based). Returns None for non-digit chars, '0', or
/// out-of-range indices.
/// Local image path for digit key `c` (1-based), or None if absent / failed download.
fn image_path_for_digit(app: &App, c: char) -> Option<String> {
    let d = c.to_digit(10)?;
    if d < 1 {
        return None;
    }
    app.image_paths
        .get((d - 1) as usize)
        .filter(|p| !p.is_empty())
        .cloned()
}

fn open_selection(app: &mut App, req_tx: &mpsc::UnboundedSender<Request>) {
    if *app.screen() != Screen::List {
        return;
    }
    let entry = match app.selected_entry() {
        Some(e) => (e.detail.clone(), e.question_id.clone()),
        None => return,
    };
    match entry {
        // Recommend cards carry the exact answer they previewed — show it directly
        // so the body matches the subtitle (no extra request).
        (Some(detail), _) => {
            app.error = None;
            app.details = vec![detail];
            app.detail_idx = 0;
            app.detail_scroll = 0;
            app.push(Screen::Detail);
            request_images(app, req_tx);
        }
        (None, Some(qid)) => {
            app.loading = true;
            let _ = req_tx.send(Request::Answers(qid));
        }
        (None, None) => {}
    }
}

fn refresh(app: &mut App, req_tx: &mpsc::UnboundedSender<Request>) {
    app.loading = true;
    match &app.list_source.clone() {
        ListSource::Recommend => { let _ = req_tx.send(Request::Recommend); }
        ListSource::Hot => { let _ = req_tx.send(Request::HotList); }
        ListSource::Search(q) => { let _ = req_tx.send(Request::Search(q.clone())); }
    }
}

fn dispatch_command(app: &mut App, cmd: Command, req_tx: &mpsc::UnboundedSender<Request>) {
    match cmd {
        Command::Zhihu => {
            if app.cookie.is_empty() {
                app.replace(Screen::Login);
            } else {
                app.list_source = ListSource::Recommend;
                app.loading = true;
                let _ = req_tx.send(Request::Recommend);
            }
        }
        Command::Hot => {
            if app.cookie.is_empty() {
                app.replace(Screen::Login);
            } else {
                app.list_source = ListSource::Hot;
                app.loading = true;
                let _ = req_tx.send(Request::HotList);
            }
        }
        Command::Refresh => {
            if !app.cookie.is_empty() {
                refresh(app, req_tx);
            }
        }
        Command::Search(q) => {
            app.list_source = ListSource::Search(q.clone());
            app.loading = true;
            let _ = req_tx.send(Request::Search(q));
        }
        Command::Login => { app.error = None; app.replace(Screen::Login); }
        Command::Help => { if *app.screen() != Screen::Help { app.push(Screen::Help); } }
        Command::Back => app.back(),
        Command::Quit => app.should_quit = true,
        Command::Unknown(s) => app.error = Some(format!("未知命令: {s}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::Screen;
    use crate::app::{ListSource};
    use crate::app::command::Command;
    use crate::platform::{ListEntry, DetailView};
    use crossterm::event::KeyCode;
    use tokio::sync::mpsc;

    fn entry(title: &str, qid: Option<&str>) -> ListEntry {
        ListEntry {
            title: title.into(),
            subtitle: String::new(),
            question_id: qid.map(|s| s.to_string()),
            detail: None,
        }
    }

    fn make_channel() -> (mpsc::UnboundedSender<Request>, mpsc::UnboundedReceiver<Request>) {
        mpsc::unbounded_channel::<Request>()
    }

    #[test]
    fn boss_key_toggles_boss_mode() {
        let mut app = App::new();
        let (tx, _rx) = make_channel();
        assert!(!app.boss_mode);
        handle_key(&mut app, KeyCode::Char('`'), &tx);
        assert!(app.boss_mode, "backtick should enable boss mode");
        handle_key(&mut app, KeyCode::Char('`'), &tx);
        assert!(!app.boss_mode, "backtick again should disable it");
        // Chinese IME emits a middle dot for the same physical key — must also work.
        handle_key(&mut app, KeyCode::Char('·'), &tx);
        assert!(app.boss_mode, "Chinese-IME middle dot should enable boss mode");
        handle_key(&mut app, KeyCode::Char('·'), &tx);
        assert!(!app.boss_mode, "middle dot again should disable it");
    }

    #[test]
    fn c_key_toggles_camouflage_on_detail() {
        let mut app = App::new();
        app.details = vec![DetailView {
            author: "a".into(), voteup: 1, body: "b".into(),
            images: vec![], answer_id: "9".into(),
        }];
        app.push(Screen::Detail);
        let (tx, _rx) = make_channel();
        assert!(app.camouflage, "camouflage on by default");
        handle_key(&mut app, KeyCode::Char('c'), &tx);
        assert!(!app.camouflage, "c should toggle camouflage off");
        handle_key(&mut app, KeyCode::Char('c'), &tx);
        assert!(app.camouflage, "c again should toggle it back on");
    }

    #[test]
    fn boss_mode_swallows_other_keys() {
        let mut app = App::new();
        app.boss_mode = true;
        let (tx, _rx) = make_channel();
        // 'q' would normally quit; in boss mode it must be ignored.
        handle_key(&mut app, KeyCode::Char('q'), &tx);
        assert!(!app.should_quit, "q must be swallowed while hidden");
        assert!(app.command.is_empty(), "typing must be swallowed while hidden");
    }

    // 1. dispatch_zhihu_with_cookie_requests_recommend
    #[test]
    fn dispatch_zhihu_with_cookie_requests_recommend() {
        let mut app = App::new();
        app.cookie = "x".into();
        let (tx, mut rx) = make_channel();
        dispatch_command(&mut app, Command::Zhihu, &tx);
        match rx.try_recv() {
            Ok(Request::Recommend) => {}
            other => panic!("expected Recommend, got {:?}", other),
        }
        assert_eq!(app.list_source, ListSource::Recommend);
    }

    // 1b. dispatch_hot_with_cookie_requests_hotlist
    #[test]
    fn dispatch_hot_with_cookie_requests_hotlist() {
        let mut app = App::new();
        app.cookie = "x".into();
        let (tx, mut rx) = make_channel();
        dispatch_command(&mut app, Command::Hot, &tx);
        match rx.try_recv() {
            Ok(Request::HotList) => {}
            other => panic!("expected HotList, got {:?}", other),
        }
        assert_eq!(app.list_source, ListSource::Hot);
    }

    // 2. dispatch_zhihu_without_cookie_goes_to_login
    #[test]
    fn dispatch_zhihu_without_cookie_goes_to_login() {
        let mut app = App::new();
        // cookie is empty by default
        let (tx, mut rx) = make_channel();
        dispatch_command(&mut app, Command::Zhihu, &tx);
        assert_eq!(app.screen(), &Screen::Login);
        assert!(rx.try_recv().is_err(), "nothing should have been sent");
    }

    // 3. dispatch_search_sends_search_request
    #[test]
    fn dispatch_search_sends_search_request() {
        let mut app = App::new();
        let (tx, mut rx) = make_channel();
        dispatch_command(&mut app, Command::Search("rust".into()), &tx);
        match rx.try_recv() {
            Ok(Request::Search(q)) => assert_eq!(q, "rust"),
            other => panic!("expected Search(rust), got {:?}", other),
        }
    }

    // 4. dispatch_quit_sets_should_quit
    #[test]
    fn dispatch_quit_sets_should_quit() {
        let mut app = App::new();
        let (tx, _rx) = make_channel();
        dispatch_command(&mut app, Command::Quit, &tx);
        assert!(app.should_quit);
    }

    // 5. dispatch_unknown_sets_error
    #[test]
    fn dispatch_unknown_sets_error() {
        let mut app = App::new();
        let (tx, _rx) = make_channel();
        dispatch_command(&mut app, Command::Unknown("/foo".into()), &tx);
        assert!(app.error.is_some(), "expected an error message to be set");
    }

    // 6. typing_a_command_then_enter_dispatches
    #[test]
    fn typing_a_command_then_enter_dispatches() {
        let mut app = App::new();
        app.cookie = "x".into();
        let (tx, mut rx) = make_channel();
        for c in "/zhihu".chars() {
            handle_key(&mut app, KeyCode::Char(c), &tx);
        }
        handle_key(&mut app, KeyCode::Enter, &tx);
        match rx.try_recv() {
            Ok(Request::Recommend) => {}
            other => panic!("expected Recommend, got {:?}", other),
        }
        assert!(app.command.is_empty(), "command buffer should be cleared after dispatch");
    }

    // 6b. typing_hot_command_dispatches_hotlist
    #[test]
    fn typing_hot_command_dispatches_hotlist() {
        let mut app = App::new();
        app.cookie = "x".into();
        let (tx, mut rx) = make_channel();
        for c in "/hot".chars() {
            handle_key(&mut app, KeyCode::Char(c), &tx);
        }
        handle_key(&mut app, KeyCode::Enter, &tx);
        match rx.try_recv() {
            Ok(Request::HotList) => {}
            other => panic!("expected HotList, got {:?}", other),
        }
        assert!(app.command.is_empty());
    }

    // 6c. r_key_on_list_refreshes
    #[test]
    fn r_key_on_list_refreshes() {
        let mut app = App::new();
        app.cookie = "x".into();
        app.list_source = ListSource::Recommend;
        app.push(Screen::List);
        let (tx, mut rx) = make_channel();
        handle_key(&mut app, KeyCode::Char('r'), &tx);
        match rx.try_recv() {
            Ok(Request::Recommend) => {}
            other => panic!("expected Recommend refresh, got {:?}", other),
        }
        assert!(app.loading);
    }

    // 7. q_on_root_quits_when_not_composing
    #[test]
    fn q_on_root_quits_when_not_composing() {
        let mut app = App::new();
        let (tx, _rx) = make_channel();
        handle_key(&mut app, KeyCode::Char('q'), &tx);
        assert!(app.should_quit);
    }

    // 8. slash_starts_command_buffer
    #[test]
    fn slash_starts_command_buffer() {
        let mut app = App::new();
        let (tx, _rx) = make_channel();
        handle_key(&mut app, KeyCode::Char('/'), &tx);
        assert_eq!(app.command, "/");
        assert!(!app.should_quit);
    }

    // 9. open_selection_on_list_sends_answers
    #[test]
    fn open_selection_on_list_sends_answers() {
        let mut app = App::new();
        app.set_list(vec![entry("t", Some("123"))]);
        app.push(Screen::List);
        let (tx, mut rx) = make_channel();
        open_selection(&mut app, &tx);
        match rx.try_recv() {
            Ok(Request::Answers(id)) => assert_eq!(id, "123"),
            other => panic!("expected Answers(123), got {:?}", other),
        }
        assert!(app.loading);
    }

    // 10. apply_list_update_from_login_replaces_to_list
    #[test]
    fn apply_list_update_from_login_replaces_to_list() {
        let mut app = App::new();
        let (tx, _rx) = make_channel();
        app.replace(Screen::Login);
        apply_update(&mut app, Update::List(vec![entry("t", None)]), &tx);
        assert_eq!(app.screen(), &Screen::List);
        assert_eq!(app.list.len(), 1);
    }

    // 11. apply_details_update_pushes_detail
    #[test]
    fn apply_details_update_pushes_detail() {
        let mut app = App::new();
        let (tx, _rx) = make_channel();
        apply_update(&mut app, Update::Details(vec![DetailView {
            author: "a".into(),
            voteup: 1,
            body: "b".into(),
            images: vec![],
            answer_id: "9".into(),
        }]), &tx);
        assert_eq!(app.screen(), &Screen::Detail);
        assert!(app.current_detail().is_some());
    }

    // 13. image_path_for_digit returns the matching local path, skipping empty/failed slots
    #[test]
    fn image_path_for_digit_returns_correct_path() {
        let mut app = App::new();
        app.image_paths = vec!["/c/1.jpg".into(), String::new(), "/c/3.jpg".into()];
        assert_eq!(image_path_for_digit(&app, '1'), Some("/c/1.jpg".into()));
        assert_eq!(image_path_for_digit(&app, '2'), None); // empty = failed download
        assert_eq!(image_path_for_digit(&app, '3'), Some("/c/3.jpg".into()));
        assert_eq!(image_path_for_digit(&app, '4'), None);
        assert_eq!(image_path_for_digit(&app, '0'), None);
        assert_eq!(image_path_for_digit(&app, 'x'), None);
    }

    // 14. FetchImages clears stale paths and is requested for an answer with images
    #[test]
    fn request_images_sends_fetch_and_clears_stale() {
        let mut app = App::new();
        let (tx, mut rx) = make_channel();
        app.details = vec![DetailView {
            author: "a".into(),
            voteup: 0,
            body: "b".into(),
            images: vec!["https://pic/a.jpg".into()],
            answer_id: "42".into(),
        }];
        app.image_paths = vec!["/old.jpg".into()];
        app.image_owner = "old".into();
        request_images(&mut app, &tx);
        assert!(app.image_paths.is_empty(), "stale paths cleared");
        assert!(app.image_owner.is_empty());
        match rx.try_recv() {
            Ok(Request::FetchImages { answer_id, urls }) => {
                assert_eq!(answer_id, "42");
                assert_eq!(urls, vec!["https://pic/a.jpg".to_string()]);
            }
            other => panic!("expected FetchImages, got {:?}", other),
        }
    }

    // 15. recommend card with an inline answer opens it directly (body matches preview)
    #[test]
    fn open_selection_uses_prefetched_detail() {
        let mut app = App::new();
        app.set_list(vec![ListEntry {
            title: "问题标题".into(),
            subtitle: "预览摘要".into(),
            question_id: Some("100".into()),
            detail: Some(DetailView {
                author: "作者".into(),
                voteup: 5,
                body: "预览的正文".into(),
                images: vec![],
                answer_id: "777".into(),
            }),
        }]);
        app.push(Screen::List);
        let (tx, mut rx) = make_channel();
        open_selection(&mut app, &tx);
        assert_eq!(app.screen(), &Screen::Detail);
        assert_eq!(app.current_detail().map(|d| d.body.as_str()), Some("预览的正文"));
        assert_eq!(app.current_detail().map(|d| d.answer_id.as_str()), Some("777"));
        assert!(rx.try_recv().is_err(), "prefetched answer must not fetch the question feed");
    }

    // 16. recommend dedup: refresh drops already-seen rows, keeps list if nothing new
    #[test]
    fn recommend_dedup_skips_seen_rows() {
        let mut app = App::new();
        app.apply_recommend(vec![entry("a", Some("1")), entry("b", Some("2"))]);
        assert_eq!(app.list.len(), 2);
        // Refresh: q:2 already seen → dropped; q:3 is new → shown (replaces list).
        app.apply_recommend(vec![entry("b-again", Some("2")), entry("c", Some("3"))]);
        assert_eq!(app.list.len(), 1);
        assert_eq!(app.list[0].title, "c");
        // Refresh returning only seen rows → keep current list, don't blank it.
        app.apply_recommend(vec![entry("a", Some("1"))]);
        assert_eq!(app.list.len(), 1);
        assert_eq!(app.list[0].title, "c");
    }

    // 12. apply_error_update_sets_error_without_navigating
    #[test]
    fn apply_error_update_sets_error_without_navigating() {
        let mut app = App::new();
        let (tx, _rx) = make_channel();
        // app starts at Root
        apply_update(&mut app, Update::Error("boom".into()), &tx);
        assert_eq!(app.error.as_deref(), Some("boom"));
        assert_eq!(app.screen(), &Screen::Root);
    }
}
