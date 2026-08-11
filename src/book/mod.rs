use anyhow::{anyhow, Context, Result};
use ego_tree::iter::Edge;
use encoding_rs::{GBK, UTF_16BE, UTF_16LE, UTF_8};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use scraper::node::Node;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

const BOOK_CACHE_VERSION: u32 = 5;

/// Extensions that can be opened by the built-in reader. Newer proprietary
/// Kindle files fall back to Calibre when the built-in MOBI parser cannot read them.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "epub", "pdf", "txt", "text", "md", "markdown", "rst", "log", "html", "htm", "xhtml", "xml",
    "csv", "json", "tex", "docx", "odt", "rtf", "fb2", "mobi", "azw", "azw3",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chapter {
    pub title: String,
    pub paragraphs: Vec<String>,
    #[serde(default)]
    pub images: Vec<String>,
}

impl Chapter {
    pub fn char_count(&self) -> usize {
        self.paragraphs
            .iter()
            .map(|p| p.chars().count())
            .sum::<usize>()
            + self.paragraphs.len().saturating_sub(1) * 2
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Book {
    pub id: String,
    pub path: String,
    pub title: String,
    pub format: String,
    pub chapters: Vec<Chapter>,
    pub error: Option<String>,
}

impl Book {
    pub fn chapter_count(&self) -> usize {
        self.chapters.len()
    }

    pub fn char_count(&self) -> usize {
        self.chapters.iter().map(Chapter::char_count).sum()
    }

    /// Number of characters before a chapter/paragraph location.
    pub fn read_count(&self, chapter: usize, paragraph: usize, offset: usize) -> usize {
        let before_chapter: usize = self
            .chapters
            .iter()
            .take(chapter)
            .map(Chapter::char_count)
            .sum();
        let Some(current) = self.chapters.get(chapter) else {
            return before_chapter;
        };
        let before_paragraph: usize = current
            .paragraphs
            .iter()
            .take(paragraph)
            .map(|p| p.chars().count() + 2)
            .sum();
        before_chapter
            + before_paragraph
            + offset.min(
                current
                    .paragraphs
                    .get(paragraph)
                    .map_or(0, |p| p.chars().count()),
            )
    }

    pub fn position_for_offset(&self, chapter: usize, offset: usize) -> (usize, usize) {
        let Some(current) = self.chapters.get(chapter) else {
            return (0, 0);
        };
        let mut remaining = offset.min(current.char_count());
        for (i, paragraph) in current.paragraphs.iter().enumerate() {
            let len = paragraph.chars().count();
            if remaining <= len {
                return (i, remaining);
            }
            remaining = remaining.saturating_sub(len + 2);
        }
        (
            current.paragraphs.len().saturating_sub(1),
            current.paragraphs.last().map_or(0, |p| p.chars().count()),
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ReadingPosition {
    pub chapter: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedBook {
    version: u32,
    path: String,
    modified: u64,
    size: u64,
    book: Book,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProgressFile {
    books: HashMap<String, ReadingPosition>,
}

pub fn cache_root() -> PathBuf {
    if let Ok(path) = std::env::var("TOUCH_FISH_CACHE") {
        return PathBuf::from(path);
    }
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("touch-fish")
}

fn books_cache_dir() -> PathBuf {
    cache_root().join("books")
}
fn progress_path() -> PathBuf {
    cache_root().join("reading-progress.json")
}

pub fn load_progress(book_id: &str) -> ReadingPosition {
    let Ok(raw) = std::fs::read_to_string(progress_path()) else {
        return ReadingPosition::default();
    };
    serde_json::from_str::<ProgressFile>(&raw)
        .ok()
        .and_then(|file| file.books.get(book_id).cloned())
        .unwrap_or_default()
}

pub fn save_progress(book_id: &str, position: ReadingPosition) -> Result<()> {
    let path = progress_path();
    let mut file = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<ProgressFile>(&raw).ok())
        .unwrap_or_default();
    file.books.insert(book_id.to_string(), position);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create book cache")?;
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&file).context("serialize reading progress")?,
    )
    .context("write reading progress")
}

pub fn book_id(path: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Recursively scan a directory and parse each supported file. Parsed chapters
/// are cached by path + file metadata so reopening a large EPUB is instant.
pub fn load_directory(directory: &Path) -> Result<Vec<Book>> {
    if !directory.is_dir() {
        return Err(anyhow!(
            "电子书目录不存在或不是目录: {}",
            directory.display()
        ));
    }
    let root = directory
        .canonicalize()
        .unwrap_or_else(|_| directory.to_path_buf());
    let mut paths = Vec::new();
    collect_files(&root, &mut paths)?;
    paths.sort_by_key(|path| path.to_string_lossy().to_lowercase());

    let cache_dir = books_cache_dir();
    let _ = std::fs::create_dir_all(&cache_dir);
    let mut books = Vec::new();
    for path in paths {
        let metadata = match std::fs::metadata(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let id = book_id(&path);
        let cache_path = cache_dir.join(format!("{id}.json"));
        let stamp = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_secs());
        if let Ok(raw) = std::fs::read_to_string(&cache_path) {
            if let Ok(cached) = serde_json::from_str::<CachedBook>(&raw) {
                if cached.version == BOOK_CACHE_VERSION
                    && cached.path == path.to_string_lossy().as_ref()
                    && cached.modified == stamp
                    && cached.size == metadata.len()
                {
                    books.push(cached.book);
                    continue;
                }
            }
        }

        let fallback_title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("未命名书籍")
            .to_string();
        let format = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("未知")
            .to_ascii_lowercase();
        let (title, chapters, error) = match if format == "epub" {
            parse_epub(&path).map(|epub| (epub.title, epub.chapters))
        } else if matches!(format.as_str(), "mobi" | "azw" | "azw3") {
            parse_mobi(&path).map(|chapters| (fallback_title.clone(), chapters))
        } else {
            parse_file(&path, &format).map(|chapters| (fallback_title.clone(), chapters))
        } {
            Ok((title, chapters)) => (
                if title.trim().is_empty() {
                    fallback_title
                } else {
                    title
                },
                chapters,
                None,
            ),
            Err(error) => (fallback_title, Vec::new(), Some(error.to_string())),
        };
        let book = Book {
            id: id.clone(),
            path: path.to_string_lossy().into_owned(),
            title,
            format,
            chapters,
            error,
        };
        let cached = CachedBook {
            version: BOOK_CACHE_VERSION,
            path: book.path.clone(),
            modified: stamp,
            size: metadata.len(),
            book: book.clone(),
        };
        if let Ok(raw) = serde_json::to_vec(&cached) {
            let _ = std::fs::write(cache_path, raw);
        }
        books.push(book);
    }
    books.sort_by_key(|book| book.title.to_lowercase());
    Ok(books)
}

fn collect_files(directory: &Path, result: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("读取目录失败: {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name.starts_with('.'))
        {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, result)?;
        } else if path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| {
                SUPPORTED_EXTENSIONS
                    .iter()
                    .any(|supported| supported.eq_ignore_ascii_case(ext))
            })
        {
            result.push(path);
        }
    }
    Ok(())
}

fn parse_file(path: &Path, format: &str) -> Result<Vec<Chapter>> {
    match format {
        "pdf" => {
            let text = match pdf_extract::extract_text(path) {
                Ok(text) => text,
                Err(_) => external_text(path, "pdftotext").context("PDF 无法提取正文")?,
            };
            Ok(split_text(
                &text.replace('\u{000c}', "\n\n"),
                path.file_stem().and_then(|s| s.to_str()).unwrap_or("PDF"),
            ))
        }
        "docx" => parse_zip_xml(path, "word/document.xml"),
        "odt" => parse_zip_xml(path, "content.xml"),
        "fb2" => {
            let text = xml_text(&read_text(path)?, true);
            Ok(split_text(
                &text,
                path.file_stem().and_then(|s| s.to_str()).unwrap_or("FB2"),
            ))
        }
        "html" | "htm" | "xhtml" => {
            let text = crate::platform::html::to_text(&read_text(path)?);
            Ok(split_text(
                &text,
                path.file_stem().and_then(|s| s.to_str()).unwrap_or("网页"),
            ))
        }
        "xml" => {
            let text = xml_text(&read_text(path)?, true);
            Ok(split_text(
                &text,
                path.file_stem().and_then(|s| s.to_str()).unwrap_or("XML"),
            ))
        }
        "rtf" => Ok(split_text(
            &strip_rtf(&read_text(path)?),
            path.file_stem().and_then(|s| s.to_str()).unwrap_or("RTF"),
        )),
        "mobi" | "azw" | "azw3" => parse_mobi(path),
        _ => Ok(split_text(
            &read_text(path)?,
            path.file_stem().and_then(|s| s.to_str()).unwrap_or("文本"),
        )),
    }
}

fn parse_mobi(path: &Path) -> Result<Vec<Chapter>> {
    let book = mobi::Mobi::from_path(path).context("读取 MOBI 文件")?;
    let raw = mobi_text(&book);
    let asset_dir = books_cache_dir().join(format!("{}-assets", book_id(path)));
    let _ = std::fs::remove_dir_all(&asset_dir);
    let _ = std::fs::create_dir_all(&asset_dir);
    let images = extract_mobi_images(&book, &asset_dir);
    let text = mobi_html_to_text(&raw, images.len());
    if !has_readable_text(&text) {
        let text = external_text(path, "ebook-convert")
            .context("内置 MOBI 解析未得到正文，且 Calibre 不可用")?;
        return Ok(split_text(
            &text,
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("电子书"),
        ));
    }
    let mut chapters = split_text(
        &text,
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("电子书"),
    );
    attach_mobi_images(&mut chapters, images);
    Ok(chapters)
}

fn attach_mobi_images(chapters: &mut [Chapter], images: Vec<String>) {
    for chapter in chapters {
        let mut local_images = Vec::new();
        for paragraph in &mut chapter.paragraphs {
            let mut search_offset = 0;
            while let Some(relative_position) = paragraph[search_offset..].find("【图片") {
                let position = search_offset + relative_position;
                let Some(end) = paragraph[position..].find('】') else {
                    break;
                };
                let end = position + end + '】'.len_utf8();
                let image_index = paragraph[position + "【图片".len()..end - '】'.len_utf8()]
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| index.checked_sub(1));
                let Some(image) = image_index.and_then(|index| images.get(index)) else {
                    paragraph.replace_range(position..end, "[图片]");
                    search_offset = position + "[图片]".len();
                    continue;
                };
                let local_index = local_images.len() + 1;
                let marker = format!("【图片{local_index}】");
                paragraph.replace_range(position..end, &marker);
                search_offset = position + marker.len();
                local_images.push(image.clone());
            }
        }
        chapter.images = local_images;
    }
}

fn mobi_text(book: &mobi::Mobi) -> String {
    match book.compression() {
        mobi::headers::Compression::PalmDoc => {
            let mut content = Vec::with_capacity(book.metadata.palmdoc.text_length as usize);
            for record in book
                .raw_records()
                .records()
                .iter()
                .skip(1)
                .take(book.metadata.palmdoc.record_count as usize)
            {
                content.extend(decompress_palmdoc(record.content));
            }
            content.truncate(book.metadata.palmdoc.text_length as usize);
            String::from_utf8_lossy(&content).into_owned()
        }
        mobi::headers::Compression::No => {
            let mut content = Vec::with_capacity(book.metadata.palmdoc.text_length as usize);
            for record in book
                .raw_records()
                .records()
                .iter()
                .skip(1)
                .take(book.metadata.palmdoc.record_count as usize)
            {
                content.extend_from_slice(record.content);
            }
            content.truncate(book.metadata.palmdoc.text_length as usize);
            String::from_utf8_lossy(&content).into_owned()
        }
        mobi::headers::Compression::Huff => book.content_as_string_lossy(),
    }
}

fn mobi_html_to_text(raw: &str, image_count: usize) -> String {
    let mut image_index = 0usize;
    let document = Html::parse_fragment(raw);
    let mut text = String::new();
    for edge in document.tree.root().traverse() {
        if let Edge::Open(node) = edge {
            match node.value() {
                Node::Text(value) => text.push_str(value),
                Node::Element(element) => match element.name() {
                    "br" => text.push('\n'),
                    "p" | "div" if !text.is_empty() => text.push_str("\n\n"),
                    "img" => {
                        image_index += 1;
                        if image_index <= image_count {
                            text.push_str(&format!("【图片{image_index}】"));
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
    text.trim().to_string()
}

fn extract_mobi_images(book: &mobi::Mobi, asset_dir: &Path) -> Vec<String> {
    book.image_records()
        .into_iter()
        .enumerate()
        .map(|(index, record)| {
            let extension = image_extension(record.content).unwrap_or("img");
            let path = asset_dir.join(format!("{}.{}", index + 1, extension));
            std::fs::write(&path, record.content)
                .ok()
                .map(|_| path.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect()
}

fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.starts_with(b"BM") {
        Some("bmp")
    } else {
        None
    }
}

fn decompress_palmdoc(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(4096);
    let mut index = 0;
    while index < data.len() {
        let byte = data[index];
        index += 1;
        match byte {
            0 | 9..=127 => output.push(byte),
            1..=8 => {
                let end = (index + byte as usize).min(data.len());
                output.extend_from_slice(&data[index..end]);
                index = end;
            }
            128..=191 => {
                if index >= data.len() {
                    break;
                }
                let pair = u16::from_be_bytes([byte, data[index]]);
                index += 1;
                let distance = ((pair & 0x3fff) >> 3) as usize;
                let length = ((pair & 7) + 3) as usize;
                if distance == 0 || distance > output.len() {
                    continue;
                }
                for _ in 0..length {
                    output.push(output[output.len() - distance]);
                }
            }
            192..=255 => {
                output.push(b' ');
                output.push(byte ^ 0x80);
            }
        }
    }
    output
}

fn has_readable_text(text: &str) -> bool {
    text.chars()
        .filter(|ch| ch.is_alphanumeric())
        .take(20)
        .count()
        == 20
}

fn parse_zip_xml(path: &Path, entry_name: &str) -> Result<Vec<Chapter>> {
    let file = File::open(path).with_context(|| format!("打开文件失败: {}", path.display()))?;
    let mut archive = ZipArchive::new(file).context("读取压缩电子书")?;
    let bytes = read_zip_entry(&mut archive, entry_name)?;
    let text = xml_text(&decode_bytes(&bytes), true);
    Ok(split_text(
        &text,
        path.file_stem().and_then(|s| s.to_str()).unwrap_or("文档"),
    ))
}

struct EpubBook {
    title: String,
    chapters: Vec<Chapter>,
}

#[derive(Default)]
struct OpfData {
    title: String,
    manifest: HashMap<String, ManifestItem>,
    spine: Vec<String>,
    toc_id: Option<String>,
}

struct ManifestItem {
    href: String,
    media_type: String,
    properties: String,
}

struct TocEntry {
    title: String,
    path: String,
}

fn parse_epub(path: &Path) -> Result<EpubBook> {
    let file = File::open(path).with_context(|| format!("打开 EPUB 失败: {}", path.display()))?;
    let mut archive = ZipArchive::new(file).context("读取 EPUB 压缩包")?;
    let container = decode_bytes(&read_zip_entry(&mut archive, "META-INF/container.xml")?);
    let opf_path = first_xml_attribute(&container, "rootfile", "full-path")
        .ok_or_else(|| anyhow!("EPUB 缺少 OPF 目录"))?;
    let opf = decode_bytes(&read_zip_entry(&mut archive, &opf_path)?);
    let package = parse_opf(&opf);
    if package.spine.is_empty() {
        return Err(anyhow!("EPUB 没有可阅读章节"));
    }
    let opf_base = Path::new(&opf_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let asset_dir = books_cache_dir().join(format!("{}-assets", book_id(path)));
    let _ = std::fs::remove_dir_all(&asset_dir);
    let _ = std::fs::create_dir_all(&asset_dir);

    let spine_paths = package
        .spine
        .iter()
        .filter_map(|idref| package.manifest.get(idref))
        .filter(|item| item.media_type.contains("html"))
        .map(|item| zip_path(opf_base, &percent_decode(&item.href)))
        .collect::<Vec<_>>();

    let toc_item = package
        .toc_id
        .as_ref()
        .and_then(|id| package.manifest.get(id))
        .or_else(|| {
            package.manifest.values().find(|item| {
                item.media_type.contains("ncx")
                    || item.properties.split_whitespace().any(|p| p == "nav")
            })
        });
    let toc_entries = toc_item
        .and_then(|item| {
            let toc_path = zip_path(opf_base, &percent_decode(&item.href));
            let raw = decode_bytes(&read_zip_entry(&mut archive, &toc_path).ok()?);
            let base = Path::new(&toc_path)
                .parent()
                .unwrap_or_else(|| Path::new(""));
            Some(if item.media_type.contains("ncx") {
                parse_ncx(&raw, base)
            } else {
                parse_nav_document(&raw, base)
            })
        })
        .unwrap_or_default();

    let mut starts = Vec::<(usize, String)>::new();
    for entry in toc_entries {
        let Some(index) = spine_paths.iter().position(|path| path == &entry.path) else {
            continue;
        };
        if starts.last().is_some_and(|(last, _)| index <= *last) {
            continue;
        }
        starts.push((index, entry.title));
    }

    let chapters = if starts.is_empty() {
        chapters_from_spine(&mut archive, &spine_paths, &asset_dir)
    } else {
        chapters_from_toc(&mut archive, &spine_paths, &starts, &asset_dir)
    };
    if chapters.is_empty() {
        return Err(anyhow!("EPUB 章节内容为空"));
    }
    Ok(EpubBook {
        title: package.title,
        chapters,
    })
}

fn parse_opf(raw: &str) -> OpfData {
    let mut reader = Reader::from_str(raw);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut package = OpfData::default();
    let mut title_open = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                let event_name = event.name();
                let name = local_name(event_name.as_ref());
                if name == "item" {
                    add_manifest(&event, &mut package.manifest);
                }
                if name == "itemref" {
                    add_spine(&event, &mut package.spine);
                }
                if name == "spine" {
                    package.toc_id = attributes(&event).remove("toc");
                }
                title_open = name == "title";
            }
            Ok(Event::Empty(event)) => {
                let event_name = event.name();
                let name = local_name(event_name.as_ref());
                if name == "item" {
                    add_manifest(&event, &mut package.manifest);
                }
                if name == "itemref" {
                    add_spine(&event, &mut package.spine);
                }
            }
            Ok(Event::Text(event)) if title_open => {
                package.title = event
                    .unescape()
                    .map_or_else(|_| String::new(), |text| text.into_owned());
            }
            Ok(Event::End(event)) if local_name(event.name().as_ref()) == "title" => {
                title_open = false
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    package.title = package.title.trim().to_string();
    package
}

fn add_manifest(event: &BytesStart<'_>, manifest: &mut HashMap<String, ManifestItem>) {
    let attrs = attributes(event);
    if let (Some(id), Some(href)) = (attrs.get("id"), attrs.get("href")) {
        manifest.insert(
            id.clone(),
            ManifestItem {
                href: href.clone(),
                media_type: attrs.get("media-type").cloned().unwrap_or_default(),
                properties: attrs.get("properties").cloned().unwrap_or_default(),
            },
        );
    }
}

fn add_spine(event: &BytesStart<'_>, spine: &mut Vec<String>) {
    if let Some(idref) = attributes(event).get("idref") {
        spine.push(idref.clone());
    }
}

fn parse_ncx(raw: &str, base: &Path) -> Vec<TocEntry> {
    let mut reader = Reader::from_str(raw);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut in_label_text = false;
    let mut label = String::new();
    let mut entries = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) if local_name(event.name().as_ref()) == "text" => {
                in_label_text = true;
            }
            Ok(Event::Text(event)) if in_label_text => {
                label = event
                    .unescape()
                    .map_or_else(|_| String::new(), |text| text.into_owned());
            }
            Ok(Event::End(event)) if local_name(event.name().as_ref()) == "text" => {
                in_label_text = false;
            }
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if local_name(event.name().as_ref()) == "content" =>
            {
                if let Some(src) = attributes(&event).get("src") {
                    let title = label.trim().to_string();
                    if !title.is_empty() {
                        entries.push(TocEntry {
                            title,
                            path: zip_path(base, &percent_decode(src)),
                        });
                    }
                }
                label.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    entries
}

fn parse_nav_document(raw: &str, base: &Path) -> Vec<TocEntry> {
    let document = Html::parse_document(raw);
    let Ok(selector) = Selector::parse("nav a") else {
        return Vec::new();
    };
    document
        .select(&selector)
        .filter_map(|link| {
            let href = link.value().attr("href")?;
            let title = link.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                return None;
            }
            Some(TocEntry {
                title,
                path: zip_path(base, &percent_decode(href)),
            })
        })
        .collect()
}

fn chapters_from_toc<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    spine_paths: &[String],
    starts: &[(usize, String)],
    asset_dir: &Path,
) -> Vec<Chapter> {
    starts
        .iter()
        .enumerate()
        .filter_map(|(position, (start, title))| {
            let end = starts
                .get(position + 1)
                .map_or(spine_paths.len(), |(index, _)| *index);
            let (paragraphs, images) =
                read_epub_content(archive, &spine_paths[*start..end], asset_dir);
            (!paragraphs.is_empty()).then(|| Chapter {
                title: title.clone(),
                paragraphs,
                images,
            })
        })
        .collect()
}

fn chapters_from_spine<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    spine_paths: &[String],
    asset_dir: &Path,
) -> Vec<Chapter> {
    let mut chapters = Vec::new();
    for path in spine_paths {
        let Ok(bytes) = read_zip_entry(archive, path) else {
            continue;
        };
        let raw = decode_bytes(&bytes);
        let (text, hrefs) = epub_body_content(&raw, 0);
        let paragraphs = paragraphs_from_text(&text);
        if paragraphs.is_empty() {
            continue;
        }
        let images = extract_epub_images(archive, path, &hrefs, asset_dir, 0);
        let title = html_heading(&raw).unwrap_or_else(|| format!("第 {} 章", chapters.len() + 1));
        chapters.push(Chapter {
            title,
            paragraphs,
            images,
        });
    }
    chapters
}

fn read_epub_content<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    paths: &[String],
    asset_dir: &Path,
) -> (Vec<String>, Vec<String>) {
    let mut paragraphs = Vec::new();
    let mut images = Vec::new();
    for path in paths {
        let Ok(bytes) = read_zip_entry(archive, path) else {
            continue;
        };
        let (text, hrefs) = epub_body_content(&decode_bytes(&bytes), images.len());
        paragraphs.extend(paragraphs_from_text(&text));
        images.extend(extract_epub_images(
            archive,
            path,
            &hrefs,
            asset_dir,
            images.len(),
        ));
    }
    (paragraphs, images)
}

fn epub_body_content(raw: &str, image_offset: usize) -> (String, Vec<String>) {
    let document = Html::parse_document(raw);
    let Ok(selector) = Selector::parse("body") else {
        return (String::new(), Vec::new());
    };
    let Some(body) = document.select(&selector).next() else {
        return (String::new(), Vec::new());
    };
    let fragment = Html::parse_fragment(&body.inner_html());
    let mut text = String::new();
    let mut hrefs = Vec::new();
    for edge in fragment.tree.root().traverse() {
        if let Edge::Open(node) = edge {
            match node.value() {
                Node::Text(value) => text.push_str(value),
                Node::Element(element) => match element.name() {
                    "br" => text.push('\n'),
                    "p" | "div" | "h1" | "h2" | "h3" | "h4" if !text.is_empty() => {
                        text.push_str("\n\n")
                    }
                    "img" | "image" => {
                        let href = ["src", "href", "xlink:href"]
                            .iter()
                            .find_map(|name| element.attr(name));
                        if let Some(href) = href.filter(|href| !href.starts_with("data:")) {
                            hrefs.push(href.to_string());
                            text.push_str(&format!("【图片{}】", image_offset + hrefs.len()));
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
    (text.trim().to_string(), hrefs)
}

fn extract_epub_images<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    document_path: &str,
    hrefs: &[String],
    asset_dir: &Path,
    image_offset: usize,
) -> Vec<String> {
    let base = Path::new(document_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    hrefs
        .iter()
        .enumerate()
        .map(|(index, href)| {
            if href.starts_with("http://") || href.starts_with("https://") {
                return String::new();
            }
            let source = zip_path(base, &percent_decode(href));
            let Ok(bytes) = read_zip_entry(archive, &source) else {
                return String::new();
            };
            let extension = Path::new(&source)
                .extension()
                .and_then(|ext| ext.to_str())
                .filter(|ext| ext.len() <= 5 && ext.chars().all(|ch| ch.is_ascii_alphanumeric()))
                .unwrap_or("img");
            let target = asset_dir.join(format!("{:03}.{extension}", image_offset + index + 1));
            if std::fs::write(&target, bytes).is_ok() {
                target.to_string_lossy().into_owned()
            } else {
                String::new()
            }
        })
        .collect()
}

fn paragraphs_from_text(text: &str) -> Vec<String> {
    text.split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .map(str::to_string)
        .collect()
}

fn attributes(event: &BytesStart<'_>) -> HashMap<String, String> {
    event
        .attributes()
        .flatten()
        .filter_map(|attr| {
            let key = local_name(attr.key.as_ref()).to_string();
            Some((key, attr.unescape_value().ok()?.into_owned()))
        })
        .collect()
}

fn first_xml_attribute(raw: &str, element: &str, attribute: &str) -> Option<String> {
    let mut reader = Reader::from_str(raw);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf).ok()? {
            Event::Start(event) | Event::Empty(event)
                if local_name(event.name().as_ref()) == element =>
            {
                return attributes(&event).remove(attribute);
            }
            Event::Eof => return None,
            _ => {}
        }
        buf.clear();
    }
}

fn read_zip_entry<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Result<Vec<u8>> {
    let mut file = archive
        .by_name(name)
        .with_context(|| format!("压缩包缺少文件: {name}"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).context("读取压缩文件")?;
    Ok(bytes)
}

fn xml_text(raw: &str, paragraphs: bool) -> String {
    let mut reader = Reader::from_str(raw);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut out = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(event)) => {
                let text = event
                    .unescape()
                    .map_or_else(|_| String::new(), |value| value.into_owned());
                if !text.trim().is_empty() {
                    out.push_str(text.trim());
                }
            }
            Ok(Event::End(event))
                if paragraphs
                    && matches!(local_name(event.name().as_ref()), "p" | "para" | "section") =>
            {
                out.push_str("\n\n");
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

fn split_text(text: &str, default_title: &str) -> Vec<Chapter> {
    let normalized = text.replace('\r', "");
    let mut chapters: Vec<Chapter> = Vec::new();
    let mut current_title = String::new();
    let mut current = Vec::new();
    let mut found_heading = false;
    for line in normalized.lines() {
        let line = line.trim();
        if is_heading(line) {
            if !current.is_empty() {
                push_chapter(&mut chapters, &current_title, &current);
            }
            current_title = line.to_string();
            current.clear();
            found_heading = true;
        } else if !line.is_empty() {
            current.push(line.to_string());
        } else if current.last().is_some_and(|line: &String| !line.is_empty()) {
            current.push(String::new());
        }
    }
    if !current.is_empty() {
        push_chapter(&mut chapters, &current_title, &current);
    }
    if !found_heading {
        let compact = normalized
            .split("\n\n")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        if compact.is_empty() {
            return vec![Chapter {
                title: format!("第一章 · {default_title}"),
                paragraphs: vec!["(本书没有可显示的正文)".into()],
                images: Vec::new(),
            }];
        }
        return vec![Chapter {
            title: format!("第一章 · {default_title}"),
            paragraphs: compact,
            images: Vec::new(),
        }];
    }
    chapters
}

fn push_chapter(chapters: &mut Vec<Chapter>, title: &str, lines: &[String]) {
    let paragraphs = lines
        .join("\n")
        .split("\n\n")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if !paragraphs.is_empty() {
        let title = if title.is_empty() {
            format!("第 {} 章", chapters.len() + 1)
        } else {
            title.to_string()
        };
        chapters.push(Chapter {
            title,
            paragraphs,
            images: Vec::new(),
        });
    }
}

fn is_heading(line: &str) -> bool {
    if line.is_empty() || line.chars().count() > 80 {
        return false;
    }
    let chinese = line.starts_with('第')
        && ["章", "节", "回", "卷", "部"]
            .iter()
            .any(|mark| line.contains(mark));
    let western = line.to_ascii_lowercase().starts_with("chapter ")
        || line.to_ascii_lowercase().starts_with("part ");
    chinese || western || line == "序言" || line == "序章" || line == "尾声" || line == "后记"
}

fn read_text(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("读取文件失败: {}", path.display()))?;
    Ok(decode_bytes(&bytes))
}

fn decode_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return UTF_16LE.decode(&bytes[2..]).0.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return UTF_16BE.decode(&bytes[2..]).0.into_owned();
    }
    let (utf8, _, had_errors) = UTF_8.decode(bytes);
    if had_errors {
        GBK.decode(bytes).0.into_owned()
    } else {
        utf8.trim_start_matches('\u{feff}').to_string()
    }
}

fn strip_rtf(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' | '}' => {}
            '\\' => {
                let mut word = String::new();
                while chars.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                    word.push(chars.next().unwrap());
                }
                while chars
                    .peek()
                    .is_some_and(|c| c.is_ascii_digit() || *c == '-')
                {
                    chars.next();
                }
                if word == "par" || word == "line" {
                    out.push('\n');
                }
                if chars.peek() == Some(&' ') {
                    chars.next();
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

fn external_text(path: &Path, command: &str) -> Result<String> {
    let output_path = std::env::temp_dir().join(format!(
        "touch-fish-{}-{}.txt",
        std::process::id(),
        book_id(path)
    ));
    let output = std::process::Command::new(command)
        .arg(path)
        .arg(&output_path)
        .output()
        .with_context(|| format!("执行 {command} 失败"))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&output_path);
        return Err(anyhow!("{command} 无法转换该文件"));
    }
    let result = read_text(&output_path);
    let _ = std::fs::remove_file(&output_path);
    result
}

fn html_heading(raw: &str) -> Option<String> {
    let document = Html::parse_fragment(raw);
    if let Ok(selector) = Selector::parse("h1, h2, h3") {
        if let Some(heading) = document
            .select(&selector)
            .map(|node| node.text().collect::<String>())
            .find(|text| !text.trim().is_empty())
        {
            return Some(heading.trim().to_string());
        }
    }
    let text = crate::platform::html::to_text(raw);
    text.lines()
        .map(str::trim)
        .find(|line| is_heading(line))
        .map(str::to_string)
}

fn local_name(name: &[u8]) -> &str {
    std::str::from_utf8(name)
        .unwrap_or("")
        .rsplit(':')
        .next()
        .unwrap_or("")
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(a * 16 + b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn zip_path(base: &Path, href: &str) -> String {
    let href = href.split(['#', '?']).next().unwrap_or(href);
    let joined = base.join(href);
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir | std::path::Component::RootDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized.to_string_lossy().replace('\\', "/").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn splits_text_into_detected_chapters() {
        let chapters = split_text(
            "第一章 开始\n这是第一段。\n\n第二章 继续\n这是第二段。",
            "书",
        );
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].title, "第一章 开始");
        assert!(chapters[1].paragraphs.join("\n\n").contains("第二段"));
    }

    #[test]
    fn plain_text_without_heading_has_a_default_chapter() {
        let chapters = split_text("第一段\n\n第二段", "书名");
        assert_eq!(chapters[0].title, "第一章 · 书名");
        assert_eq!(chapters[0].paragraphs.len(), 2);
    }

    #[test]
    fn decompresses_palmdoc_back_references() {
        assert_eq!(decompress_palmdoc(b"Hello \x80\x32"), b"Hello Hello");
    }

    #[test]
    fn position_roundtrips_to_character_counts() {
        let book = Book {
            id: "x".into(),
            path: String::new(),
            title: "书".into(),
            format: "txt".into(),
            error: None,
            chapters: vec![Chapter {
                title: "第一章".into(),
                paragraphs: vec!["abc".into(), "defg".into()],
                images: Vec::new(),
            }],
        };
        assert_eq!(book.read_count(0, 1, 2), 7);
        assert_eq!(book.position_for_offset(0, 7), (1, 2));
    }

    #[test]
    fn epub_manifest_and_spine_are_read_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.epub");
        let file = File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("META-INF/container.xml", options).unwrap();
        zip.write_all(br#"<container><rootfiles><rootfile full-path="OPS/content.opf"/></rootfiles></container>"#).unwrap();
        zip.start_file("OPS/content.opf", options).unwrap();
        zip.write_all(r#"<package><metadata><dc:title>测试书</dc:title></metadata><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/><item id="b" href="b.xhtml" media-type="application/xhtml+xml"/><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/></manifest><spine toc="ncx"><itemref idref="b"/><itemref idref="a"/></spine></package>"#.as_bytes()).unwrap();
        zip.start_file("OPS/toc.ncx", options).unwrap();
        zip.write_all(r#"<ncx><navMap><navPoint><navLabel><text>目录第一章</text></navLabel><content src="b.xhtml"/></navPoint><navPoint><navLabel><text>目录第二章</text></navLabel><content src="a.xhtml"/></navPoint></navMap></ncx>"#.as_bytes()).unwrap();
        zip.start_file("OPS/a.xhtml", options).unwrap();
        zip.write_all("<html><body><h1>第二章</h1><p>后面</p></body></html>".as_bytes())
            .unwrap();
        zip.start_file("OPS/b.xhtml", options).unwrap();
        zip.write_all(
            "<html><body><h1>第一章</h1><p>前面<img src=\"pic.jpg\"/></p></body></html>".as_bytes(),
        )
        .unwrap();
        zip.start_file("OPS/pic.jpg", options).unwrap();
        zip.write_all(b"fake image").unwrap();
        zip.finish().unwrap();
        let epub = parse_epub(&path).unwrap();
        assert_eq!(epub.title, "测试书");
        assert_eq!(epub.chapters.len(), 2);
        assert_eq!(epub.chapters[0].title, "目录第一章");
        assert_eq!(epub.chapters[1].title, "目录第二章");
        assert!(epub.chapters[0].paragraphs.join("\n\n").contains("前面"));
        assert!(epub.chapters[0]
            .paragraphs
            .join("\n\n")
            .contains("【图片1】"));
        assert_eq!(epub.chapters[0].images.len(), 1);
        assert!(Path::new(&epub.chapters[0].images[0]).is_file());
        assert!(epub.chapters[1].paragraphs.join("\n\n").contains("后面"));
    }

    #[test]
    fn epub_body_ignores_head_styles() {
        let raw = "<html><head><style>@page { margin: 0 }</style></head><body><h1>正文</h1><p>内容</p></body></html>";
        let text = epub_body_content(raw, 0).0;
        assert!(!text.contains("@page"));
        assert!(text.contains("正文"));
        assert!(text.contains("内容"));
    }

    #[test]
    fn parses_external_epub_when_provided() {
        let Ok(path) = std::env::var("TEST_EPUB") else {
            return;
        };
        let epub = parse_epub(Path::new(&path)).unwrap();
        assert!(!epub.title.is_empty());
        assert!(!epub.chapters.is_empty());
        assert!(epub
            .chapters
            .iter()
            .flat_map(|chapter| &chapter.paragraphs)
            .all(|paragraph| !paragraph.contains("@page")));
        eprintln!(
            "{}: {} chapters, {} images",
            epub.title,
            epub.chapters.len(),
            epub.chapters
                .iter()
                .map(|chapter| chapter.images.len())
                .sum::<usize>()
        );
    }

    #[test]
    fn parses_external_mobi_when_provided() {
        let Ok(path) = std::env::var("TEST_MOBI") else {
            return;
        };
        let chapters = parse_file(Path::new(&path), "mobi").unwrap();
        let characters = chapters.iter().map(Chapter::char_count).sum::<usize>();
        let images = chapters.iter().map(|chapter| chapter.images.len()).sum::<usize>();
        assert!(!chapters.is_empty());
        assert!(characters > 100_000);
        eprintln!(
            "MOBI: {} chapters, {} characters, {} images",
            chapters.len(),
            characters,
            images
        );
    }

    #[test]
    fn progress_is_saved_per_book() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("TOUCH_FISH_CACHE", dir.path());
        save_progress(
            "book-a",
            ReadingPosition {
                chapter: 3,
                offset: 99,
            },
        )
        .unwrap();
        assert_eq!(load_progress("book-a").chapter, 3);
        assert_eq!(load_progress("book-b"), ReadingPosition::default());
    }
}
