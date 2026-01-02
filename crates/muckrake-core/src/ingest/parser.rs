use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Encoding error: {0}")]
    Encoding(String),
    #[error("Parse failed: {0}")]
    ParseFailed(String),
}

pub type ParseResult<T> = Result<T, ParseError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentFormat {
    PlainText,
    Markdown,
    Html,
    Pdf,
    Docx,
    Xlsx,
    Csv,
    Json,
    Xml,
}

impl DocumentFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "txt" => Some(Self::PlainText),
            "md" | "markdown" => Some(Self::Markdown),
            "html" | "htm" => Some(Self::Html),
            "pdf" => Some(Self::Pdf),
            "docx" => Some(Self::Docx),
            "xlsx" => Some(Self::Xlsx),
            "csv" => Some(Self::Csv),
            "json" => Some(Self::Json),
            "xml" => Some(Self::Xml),
            _ => None,
        }
    }

    pub fn from_mime(mime: &str) -> Option<Self> {
        match mime {
            "text/plain" => Some(Self::PlainText),
            "text/markdown" => Some(Self::Markdown),
            "text/html" => Some(Self::Html),
            "application/pdf" => Some(Self::Pdf),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                Some(Self::Docx)
            }
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => {
                Some(Self::Xlsx)
            }
            "text/csv" => Some(Self::Csv),
            "application/json" => Some(Self::Json),
            "application/xml" | "text/xml" => Some(Self::Xml),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub title: Option<String>,
    pub level: u8,
    pub content: String,
    pub start_offset: usize,
    pub end_offset: usize,
}

impl Section {
    #[must_use]
    pub fn new(content: String, start_offset: usize, end_offset: usize) -> Self {
        Self {
            title: None,
            level: 0,
            content,
            start_offset,
            end_offset,
        }
    }

    #[must_use]
    pub fn with_title(mut self, title: String, level: u8) -> Self {
        self.title = Some(title);
        self.level = level;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedDocument {
    pub title: Option<String>,
    pub format: DocumentFormat,
    pub full_text: String,
    pub sections: Vec<Section>,
    pub page_count: Option<u32>,
    pub metadata: serde_json::Value,
}

impl ParsedDocument {
    #[must_use]
    pub fn new(format: DocumentFormat, full_text: String) -> Self {
        Self {
            title: None,
            format,
            full_text,
            sections: Vec::new(),
            page_count: None,
            metadata: serde_json::Value::Null,
        }
    }

    #[must_use]
    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    #[must_use]
    pub fn with_sections(mut self, sections: Vec<Section>) -> Self {
        self.sections = sections;
        self
    }

    #[must_use]
    pub fn with_page_count(mut self, count: u32) -> Self {
        self.page_count = Some(count);
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

#[async_trait::async_trait]
pub trait Parser: Send + Sync {
    fn supported_formats(&self) -> &[DocumentFormat];

    fn can_parse(&self, format: DocumentFormat) -> bool {
        self.supported_formats().contains(&format)
    }

    async fn parse_bytes(&self, data: &[u8], format: DocumentFormat) -> ParseResult<ParsedDocument>;

    async fn parse_file(&self, path: &Path) -> ParseResult<ParsedDocument> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| ParseError::UnsupportedFormat("no extension".into()))?;

        let format = DocumentFormat::from_extension(ext)
            .ok_or_else(|| ParseError::UnsupportedFormat(ext.into()))?;

        if !self.can_parse(format) {
            return Err(ParseError::UnsupportedFormat(format!("{:?}", format)));
        }

        let data = tokio::fs::read(path).await?;
        self.parse_bytes(&data, format).await
    }
}

pub struct PlainTextParser;

impl PlainTextParser {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for PlainTextParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Parser for PlainTextParser {
    fn supported_formats(&self) -> &[DocumentFormat] {
        &[DocumentFormat::PlainText, DocumentFormat::Markdown]
    }

    async fn parse_bytes(&self, data: &[u8], format: DocumentFormat) -> ParseResult<ParsedDocument> {
        let text = String::from_utf8(data.to_vec())
            .map_err(|e| ParseError::Encoding(e.to_string()))?;

        let sections = if format == DocumentFormat::Markdown {
            parse_markdown_sections(&text)
        } else {
            vec![Section::new(text.clone(), 0, text.len())]
        };

        Ok(ParsedDocument::new(format, text).with_sections(sections))
    }
}

fn parse_markdown_sections(text: &str) -> Vec<Section> {
    let mut sections = Vec::new();
    let mut current_start = 0;
    let mut current_title: Option<(String, u8)> = None;
    let mut current_content = String::new();

    for line in text.lines() {
        if let Some(level) = heading_level(line) {
            if !current_content.is_empty() || current_title.is_some() {
                let end = current_start + current_content.len();
                let mut section = Section::new(current_content.clone(), current_start, end);
                if let Some((title, lvl)) = current_title.take() {
                    section = section.with_title(title, lvl);
                }
                sections.push(section);
            }

            let title = line.trim_start_matches('#').trim().to_string();
            current_title = Some((title, level));
            current_content.clear();
            current_start = text.find(line).unwrap_or(current_start);
        } else {
            if !current_content.is_empty() {
                current_content.push('\n');
            }
            current_content.push_str(line);
        }
    }

    if !current_content.is_empty() || current_title.is_some() {
        let end = current_start + current_content.len();
        let mut section = Section::new(current_content, current_start, end);
        if let Some((title, lvl)) = current_title {
            section = section.with_title(title, lvl);
        }
        sections.push(section);
    }

    sections
}

fn heading_level(line: &str) -> Option<u8> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }

    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if level > 0 && level <= 6 && trimmed.chars().nth(level) == Some(' ') {
        Some(level as u8)
    } else {
        None
    }
}

pub struct CompositeParser {
    parsers: Vec<Box<dyn Parser>>,
}

impl CompositeParser {
    #[must_use]
    pub fn new() -> Self {
        Self {
            parsers: Vec::new(),
        }
    }

