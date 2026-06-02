#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Root,
    Login,
    List,     // hot list or search results
    Detail,
    Comments,
    Help,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::platform::ListEntry;

    fn entry(t: &str) -> ListEntry {
        ListEntry { title: t.into(), subtitle: String::new(), open_token: Some("1".to_string()), detail: None }
    }

    #[test]
    fn starts_at_root() {
        let app = App::new();
        assert_eq!(app.screen(), &Screen::Root);
    }

    #[test]
    fn push_and_back_navigates_the_stack() {
        let mut app = App::new();
        app.push(Screen::List);
        app.push(Screen::Detail);
        assert_eq!(app.screen(), &Screen::Detail);
        app.back();
        assert_eq!(app.screen(), &Screen::List);
        app.back();
        assert_eq!(app.screen(), &Screen::Root);
        app.back(); // must not pop past root
        assert_eq!(app.screen(), &Screen::Root);
    }

    #[test]
    fn list_cursor_moves_within_bounds() {
        let mut app = App::new();
        app.set_list(vec![entry("a"), entry("b"), entry("c")]);
        app.push(Screen::List);
        assert_eq!(app.list_cursor(), 0);
        app.cursor_down(); app.cursor_down(); app.cursor_down(); // clamps at 2
        assert_eq!(app.list_cursor(), 2);
        app.cursor_up(); app.cursor_up(); app.cursor_up(); // clamps at 0
        assert_eq!(app.list_cursor(), 0);
    }

    #[test]
    fn replace_swaps_top_of_stack() {
        let mut app = App::new();
        app.replace(Screen::Login);
        assert_eq!(app.screen(), &Screen::Login);
        // replace does not grow the stack: back from a replaced root stays put
        app.back();
        assert_eq!(app.screen(), &Screen::Login);
    }
}
