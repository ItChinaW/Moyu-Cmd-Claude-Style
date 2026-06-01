/// Parse a slash-command line into a `Command`.
pub fn parse(line: &str) -> Command {
    let line = line.trim();
    let (head, rest) = match line.split_once(char::is_whitespace) {
        Some((h, r)) => (h, r.trim()),
        None => (line, ""),
    };
    match head {
        "/zhihu" => Command::Zhihu,
        "/login" => Command::Login,
        "/back" => Command::Back,
        "/quit" => Command::Quit,
        "/search" if !rest.is_empty() => Command::Search(rest.to_string()),
        _ => Command::Unknown(line.to_string()),
    }
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Zhihu,
    Search(String),
    Login,
    Back,
    Quit,
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_commands() {
        assert_eq!(parse("/zhihu"), Command::Zhihu);
        assert_eq!(parse("/login"), Command::Login);
        assert_eq!(parse("/back"), Command::Back);
        assert_eq!(parse("/quit"), Command::Quit);
        assert_eq!(parse("/search 程序员 摸鱼"), Command::Search("程序员 摸鱼".into()));
    }

    #[test]
    fn unknown_and_whitespace() {
        assert_eq!(parse("  /zhihu  "), Command::Zhihu);
        assert_eq!(parse("/foo"), Command::Unknown("/foo".into()));
        assert_eq!(parse("/search"), Command::Unknown("/search".into())); // needs an arg
    }
}