    pub fn with_parser(mut self, parser: Box<dyn Parser>) -> Self {
        self.parsers.push(parser);
        self
    }

    pub fn add_parser(&mut self, parser: Box<dyn Parser>) {
        self.parsers.push(parser);
    }

    fn find_parser(&self, format: DocumentFormat) -> Option<&dyn Parser> {
        self.parsers.iter().find(|p| p.can_parse(format)).map(|p| p.as_ref())
    }
}

impl Default for CompositeParser {
    fn default() -> Self {
        Self::new().with_parser(Box::new(PlainTextParser::new()))
    }
}

#[async_trait::async_trait]
impl Parser for CompositeParser {
    fn supported_formats(&self) -> &[DocumentFormat] {
        &[
            DocumentFormat::PlainText,
            DocumentFormat::Markdown,
            DocumentFormat::Html,
            DocumentFormat::Pdf,
            DocumentFormat::Docx,
            DocumentFormat::Json,
            DocumentFormat::Xml,
            DocumentFormat::Csv,
            DocumentFormat::Xlsx,
        ]
    }

    fn can_parse(&self, format: DocumentFormat) -> bool {
        self.find_parser(format).is_some()
    }

    async fn parse_bytes(&self, data: &[u8], format: DocumentFormat) -> ParseResult<ParsedDocument> {
        let parser = self
            .find_parser(format)
            .ok_or_else(|| ParseError::UnsupportedFormat(format!("{:?}", format)))?;

        parser.parse_bytes(data, format).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plain_text_parser() {
        let parser = PlainTextParser::new();
        let data = b"Hello, world!";

        let doc = parser.parse_bytes(data, DocumentFormat::PlainText).await.unwrap();

        assert_eq!(doc.full_text, "Hello, world!");
        assert_eq!(doc.sections.len(), 1);
    }

    #[tokio::test]
    async fn test_markdown_sections() {
        let parser = PlainTextParser::new();
        let data = b"# Title\n\nIntro\n\n## Section 1\n\nContent 1\n\n## Section 2\n\nContent 2";

        let doc = parser.parse_bytes(data, DocumentFormat::Markdown).await.unwrap();

        assert!(doc.sections.len() >= 2);
    }

    #[test]
    fn test_format_from_extension() {
        assert_eq!(DocumentFormat::from_extension("pdf"), Some(DocumentFormat::Pdf));
        assert_eq!(DocumentFormat::from_extension("md"), Some(DocumentFormat::Markdown));
        assert_eq!(DocumentFormat::from_extension("unknown"), None);
    }

    #[test]
    fn test_format_from_mime() {
        assert_eq!(
            DocumentFormat::from_mime("application/pdf"),
            Some(DocumentFormat::Pdf)
        );
        assert_eq!(
            DocumentFormat::from_mime("text/markdown"),
            Some(DocumentFormat::Markdown)
        );
    }
}
