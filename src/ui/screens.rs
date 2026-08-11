use crate::app::App;
use crate::app::state::Screen;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
};

pub fn draw(f: &mut Frame, app: &App) {
    if app.boss_mode {
        draw_boss(f, f.area());
        return;
    }
    let footer_height = if *app.screen() == Screen::Reader { 2 } else { 1 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(footer_height)])
        .split(f.area());
    match app.screen() {
        Screen::Root => draw_root(f, chunks[0], app),
        Screen::Login => draw_login(f, chunks[0], app),
        Screen::List => draw_list(f, chunks[0], app),
        Screen::Detail => draw_detail(f, chunks[0], app),
        Screen::Comments => draw_comments(f, chunks[0], app),
        Screen::Help => draw_help(f, chunks[0]),
        Screen::BookDirectory => draw_book_directory(f, chunks[0], app),
        Screen::Books => draw_books(f, chunks[0], app),
        Screen::Chapters => draw_chapters(f, chunks[0], app),
        Screen::Reader => draw_reader(f, chunks[0], app),
    }
    draw_command_bar(f, chunks[1], app);
}

fn draw_root(f: &mut Frame, area: Rect, app: &App) {
    let mut lines = vec![
        Line::from(Span::styled("选择入口（↑↓ 选择，回车进入）", Style::default().fg(Color::DarkGray))),
        Line::from(""),
    ];
    let selected_books = app.root_is_books();
    lines.push(Line::from(Span::styled(
        format!("{}电子书阅读", if selected_books { "> " } else { "  " }),
        if selected_books { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default() },
    )));
    for (i, p) in crate::platform::Platform::ALL.iter().enumerate() {
        let selected = i + 1 == app.root_cursor;
        let marker = if selected { "> " } else { "  " };
        let mut label = format!("{marker}{}", p.label());
        if p.needs_cookie() { label.push_str("  (需 cookie)"); }
        let style = if selected {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(label, style)));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_book_directory(f: &mut Frame, area: Rect, app: &App) {
    let previous = if app.book_directory.is_empty() {
        "还没有设置过书库目录".to_string()
    } else {
        format!("上次书库: {}", app.book_directory)
    };
    let lines = vec![
        Line::from(Span::styled("电子书阅读", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("请输入电子书所在目录，程序会递归加载常见格式："),
        Line::from("EPUB · PDF · TXT · Markdown · HTML · DOCX · ODT · RTF · FB2 · MOBI/AZW"),
        Line::from(""),
        Line::from(Span::styled(previous, Style::default().fg(Color::DarkGray))),
        Line::from(""),
        Line::from(format!("目录 > {}", app.command)),
        Line::from("回车或 f 弹出系统目录选择器 · Esc 返回 · 也可直接输入路径"),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn draw_books(f: &mut Frame, area: Rect, app: &App) {
    let mut header = vec![Line::from(vec![
        Span::styled("电子书书架", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" · {}", app.book_directory), Style::default().fg(Color::DarkGray)),
    ]), Line::from("")];
    if app.books.is_empty() {
        header.push(Line::from("没有找到电子书。按 Esc 重新选择目录。"));
        f.render_widget(Paragraph::new(header), area);
        return;
    }
    let items: Vec<ListItem> = app.books.iter().enumerate().map(|(i, book)| {
        let selected = i == app.book_cursor();
        let marker = if selected { "> " } else { "  " };
        let style = if selected { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default() };
        let summary = if let Some(error) = &book.error {
            format!("     {} · 解析失败: {}", book.format.to_ascii_uppercase(), one_line(error, 60))
        } else {
            format!("     {} · {} 章 · {} 字", book.format.to_ascii_uppercase(), book.chapter_count(), book.char_count())
        };
        ListItem::new(vec![
            Line::from(vec![Span::raw(marker), Span::styled(format!("{}. {}", i + 1, book.title), style)]),
            Line::from(Span::styled(summary, Style::default().fg(Color::DarkGray))),
        ])
    }).collect();
    let mut state = ListState::default();
    state.select(Some(app.book_cursor()));
    f.render_widget(Paragraph::new(header), area);
    let list_area = Rect { x: area.x, y: area.y.saturating_add(2), width: area.width, height: area.height.saturating_sub(2) };
    f.render_stateful_widget(List::new(items), list_area, &mut state);
}

fn draw_chapters(f: &mut Frame, area: Rect, app: &App) {
    let Some(book) = app.current_book() else {
        f.render_widget(Paragraph::new("没有选中的书籍"), area);
        return;
    };
    let progress = crate::book::load_progress(&book.id);
    let header = vec![
        Line::from(Span::styled(format!("《{}》 · 选择章节", book.title), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled(format!("共 {} 章 · 默认从上次阅读位置继续", book.chapter_count()), Style::default().fg(Color::DarkGray))),
        Line::from(""),
    ];
    let items: Vec<ListItem> = book.chapters.iter().enumerate().map(|(i, chapter)| {
        let selected = i == app.chapter_cursor;
        let marker = if selected { "> " } else { "  " };
        let resume = if i == progress.chapter { "  · 上次读到这里" } else { "" };
        let style = if selected { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default() };
        ListItem::new(Line::from(vec![
            Span::raw(marker), Span::styled(format!("{}. {}", i + 1, chapter.title), style),
            Span::styled(resume, Style::default().fg(Color::Yellow)),
        ]))
    }).collect();
    let mut state = ListState::default();
    state.select(Some(app.chapter_cursor));
    f.render_widget(Paragraph::new(header), area);
    let list_area = Rect { x: area.x, y: area.y.saturating_add(3), width: area.width, height: area.height.saturating_sub(3) };
    f.render_stateful_widget(List::new(items), list_area, &mut state);
}

fn draw_reader(f: &mut Frame, area: Rect, app: &App) {
    let Some(chapter) = app.current_chapter() else {
        f.render_widget(Paragraph::new("没有可阅读的章节"), area);
        return;
    };
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(chapter.title.clone(), Style::default().fg(Color::Yellow))),
        Line::from(""),
    ];
    for (i, paragraph) in chapter.paragraphs.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
            if app.camouflage && i % 7 == 0 { lines.extend(decoy_block(i / 7)); lines.push(Line::from("")); }
        }
        lines.push(Line::from(paragraph.clone()));
    }
    let p = Paragraph::new(lines).wrap(Wrap { trim: false }).scroll((app.reader_scroll, 0));
    f.render_widget(p, area);
}

fn draw_login(f: &mut Frame, area: Rect, app: &App) {
    // The login screen serves every cookie-gated platform (知乎/NGA/Linux.do), so
    // name the one being logged into rather than hardcoding 知乎.
    let plat = app.pending_login_platform.unwrap_or(app.active_platform).label();
    let msg = if let Some(e) = &app.error {
        format!("登录失败: {e}\n请重新粘贴 {plat} Cookie 后回车")
    } else if app.loading {
        "验证中…".to_string()
    } else {
        format!("未检测到 {plat} 登录态。浏览器 F12 → Network → 任意 {plat} 请求复制 Cookie，粘贴后回车。")
    };
    f.render_widget(Paragraph::new(msg).wrap(Wrap { trim: true }), area);
}

fn draw_list(f: &mut Frame, area: Rect, app: &App) {
    if app.camouflage && app.active_platform != crate::platform::Platform::Stock {
        draw_list_todos(f, area, app);
        return;
    }
    if app.active_platform == crate::platform::Platform::Stock {
        draw_stock_list(f, area, app);
        return;
    }
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
    // ListState keeps the selected row scrolled into view as the cursor moves.
    let mut state = ListState::default();
    if !app.list.is_empty() { state.select(Some(app.list_cursor())); }
    f.render_stateful_widget(List::new(items), area, &mut state);
}

fn pad_to_width(s: &str, cols: usize) -> String {
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = if ch.is_ascii() { 1 } else { 2 };
        if w + cw > cols {
            break;
        }
        out.push(ch);
        w += cw;
    }
    if w < cols {
        out.push_str(&" ".repeat(cols - w));
    }
    out
}

