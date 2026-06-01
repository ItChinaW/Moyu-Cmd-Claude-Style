pub mod screens;
use crate::app::App;
use ratatui::Frame;

/// Draw the whole UI for the current app state.
pub fn draw(f: &mut Frame, app: &App) {
    screens::draw(f, app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::Screen;
    use crate::platform::ListEntry;
    use ratatui::{backend::TestBackend, Terminal};

    fn render(app: &App) -> String {
        use unicode_width::UnicodeWidthStr;
        let backend = TestBackend::new(60, 12);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| draw(f, app)).unwrap();
        let buf = term.backend().buffer().clone();
        (0..buf.area.height).map(|y| {
            let mut line = String::new();
            let mut x = 0u16;
            while x < buf.area.width {
                let cell = &buf[(x, y)];
                let sym = cell.symbol();
                let w = sym.width() as u16;
                line.push_str(sym);
                // advance past spacer cells for wide characters
                x += w.max(1);
            }
            line
        }).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn root_screen_mentions_zhihu_command() {
        let app = App::new();
        assert!(render(&app).contains("/zhihu"));
    }

    #[test]
    fn list_screen_shows_titles_and_cursor() {
        let mut app = App::new();
        app.set_list(vec![
            ListEntry { title: "标题甲".into(), subtitle: String::new(), question_id: Some("1".into()) },
            ListEntry { title: "标题乙".into(), subtitle: String::new(), question_id: Some("2".into()) },
        ]);
        app.push(Screen::List);
        let screen = render(&app);
        assert!(screen.contains("标题甲"));
        assert!(screen.contains("标题乙"));
        assert!(screen.contains('>')); // cursor marker on the selected row
    }
}
