use anyhow::Result;
use crate::app::App;
use crate::app::command::{self, Command};
use crate::app::state::Screen;
use crate::platform::{ListEntry, DetailView, CommentView};
use crate::platform::zhihu::client::ZhihuClient;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::time::Duration;
use tokio::sync::mpsc;

pub enum Request {
    Connect(String),     // cookie
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
            Ok(c) => match c.hot_list().await {
                Ok(list) => { *client = Some(c); Update::Connected { cookie, list } }
                Err(e) => Update::ConnectFailed(e.to_string()),
            },
            Err(e) => Update::ConnectFailed(e.to_string()),
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
    match code {
        KeyCode::Char(c) => {
            // On Login everything typed is the cookie; elsewhere typing begins with '/'.
            if *app.screen() == Screen::Login || c == '/' || !app.command.is_empty() {
                app.command.push(c);
            } else if c == 'q' {
                app.should_quit = true;
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
        KeyCode::Right | KeyCode::Tab => {
            if *app.screen() == Screen::Detail {
                // clone the id first so the immutable borrow ends before we mutate `app`
                let aid = app.current_detail().map(|d| d.answer_id.clone());
                if let Some(aid) = aid {
                    app.loading = true;
                    let _ = req_tx.send(Request::Comments(aid));
                }
            }
        }
        _ => {}
    }
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

fn dispatch_command(app: &mut App, cmd: Command, req_tx: &mpsc::UnboundedSender<Request>) {
    match cmd {
        Command::Zhihu => {
            if app.cookie.is_empty() {
                app.replace(Screen::Login);
            } else {
                app.loading = true;
                let _ = req_tx.send(Request::HotList);
            }
        }
        Command::Search(q) => { app.loading = true; let _ = req_tx.send(Request::Search(q)); }
        Command::Login => { app.error = None; app.replace(Screen::Login); }
        Command::Back => app.back(),
        Command::Quit => app.should_quit = true,
        Command::Unknown(s) => app.error = Some(format!("未知命令: {s}")),
    }
}