fn draw_stock_list(f: &mut Frame, area: Rect, app: &App) {
    if app.list.is_empty() {
        let lines = vec![
            Line::from(Span::styled("股票自选为空", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from("  /add 159941    添加 A 股"),
            Line::from("  /add SPCX      添加美股"),
            Line::from("  /delete SPCX   删除自选"),
            Line::from(""),
            Line::from(Span::styled("一行两列展示，列表会通过 WS 自动实时更新。", Style::default().fg(Color::DarkGray))),
        ];
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        return;
    }
    let col_w = (area.width as usize).saturating_sub(3) / 2;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let row_count = app.list.len().div_ceil(2);
    for row in 0..row_count {
        let left_idx = row * 2;
        let right_idx = left_idx + 1;
        let left = render_stock_cell(app, left_idx, col_w);
        let right = render_stock_cell(app, right_idx, col_w);
        lines.push(Line::from(format!("{}   {}", left.0, right.0)));
        lines.push(Line::from(format!("{}   {}", left.1, right.1)));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn render_stock_cell(app: &App, idx: usize, width: usize) -> (String, String) {
    let Some(item) = app.list.get(idx) else {
        return (" ".repeat(width), " ".repeat(width));
    };
    let selected = idx == app.list_cursor();
    let marker = if selected { "> " } else { "  " };
    let title_w = width.saturating_sub(marker.len());
    let line1 = pad_to_width(&format!("{marker}{}", item.title), width);
    let line2 = pad_to_width(&format!("  {}", item.subtitle), title_w + 2);
    (line1, line2)
}

/// Truncate to a single display line of at most `cols` terminal columns, cutting
/// at the first newline and appending `…` if clipped. CJK/wide chars count as 2.
fn one_line(s: &str, cols: usize) -> String {
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        if ch == '\n' || ch == '\r' {
            out.push('…');
            return out;
        }
        let cw = if ch.is_ascii() { 1 } else { 2 };
        if w + cw > cols {
            out.push('…');
            return out;
        }
        out.push(ch);
        w += cw;
    }
    out
}

/// Camouflage skin for the hot list: renders entries as a Claude Code todo list
/// (`● Update Todos` with ☒/◐/☐ checkboxes). Items above the cursor read as done,
/// the cursor is the in-progress task, the rest are pending — so scrolling looks
/// like work advancing. Subtitles are clipped to one line so more entries fit, and
/// a decoy tool-call block is dropped between every group so the page reads like a
/// real session interleaving todo updates with git/diff work.
fn draw_list_todos(f: &mut Frame, area: Rect, app: &App) {
    const GROUP: usize = 5;
    let dim = Style::default().fg(Color::DarkGray);
    let cursor = app.list_cursor();
    // Width budget for the one-line subtitle (8-space indent + small margin).
    let sub_cols = (area.width as usize).saturating_sub(10).max(20);
    // Title budget leaves room for the connector (5) + checkbox (2) so a long
    // title never wraps — keeps the line count exact for cursor-follow scrolling.
    let title_cols = (area.width as usize).saturating_sub(8).max(10);
    // Line index (within `lines`) of the cursor's todo row, for scroll-to-cursor.
    let mut cursor_line = 0usize;
    let header = || {
        Line::from(vec![
            Span::styled("● ", Style::default().fg(Color::Green)),
            Span::styled("Update Todos", Style::default().add_modifier(Modifier::BOLD)),
        ])
    };
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (g, chunk) in app.list.chunks(GROUP).enumerate() {
        if g > 0 {
            // Interleave a Claude-Code tool-call block between todo groups.
            lines.push(Line::from(""));
            lines.extend(decoy_block(g - 1));
            lines.push(Line::from(""));
        }
        lines.push(header());
        for (j, e) in chunk.iter().enumerate() {
            let i = g * GROUP + j;
            let connector = if j == 0 { "  ⎿  " } else { "     " };
            let (mark, style) = if i < cursor {
                ("☒ ", dim)
            } else if i == cursor {
                ("◐ ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            } else {
                ("☐ ", Style::default().fg(Color::Gray))
            };
            if i == cursor { cursor_line = lines.len(); }
            lines.push(Line::from(vec![
                Span::raw(connector),
                Span::styled(mark, style),
                Span::styled(one_line(&e.title, title_cols), style),
            ]));
            if !e.subtitle.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("        {}", one_line(&e.subtitle, sub_cols)),
                    dim,
                )));
            }
        }
    }
    // Scroll so the cursor row stays on screen (with ~3 rows of trailing context)
    // as the selection moves past the bottom; the list itself can be far taller
    // than the viewport once "load more" has accumulated pages.
    let view_h = area.height as usize;
    let scroll = cursor_line.saturating_sub(view_h.saturating_sub(3)) as u16;
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).scroll((scroll, 0)),
        area,
    );
}

