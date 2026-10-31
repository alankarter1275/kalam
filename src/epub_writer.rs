use anyhow::Result;
use epub_builder::{EpubBuilder, EpubContent, ReferenceType, ZipLibrary};
use std::fs::File;
use std::io::Cursor;
use std::path::Path;

pub struct WebChapter {
    pub title: String,
    pub html_content: String,
}

pub fn generate_epub(
    out_path: &Path,
    title: &str,
    author: &str,
    cover_bytes: Option<&[u8]>,
    chapters: Vec<WebChapter>,
) -> Result<()> {
    let mut epub = EpubBuilder::new(ZipLibrary::new()?)?;

    epub.metadata("author", author)?;
    epub.metadata("title", title)?;

    let css = b"body { font-family: sans-serif; line-height: 1.5; padding: 1em; } h1 { text-align: center; }";
    epub.stylesheet(Cursor::new(css))?;

    if let Some(cover) = cover_bytes {
        let ext = if cover.starts_with(b"\x89PNG\r\n\x1a\n") {
            "png"
        } else if cover.starts_with(b"GIF8") {
            "gif"
        } else {
            "jpg"
        };
        epub.add_cover_image(format!("cover.{}", ext), Cursor::new(cover), "image/*")?;
    }

    for (i, ch) in chapters.iter().enumerate() {
        let escaped_title = quick_xml::escape::escape(&ch.title).into_owned();
        let xhtml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>{}</title></head>
<body>
<h1>{}</h1>
{}
</body>
</html>"#,
            escaped_title, escaped_title, ch.html_content
        );

        let file_name = format!("chapter_{}.xhtml", i + 1);
        let mut content =
            EpubContent::new(&file_name, Cursor::new(xhtml.into_bytes())).title(&ch.title);

        if i == 0 {
            content = content.reftype(ReferenceType::Text);
        }

        epub.add_content(content)?;
    }

    let mut out_file = File::create(out_path)?;
    epub.generate(&mut out_file)?;

    Ok(())
}
