use crate::app::{App, ListSource};
use crate::app::state::Screen;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(f.area());
    match app.screen() {
        Screen::Root => draw_root(f, chunks[0]),
        Screen::Login => draw_login(f, chunks[0], app),
        Screen::List => draw_list(f, chunks[0], app),
        Screen::Detail => draw_detail(f, chunks[0], app),
        Screen::Comments => draw_comments(f, chunks[0], app),
    }
    draw_command_bar(f, chunks[1], app);
}

fn draw_root(f: &mut Frame, area: Rect) {
    let p = Paragraph::new("输入 /zhihu 进入知乎\n(以后: /tieba 进入贴吧)")
        .block(Block::default().borders(Borders::ALL).title("touch-fish"));
    f.render_widget(p, area);
}

fn draw_login(f: &mut Frame, area: Rect, app: &App) {
    let msg = if let Some(e) = &app.error {
        format!("登录失败: {e}\n\n请重新粘贴知乎 Cookie 后回车")
    } else {
        "未检测到登录态。\n从浏览器 F12 → Network → 任意知乎请求复制 Cookie，\n粘贴到下方命令行后回车。".to_string()
    };
    let title = if app.loading { "验证中…" } else { "知乎登录" };
    let p = Paragraph::new(msg)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}

fn draw_list(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app.list.iter().enumerate().map(|(i, e)| {
        let marker = if i == app.list_cursor() { "> " } else { "  " };
        let style = if i == app.list_cursor() {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else { Style::default() };
        let mut lines = vec![Line::from(vec![
            Span::raw(marker),
            Span::styled(format!("{}. {}", i + 1, e.title), style),
        ])];
        if !e.subtitle.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("     {}", e.subtitle),
                Style::default().fg(Color::DarkGray),
            )));
        }
        ListItem::new(lines)
    }).collect();
    let feed_name = match &app.list_source {
        ListSource::Recommend => "推荐".to_string(),
        ListSource::Hot => "热榜".to_string(),
        ListSource::Search(q) => format!("搜索: {q}"),
    };
    let title = if app.loading {
        format!("{feed_name} (加载中…)")
    } else {
        feed_name
    };
    f.render_widget(List::new(items).block(Block::default().borders(Borders::ALL).title(title)), area);
}

fn draw_detail(f: &mut Frame, area: Rect, app: &App) {
    let (title, body) = match app.current_detail() {
        Some(d) => (format!("{} · 赞{}", d.author, d.voteup), d.body.clone()),
        None => ("详情".to_string(), "(无内容)".to_string()),
    };
    let p = Paragraph::new(body)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0))
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}

fn draw_comments(f: &mut Frame, area: Rect, app: &App) {
    let text: Vec<Line> = app.comments.iter().flat_map(|c| {
        let header = if c.child_count > 0 {
            format!("{} (赞{} · {}条回复)", c.author, c.like_count, c.child_count)
        } else {
            format!("{} (赞{})", c.author, c.like_count)
        };
        vec![
            Line::from(Span::styled(header, Style::default().fg(Color::Yellow))),
            Line::from(c.body.clone()),
            Line::from(""),
        ]
    }).collect();
    let p = Paragraph::new(text)
        .wrap(Wrap { trim: true })
        .scroll((app.comment_scroll, 0))
        .block(Block::default().borders(Borders::ALL).title("评论"));
    f.render_widget(p, area);
}

fn draw_command_bar(f: &mut Frame, area: Rect, app: &App) {
    let line = if let Some(e) = &app.error {
        Line::from(Span::styled(format!(" {e} "), Style::default().fg(Color::Red)))
    } else {
        Line::from(format!("> {}", app.command))
    };
    let hint = match app.screen() {
        Screen::Root => "输入 /zhihu 进入知乎   /quit 退出",
        Screen::Login => "粘贴 Cookie 后回车   Esc 返回",
        Screen::List => "↑↓选择 Enter进入 r刷新 /hot热榜 /search搜索 ←返回 q退出",
        Screen::Detail => "↑↓滚动 n/p切换回答 →/Tab看评论 ←返回",
        Screen::Comments => "↑↓滚动 ←返回",
    };
    f.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL).title(hint)),
        area,
    );
}