/// One authentic-looking Claude Code transcript block (rotated by index), styled to
/// match a real session: colored ● bullets, bold tool names, ⎿ result lines,
/// Error/Exit codes, git diff stats, "+N lines (ctrl+o to expand)". Uses the user's
/// real internal git remote and project paths so a passing glance reads as coding.
fn decoy_block(i: usize) -> Vec<Line<'static>> {
    let red = Style::default().fg(Color::Red);
    let green = Style::default().fg(Color::Green);
    let dim = Style::default().fg(Color::DarkGray);
    let pink = Style::default().fg(Color::LightRed);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    // Dark red/green diff bars (truecolor; old terminals fall back to fg only).
    let del = Style::default().fg(Color::Rgb(245, 215, 215)).bg(Color::Rgb(74, 20, 20));
    let add = Style::default().fg(Color::Rgb(215, 245, 220)).bg(Color::Rgb(18, 58, 30));
    // Trailing-pad each diff row so the colored bar reads as a full line.
    let row = |s: &str, st: Style| Line::from(Span::styled(format!("{s:<64}"), st));
    let bullet = |c: Color| Span::styled("● ", Style::default().fg(c));
    let repo = "http://gitlab.internal:3000/web/admin-portal.git";
    match i % 7 {
        0 => vec![Line::from(vec![
            bullet(Color::Gray),
            Span::styled("Changes look consistent. Committing:", Style::default().fg(Color::Gray)),
        ])],
        1 => vec![
            Line::from(vec![bullet(Color::Red), Span::styled("Bash", bold),
                Span::raw("(git add -u && git commit -m \"preserve dark CTA button backgrounds…\")")]),
            Line::from(vec![Span::styled("  ⎿  ", dim), Span::styled("Error: Exit code 1", red)]),
            Line::from(Span::styled("     [dev 2b88b813] preserve dark CTA button backgrounds on hover across admin", pink)),
            Line::from(Span::styled("      18 files changed, 34 insertions(+), 34 deletions(-)", dim)),
            Line::from(Span::styled("  … +16 lines (ctrl+o to expand)", dim)),
        ],
        2 => vec![
            Line::from(vec![bullet(Color::Green), Span::styled("Bash", bold),
                Span::raw("(git diff app/dashboard/profile/page.tsx | head -60)")]),
            Line::from(Span::styled("  ⎿  diff --git a/app/dashboard/profile/page.tsx", dim)),
            Line::from(Span::styled("     index 10c01820..7c3faab1 100644", dim)),
            Line::from(Span::styled("  … +57 lines (ctrl+o to expand)", dim)),
        ],
        3 => vec![
            Line::from(vec![bullet(Color::Red), Span::styled("Bash", bold),
                Span::raw("(git pull --rebase && git push)")]),
            Line::from(vec![Span::styled("  ⎿  ", dim), Span::styled("Error: Exit code 1", red)]),
            Line::from(Span::styled(format!("     fatal: unable to access '{repo}/': Empty reply from server"), pink)),
        ],
        4 => vec![
            Line::from(vec![bullet(Color::Green), Span::styled("Update", bold),
                Span::raw(" components/PriceForm.tsx")]),
            Line::from(Span::styled("  ⎿  Updated with 2 additions and 1 removal", dim)),
            Line::from(Span::styled("       +   const [loading, setLoading] = useState(false)", green)),
            Line::from(Span::styled("       -   onClose={() => setOpen(false)}", red)),
        ],
        5 => vec![
            Line::from(vec![bullet(Color::Red), Span::styled("Bash", bold),
                Span::raw("(sleep 5 && git push 2>&1; git status)")]),
            Line::from(vec![Span::styled("  ⎿  ", dim), Span::styled("Error: Exit code 128", red)]),
            Line::from(Span::styled(format!("     fatal: unable to access '{repo}/': Empty reply from server"), pink)),
        ],
        _ => vec![
            Line::from(vec![bullet(Color::Green), Span::styled("Update", bold),
                Span::raw("(app/admin/finance/page.tsx)")]),
            Line::from(Span::styled("  ⎿  Added 2 lines, removed 4 lines", dim)),
            row("    367 -        styles: {", del),
            row("    368 -          root: { maxWidth: 360 },", del),
            row("    369 -          body: { maxHeight: \"40vh\", overflowY: \"auto\" },", del),
            row("    370 -        },", del),
            row("    367 +        overlayStyle: { maxWidth: 360 },", add),
            row("    368 +        overlayInnerStyle: { maxHeight: \"40vh\", overflowY: \"auto\" },", add),
        ],
    }
}

