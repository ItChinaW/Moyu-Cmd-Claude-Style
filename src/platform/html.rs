use ego_tree::iter::Edge;
use scraper::Html;
use scraper::node::Node;

/// Convert answer HTML to (text_with_image_markers, image_urls).
/// `<p>`→blank-line blocks, `<br>`→newline, `<img>`→`【图N】` marker (N is 1-based),
/// other tags stripped. Image URL is taken from data-original / data-actualsrc / src
/// (first that starts with http).
pub fn to_text_and_images(html: &str) -> (String, Vec<String>) {
    let frag = Html::parse_fragment(html);
    let mut out = String::new();
    let mut urls: Vec<String> = Vec::new();
    for edge in frag.tree.root().traverse() {
        if let Edge::Open(node) = edge {
            match node.value() {
                Node::Text(t) => out.push_str(t),
                Node::Element(e) => match e.name() {
                    "br" => out.push('\n'),
                    "img" => {
                        // Prefer data-original, then data-actualsrc, then src —
                        // only accept a value that starts with "http".
                        let url = ["data-original", "data-actualsrc", "src"]
                            .iter()
                            .find_map(|attr| {
                                let v = e.attr(attr)?;
                                if v.starts_with("http") { Some(v.to_string()) } else { None }
                            });
                        if let Some(u) = url {
                            urls.push(u);
                            out.push_str(&format!("【图{}】", urls.len()));
                        }
                    }
                    "p" if !out.is_empty() => out.push_str("\n\n"),
                    _ => {}
                },
                _ => {}
            }
        }
    }
    (out.trim().to_string(), urls)
}

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

    #[test]
    fn extracts_image_urls_and_markers() {
        let html = r#"<p>看图</p><img data-original="https://pic1.zhimg.com/a.jpg" src="data:image/svg+xml;base64,xxx"/><p>结束</p>"#;
        let (text, imgs) = to_text_and_images(html);
        assert_eq!(imgs, vec!["https://pic1.zhimg.com/a.jpg".to_string()]);
        assert!(text.contains("【图1】"));
        assert!(text.contains("看图"));
        assert!(text.contains("结束"));
    }

    #[test]
    fn skips_images_without_real_url() {
        let html = r#"<img src="data:image/svg+xml;base64,xxx"/>文字"#;
        let (text, imgs) = to_text_and_images(html);
        assert!(imgs.is_empty());
        assert!(!text.contains("【图"));
        assert!(text.contains("文字"));
    }

    #[test]
    fn falls_back_to_actualsrc_then_src() {
        let html = r#"<img data-actualsrc="https://pic2.zhimg.com/b.jpg"/><img src="https://pic3.zhimg.com/c.png"/>"#;
        let (_t, imgs) = to_text_and_images(html);
        assert_eq!(imgs, vec!["https://pic2.zhimg.com/b.jpg".to_string(), "https://pic3.zhimg.com/c.png".to_string()]);
    }
}
