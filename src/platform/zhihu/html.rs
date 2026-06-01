use ego_tree::iter::Edge;
use scraper::Html;
use scraper::node::Node;

/// Convert Zhihu answer/comment HTML into plain terminal text.
/// - `<p>` blocks are separated by blank lines
/// - `<br>` becomes a newline
/// - `<img>` becomes `[图片]`
/// - All other tags are stripped but their text content is kept
pub fn to_text(html: &str) -> String {
    let frag = Html::parse_fragment(html);
    let mut out = String::new();
    for edge in frag.tree.root().traverse() {
        if let Edge::Open(node) = edge {
            match node.value() {
                Node::Text(t) => out.push_str(t),
                Node::Element(e) => match e.name() {
                    "br" => out.push('\n'),
                    "img" => out.push_str("[图片]"),
                    "p" if !out.is_empty() => out.push_str("\n\n"),
                    _ => {}
                },
                _ => {}
            }
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraphs_become_blank_line_separated() {
        assert_eq!(to_text("<p>第一段</p><p>第二段</p>"), "第一段\n\n第二段");
    }

    #[test]
    fn breaks_and_images() {
        assert_eq!(to_text("行一<br/>行二<img src=\"x\"/>"), "行一\n行二[图片]");
    }

    #[test]
    fn strips_other_tags_keeps_text() {
        assert_eq!(to_text("<b>粗</b><a href=\"u\">链接</a>"), "粗链接");
    }

    #[test]
    fn handles_real_answer_html_without_panicking() {
        // A snippet shaped like real Zhihu answer content.
        let html = "<p>开头</p><p>中间<b>加粗</b>和<a href=\"https://x\">链接</a></p><figure><img src=\"https://pic\"/></figure><p>结尾</p>";
        let out = to_text(html);
        assert!(out.contains("开头"));
        assert!(out.contains("加粗"));
        assert!(out.contains("[图片]"));
        assert!(out.contains("结尾"));
    }
}