fn draw_detail(f: &mut Frame, area: Rect, app: &App) {
    let (header, body) = match app.current_detail() {
        Some(d) => (format!("● {} · 赞{}", d.author, d.voteup), d.body.clone()),
        None => ("●".to_string(), "(无内容)".to_string()),
    };
    let mut lines: Vec<Line<'static>> = Vec::new();
    if app.camouflage {
        // Real answer text stays bright (reads like Claude's narration); between
        // paragraphs we drop in a colored Claude-Code tool-call block.
        lines.push(Line::from(Span::styled(header, Style::default().fg(Color::Gray))));
        lines.push(Line::from(""));
        let mut block_i = 0usize;
        let mut prev_blank = true;
        for src in body.split('\n') {
            if src.is_empty() {
                if !prev_blank {
                    lines.push(Line::from(""));
                    lines.extend(decoy_block(block_i));
                    lines.push(Line::from(""));
                    block_i += 1;
                }
                prev_blank = true;
            } else {
                lines.push(Line::from(src.to_string()));
                prev_blank = false;
            }
        }
    } else {
        lines.push(Line::from(header));
        lines.push(Line::from(""));
        for src in body.split('\n') {
            lines.push(Line::from(src.to_string()));
        }
    }
    append_image_section(&mut lines, app);
    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    f.render_widget(p, area);
}

