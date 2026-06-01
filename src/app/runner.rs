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
}

pub enum Update {
    Connected { cookie: String, list: Vec<ListEntry> },
    ConnectFailed(String),
    List(Vec<ListEntry>),
    Details(Vec<DetailView>),
    Comments(Vec<CommentView>),
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
    }
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
            Some(upd) = upd_rx.recv() => apply_update(&mut app, upd),
            _ = tick.tick() => {}
        }
    };

    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(term.backend_mut(), crossterm::terminal::LeaveAlternateScreen);
    result
}

fn apply_update(app: &mut App, upd: Update) {
    app.loading = false;
    match upd {
        Update::Connected { cookie, list } => {
            app.cookie = cookie.clone();
            // Persist the validated cookie so next launch skips login.
            let mut cfg = crate::config::Config::load().unwrap_or_default();
            cfg.zhihu.cookie = cookie;
            let _ = cfg.save();
            app.error = None;
            app.set_list(list);
            match app.screen() {
                Screen::List => {}
                Screen::Login => app.replace(Screen::List),
                _ => app.push(Screen::List),
            }
        }
        Update::List(list) => {
            app.error = None;
            app.set_list(list);
            match app.screen() {
                Screen::List => {}
                Screen::Login => app.replace(Screen::List),
                _ => app.push(Screen::List),
            }
        }
        Update::Details(d) => {
            app.error = None; app.details = d; app.detail_idx = 0; app.detail_scroll = 0;
            app.push(Screen::Detail);
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
    // Boss key: ` toggles a fake innocuous screen. Works on every screen and even
    // while typing; while hidden, all other keys are swallowed. Not advertised on-screen.
    if code == KeyCode::Char('`') {
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
            } else if *app.screen() == Screen::Detail {
                if c == 'n' && app.detail_idx + 1 < app.details.len() {
                    app.detail_idx += 1;
                    app.detail_scroll = 0;
                } else if c == 'p' {
                    app.detail_idx = app.detail_idx.saturating_sub(1);
                    app.detail_scroll = 0;
                } else if c == 'c' {
                    app.camouflage = !app.camouflage;
                } else if let Some(url) = image_for_digit(app.current_detail(), c) {
                    open_url(&url);
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

fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
}

/// Pure helper: given the current detail view and a digit character, return the
/// corresponding image URL (1-based). Returns None for non-digit chars, '0', or
/// out-of-range indices.
fn image_for_digit(detail: Option<&DetailView>, c: char) -> Option<String> {
    let d = c.to_digit(10)?;
    if d < 1 {
        return None;
    }
    detail?.images.get((d - 1) as usize).cloned()
}

fn open_selection(app: &mut App, req_tx: &mpsc::UnboundedSender<Request>) {
    if *app.screen() == Screen::List {
        let qid = app.selected_entry().and_then(|e| e.question_id.clone());
        if let Some(qid) = qid {
            app.loading = true;
            let _ = req_tx.send(Request::Answers(qid));
        }
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
        app.replace(Screen::Login);
        apply_update(&mut app, Update::List(vec![entry("t", None)]));
        assert_eq!(app.screen(), &Screen::List);
        assert_eq!(app.list.len(), 1);
    }

    // 11. apply_details_update_pushes_detail
    #[test]
    fn apply_details_update_pushes_detail() {
        let mut app = App::new();
        apply_update(&mut app, Update::Details(vec![DetailView {
            author: "a".into(),
            voteup: 1,
            body: "b".into(),
            images: vec![],
            answer_id: "9".into(),
        }]));
        assert_eq!(app.screen(), &Screen::Detail);
        assert!(app.current_detail().is_some());
    }

    // 13. image_for_digit returns correct url
    #[test]
    fn image_for_digit_returns_correct_url() {
        let dv = DetailView {
            author: "a".into(),
            voteup: 0,
            body: "b".into(),
            images: vec!["u1".into(), "u2".into()],
            answer_id: "1".into(),
        };
        assert_eq!(image_for_digit(Some(&dv), '1'), Some("u1".into()));
        assert_eq!(image_for_digit(Some(&dv), '2'), Some("u2".into()));
        assert_eq!(image_for_digit(Some(&dv), '3'), None);
        assert_eq!(image_for_digit(Some(&dv), '0'), None);
        assert_eq!(image_for_digit(Some(&dv), 'x'), None);
        assert_eq!(image_for_digit(None, '1'), None);
    }

    // 12. apply_error_update_sets_error_without_navigating
    #[test]
    fn apply_error_update_sets_error_without_navigating() {
        let mut app = App::new();
        // app starts at Root
        apply_update(&mut app, Update::Error("boom".into()));
        assert_eq!(app.error.as_deref(), Some("boom"));
        assert_eq!(app.screen(), &Screen::Root);
    }
}
