/// Parse a slash-command line into a `Command`.
pub fn parse(line: &str) -> Command {
    let line = line.trim();
    let (head, rest) = match line.split_once(char::is_whitespace) {
        Some((h, r)) => (h, r.trim()),
        None => (line, ""),
    };
    match head {
        "/zhihu" => Command::Zhihu,
        "/v2ex" => Command::V2ex,
        "/hupu" => Command::Hupu,
        "/nga" => Command::Nga,
        "/linuxdo" => Command::LinuxDo,
        "/stock" => Command::Stock,
        "/hot" => Command::Hot,
        "/refresh" => Command::Refresh,
        "/login" => Command::Login,
        "/help" | "/?" | "/h" => Command::Help,
        "/back" => Command::Back,
        "/quit" => Command::Quit,
        "/search" if !rest.is_empty() => Command::Search(rest.to_string()),
        "/add" if !rest.is_empty() => Command::Add(rest.to_string()),
        "/delete" if !rest.is_empty() => Command::Delete(rest.to_string()),
        _ => Command::Unknown(line.to_string()),
    }
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Zhihu,
    V2ex,
    Hupu,
    Nga,
    LinuxDo,
    Stock,
    Hot,
    Refresh,
    Search(String),
    Add(String),
    Delete(String),
    Login,
    Help,
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
        assert_eq!(parse("/hot"), Command::Hot);
        assert_eq!(parse("/stock"), Command::Stock);
        assert_eq!(parse("/refresh"), Command::Refresh);
        assert_eq!(parse("/login"), Command::Login);
        assert_eq!(parse("/help"), Command::Help);
        assert_eq!(parse("/?"), Command::Help);
        assert_eq!(parse("/back"), Command::Back);
        assert_eq!(parse("/quit"), Command::Quit);
        assert_eq!(parse("/search 程序员 摸鱼"), Command::Search("程序员 摸鱼".into()));
        assert_eq!(parse("/add SPCX"), Command::Add("SPCX".into()));
        assert_eq!(parse("/delete 159941"), Command::Delete("159941".into()));
    }

    #[test]
    fn unknown_and_whitespace() {
        assert_eq!(parse("  /zhihu  "), Command::Zhihu);
        assert_eq!(parse("/foo"), Command::Unknown("/foo".into()));
        assert_eq!(parse("/search"), Command::Unknown("/search".into())); // needs an arg
        assert_eq!(parse("/add"), Command::Unknown("/add".into()));
    }
}