/// Render the current answer's images as a footer of local file paths. Editors
/// (VS Code etc.) turn an existing absolute path into a clickable link that opens
/// the image in an editor tab — a remote URL would only open a browser, so we show
/// the downloaded local path. Falls back to digit keys 1-9.
fn append_image_section(lines: &mut Vec<Line<'static>>, app: &App) {
    let dim = Style::default().fg(Color::DarkGray);
    let Some(d) = app.current_detail() else { return };
    if d.images.is_empty() {
        return;
    }
    let ready = app.image_owner == d.answer_id;
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "──── 图片(点击路径在编辑器打开 · 或按数字键 1-9)────",
        dim,
    )));
    for i in 0..d.images.len() {
        let label = format!("【图{}】 ", i + 1);
        let value = match (ready, app.image_paths.get(i)) {
            (true, Some(p)) if !p.is_empty() => {
                Span::styled(p.clone(), Style::default().fg(Color::Cyan))
            }
            (true, _) => Span::styled("(下载失败)".to_string(), dim),
            (false, _) => Span::styled("下载中…".to_string(), dim),
        };
        lines.push(Line::from(vec![Span::styled(label, dim), value]));
    }
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
        Line::from("├ ○ /dashboard/profile                3.44 kB          112 kB"),
        Line::from("├ ○ /dashboard/users                           2.18 kB          104 kB"),
        Line::from("└ ○ /dashboard                             5.07 kB          128 kB"),
        Line::from(""),
        Line::from(Span::styled("   Compiling /dashboard/profile ...", Style::default().fg(Color::DarkGray))),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

