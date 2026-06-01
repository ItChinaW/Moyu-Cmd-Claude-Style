use crate::app::App;
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
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(f.area());
    match app.screen() {
        Screen::Root => draw_root(f, chunks[0]),
        Screen::Login => draw_login(f, chunks[0], app),
        Screen::List => draw_list(f, chunks[0], app),
        Screen::Detail => draw_detail(f, chunks[0], app),
        Screen::Comments => draw_comments(f, chunks[0], app),
        Screen::Help => draw_help(f, chunks[0]),
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

/// Low-key bottom prompt — looks like a shell line. All commands/keys live in /help.
fn draw_command_bar(f: &mut Frame, area: Rect, app: &App) {
    let line = if let Some(e) = &app.error {
        Line::from(Span::styled(format!("> {e}"), Style::default().fg(Color::Red)))
    } else if app.loading {
        Line::from(Span::styled("> …".to_string(), Style::default().fg(Color::DarkGray)))
    } else {
        Line::from(format!("> {}", app.command))
    };
    f.render_widget(Paragraph::new(line), area);
}

/// Full command + keybinding reference, opened with /help.
fn draw_help(f: &mut Frame, area: Rect) {
    let head = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let key = Style::default().fg(Color::Yellow);
    let dim = Style::default().fg(Color::DarkGray);
    let kv = |k: &str, v: &str| Line::from(vec![
        Span::styled(format!("  {k:<14}"), key),
        Span::raw(v.to_string()),
    ]);
    let lines = vec![
        Line::from(Span::styled("touch-fish · 帮助", head)),
        Line::from(""),
        Line::from(Span::styled(" 命令(在 > 后输入)", head)),
        kv("/zhihu", "进入知乎(推荐流·最新)"),
        kv("/hot", "热榜"),
        kv("/search 词", "搜索"),
        kv("/refresh", "刷新当前列表(也可按 r)"),
        kv("/login", "重新登录 / 切换账号(粘贴新 Cookie)"),
        kv("/help", "显示本帮助(/? 亦可)"),
        kv("/back", "返回上一级"),
        kv("/quit", "退出"),
        Line::from(""),
        Line::from(Span::styled(" 按键", head)),
        kv("↑ ↓", "选择 / 滚动"),
        kv("Enter", "进入选中项 / 执行命令"),
        kv("→ 或 Tab", "看评论(详情页)"),
        kv("← 或 Esc", "返回上一级"),
        kv("n / p", "上 / 下一个回答(详情页)"),
        kv("1-9", "在浏览器打开第 N 张图(详情页)"),
        kv("c", "开关 Claude 伪装(详情页)"),
        kv("r", "刷新(列表页)"),
        kv("q", "退出"),
        kv("` 或 ·", "老板键:一键隐藏 / 恢复(中英文输入法都可)"),
        Line::from(""),
        Line::from(Span::styled(" 按 ← 或 Esc 返回", dim)),
    ];
    f.render_widget(Paragraph::new(lines), area);
}
