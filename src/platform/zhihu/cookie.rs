/// Extract a cookie field's value from a raw `Cookie:` header string.
/// Surrounding double-quotes (Zhihu wraps `d_c0` in quotes) are stripped.
/// Only the first `=` separates key from value, so values containing `=`
/// (like Zhihu's `d_c0`) are preserved intact.
pub fn field(cookie: &str, name: &str) -> Option<String> {
    for pair in cookie.split(';') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=') {
            if k.trim() == name {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_field_value() {
        let c = "_xsrf=foo; d_c0=\"AB12|169\"; z_c0=bar";
        assert_eq!(field(c, "d_c0").as_deref(), Some("AB12|169"));
        assert_eq!(field(c, "z_c0").as_deref(), Some("bar"));
    }

    #[test]
    fn missing_field_is_none() {
        assert_eq!(field("a=1; b=2", "d_c0"), None);
    }

    #[test]
    fn value_containing_equals_is_preserved() {
        // Zhihu's d_c0 value commonly contains internal '=' and '|' characters.
        let c = "d_c0=AFCSc2wb_RmPTtdAxKxyQyok=|1739262461; z_c0=x";
        assert_eq!(field(c, "d_c0").as_deref(), Some("AFCSc2wb_RmPTtdAxKxyQyok=|1739262461"));
    }
}