/// Low-key bottom prompt — looks like a shell line. All commands/keys live in /help.
fn draw_command_bar(f: &mut Frame, area: Rect, app: &App) {
    if *app.screen() == Screen::Reader {
        let status = reader_status(app).unwrap_or_else(|| "电子书 · 无阅读状态".into());
        let hint = if !app.reader_image_input.is_empty() {
            Span::styled(
                format!(
                    "待打开图片 {} · Enter 打开 · Backspace 修改 · Esc 取消并返回",
                    app.reader_image_input
                ),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )
        } else if let Some(error) = &app.error {
            Span::styled(format!("> {error}"), Style::default().fg(Color::Red))
        } else {
            Span::styled(
                "-/－ 上一章  =/＝/+/＋ 下一章  ↑↓ 滚动  PgUp/PgDn 翻页  Shift+数字 选图片  Enter 打开  Esc 返回",
                Style::default().fg(Color::DarkGray),
            )
        };
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    status,
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )),
                Line::from(hint),
            ]),
            area,
        );
        return;
    }
    let line = if let Some(e) = &app.error {
        Line::from(Span::styled(format!("> {e}"), Style::default().fg(Color::Red)))
    } else if app.loading {
        Line::from(Span::styled("> …".to_string(), Style::default().fg(Color::DarkGray)))
    } else if app.active_platform == crate::platform::Platform::Stock {
        Line::from(vec![
            Span::styled("股票 · ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("> {}", app.command)),
            Span::styled("   /add 代码  /delete 代码  `老板键", Style::default().fg(Color::DarkGray)),
        ])
    } else if app.camouflage {
        Line::from(format!("> {}", app.command))
    } else {
        // Non-camouflaged: surface the active platform as a status prefix.
        Line::from(vec![
            Span::styled(format!("{} · ", app.active_platform.label()), Style::default().fg(Color::DarkGray)),
            Span::raw(format!("> {}", app.command)),
        ])
    };
    f.render_widget(Paragraph::new(line), area);
}

fn reader_status(app: &App) -> Option<String> {
    let book = app.current_book()?;
    let chapter = app.current_chapter()?;
    let offset = app.reader_offset.min(chapter.char_count());
    let remaining = chapter.char_count().saturating_sub(offset);
    let (paragraph, paragraph_offset) = book.position_for_offset(app.reader_chapter, offset);
    let read = book.read_count(app.reader_chapter, paragraph, paragraph_offset);
    Some(format!(
        "《{}》 · 第 {}/{} 章 · 段落 {}/{} · 已读 {} 字 · 本章剩余 {} 字 · 全书 {} 字 · 图片 {} 张",
        book.title,
        app.reader_chapter + 1,
        book.chapter_count(),
        paragraph + 1,
        chapter.paragraphs.len(),
        read,
        remaining,
        book.char_count(),
        chapter.images.len(),
    ))
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
        kv("/v2ex", "V2EX"),
        kv("/hupu", "虎扑"),
        kv("/nga", "NGA(需 cookie)"),
        kv("/linuxdo", "Linux.do(需 cookie)"),
        kv("/stock", "股票自选(A股/美股)"),
        kv("/add 代码", "添加自选,如 /add SPCX"),
        kv("/delete 代码", "删除自选,如 /delete 159941"),
        kv("/hot", "热榜"),
        kv("/search 词", "搜索"),
        kv("/refresh", "刷新当前列表"),
        kv("/login", "重新登录 / 切换账号(粘贴新 Cookie)"),
        kv("/books [目录]", "打开电子书书架 / 更换书库目录"),
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
        kv("Tab / Shift-Tab", "阅读器切换下一章 / 上一章"),
        kv("Home / End", "跳到本章开头 / 结尾(阅读器)"),
        kv("- / =", "上一章 / 下一章(+、－、＋、＝亦可)"),
        kv("PageUp / PageDown", "上 / 下翻页(阅读器)"),
        kv("Shift+图片序号", "选择阅读器图片，再按 Enter 打开"),
        kv("1-9", "直接打开第 N 张图(知乎详情页)"),
        kv("c", "开关 Claude 伪装(详情页 / 阅读器)"),
        kv("r", "刷新(列表页)"),
        kv("q", "退出"),
        kv("` 或 ·", "老板键:一键隐藏 / 恢复(中英文输入法都可)"),
        Line::from(""),
        Line::from(Span::styled(" 按 ← 或 Esc 返回", dim)),
    ];
    f.render_widget(Paragraph::new(lines), area);
}
