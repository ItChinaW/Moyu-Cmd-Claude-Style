use crate::app::{App, ListSource};
use crate::app::state::Screen;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph, Wrap},
};

pub fn draw(f: &mut Frame, app: &App) {
    if app.boss_mode {
        draw_boss(f, f.area());
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
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
    f.render_widget(Paragraph::new("输入 /zhihu 进入知乎"), area);
}

fn draw_login(f: &mut Frame, area: Rect, app: &App) {
    let msg = if let Some(e) = &app.error {
        format!("登录失败: {e}\n请重新粘贴知乎 Cookie 后回车")
    } else if app.loading {
        "验证中…".to_string()
    } else {
        "未检测到登录态。浏览器 F12 → Network → 任意知乎请求复制 Cookie，粘贴后回车。".to_string()
    };
    f.render_widget(Paragraph::new(msg).wrap(Wrap { trim: true }), area);
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
    f.render_widget(List::new(items), area);
}

/// Dim Claude-Code-style decoy lines interleaved between answer paragraphs.
/// Uses the user's real project filenames so a passing glance reads as coding.
const DECOYS: &[&str] = &[
    "⏺ Read app/admin/personal/profile/page.tsx (142 lines)",
    "    export default function ProfilePage() {",
    "⏺ Update components/ModelPriceFormBlocks.tsx",
    "    +   const [loading, setLoading] = useState(false)",
    "● Bash(pnpm typecheck)  ⎿  0 errors, 0 warnings",
    "⏺ Edit components/ModelIconPickerModal.tsx",
    "    -   onClose={() => setOpen(false)}",
    "    return <ProfileForm onSubmit={handleSubmit} />",
    "⏺ Wrote app/admin/users/route.ts (28 lines)",
    "    await db.user.update({ where: { id }, data })",
    "● Search(useEffect dependency array)  ⎿  3 matches",
    "    const handleSubmit = useCallback(async () => {",
];

fn draw_detail(f: &mut Frame, area: Rect, app: &App) {
    let body = match app.current_detail() {
        Some(d) => format!("{} · 赞{}\n\n{}", d.author, d.voteup, d.body),
        None => "(无内容)".to_string(),
    };
    let dim = Style::default().fg(Color::DarkGray);
    let mut lines: Vec<Line> = Vec::new();
    if app.camouflage {
        // Real text in default color (readable); at each paragraph break drop in a
        // dim decoy code line. Deterministic rotation keeps decoys stable on redraw.
        let mut decoy_i = 0usize;
        let mut prev_blank = true; // suppress a leading decoy
        for src in body.split('\n') {
            if src.is_empty() {
                if !prev_blank {
                    lines.push(Line::from(Span::styled(
                        DECOYS[decoy_i % DECOYS.len()].to_string(),
                        dim,
                    )));
                    decoy_i += 1;
                }
                lines.push(Line::from(""));
                prev_blank = true;
            } else {
                lines.push(Line::from(src.to_string()));
                prev_blank = false;
            }
        }
    } else {
        for src in body.split('\n') {
            lines.push(Line::from(src.to_string()));
        }
    }
    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
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
        .scroll((app.comment_scroll, 0));
    f.render_widget(p, area);
}

/// Innocuous "I'm definitely working" screen shown when the boss key is pressed.
/// Looks like a Next.js production build sitting idle in a terminal.
fn draw_boss(f: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from("$ pnpm build"),
        Line::from(""),
        Line::from("> frontend@0.1.0 build"),
        Line::from("> next build"),
        Line::from(""),
        Line::from("   ▲ Next.js 14.2.3"),
        Line::from(""),
        Line::from("   Creating an optimized production build ..."),
        Line::from(Span::styled(" ✓ Compiled successfully", Style::default().fg(Color::Green))),
        Line::from("   Linting and checking validity of types ..."),
        Line::from("   Collecting page data ..."),
        Line::from("   Generating static pages (38/48) ..."),
        Line::from(""),
        Line::from("Route (app)                                Size     First Load JS"),
        Line::from("┌ ○ /                                      1.21 kB         96.3 kB"),
        Line::from("├ ○ /admin/personal/profile                3.44 kB          112 kB"),
        Line::from("├ ○ /admin/users                           2.18 kB          104 kB"),
        Line::from("└ ○ /dashboard                             5.07 kB          128 kB"),
        Line::from(""),
        Line::from(Span::styled("   Compiling /admin/personal/profile ...", Style::default().fg(Color::DarkGray))),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_command_bar(f: &mut Frame, area: Rect, app: &App) {
    let hint = match app.screen() {
        Screen::Root => "/zhihu 进入知乎   /quit 退出".to_string(),
        Screen::Login => "粘贴 Cookie 后回车   Esc 返回".to_string(),
        Screen::List => {
            let feed = match &app.list_source {
                ListSource::Recommend => "推荐".to_string(),
                ListSource::Hot => "热榜".to_string(),
                ListSource::Search(q) => format!("搜索:{q}"),
            };
            let load = if app.loading { " (加载中…)" } else { "" };
            format!("{feed}{load}   ↑↓选择 Enter进入 r刷新 /hot /search ←返回 q退出")
        }
        Screen::Detail => "↑↓滚动 n/p切换回答 数字键开图 c伪装 →评论 ←返回".to_string(),
        Screen::Comments => "↑↓滚动 ←返回".to_string(),
    };
    let cmd_line = if let Some(e) = &app.error {
        Line::from(Span::styled(format!("> {e}"), Style::default().fg(Color::Red)))
    } else {
        Line::from(format!("> {}", app.command))
    };
    let text = vec![
        Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray))),
        cmd_line,
    ];
    f.render_widget(Paragraph::new(text), area);
}
