use super::tree::{DocType, DocumentTree, NodeType, TreeNode};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Failed to read file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Unsupported file type: {0}")]
    UnsupportedType(String),
    #[error("Parse failed: {0}")]
    Other(String),
}

pub trait DocumentParser {
    fn parse(&self, file_path: &str) -> Result<DocumentTree, ParseError>;
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn file_name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

// ── Markdown ──────────────────────────────────────────────────────────────────

pub struct MarkdownParser;

impl DocumentParser for MarkdownParser {
    fn parse(&self, file_path: &str) -> Result<DocumentTree, ParseError> {
        let content = std::fs::read_to_string(file_path)?;
        let file_name = file_name_of(file_path);

        let mut tree = DocumentTree::new(file_name, DocType::Markdown);
        let root_id = tree.root_id.clone();
        let parser = Parser::new(&content);

        let mut current_text = String::new();
        let mut current_parent = root_id.clone();
        let mut in_heading = false;
        let mut heading_level: u32 = 0;
        let mut in_code_block = false;
        let mut code_language = String::new();
        let mut in_paragraph = false;
        let mut in_list_item = false;
        let mut in_image = false;
        let mut image_url = String::new();
        let mut section_stack: Vec<(u32, String)> = vec![(0, root_id.clone())];

        for event in parser {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    if !current_text.trim().is_empty() && in_paragraph {
                        let node = TreeNode::new(NodeType::Paragraph, current_text.trim().to_string());
                        let _ = tree.add_node(&current_parent, node);
                    }
                    current_text.clear();
                    in_heading = true;
                    heading_level = level as u32;
                    in_paragraph = false;
                }
                Event::End(TagEnd::Heading(_)) => {
                    if in_heading {
                        let mut node = TreeNode::new(NodeType::Section, current_text.trim().to_string());
                        node.metadata.insert("heading_level".to_string(), serde_json::json!(heading_level));
                        while section_stack.last().is_some_and(|(lvl, _)| *lvl >= heading_level) {
                            section_stack.pop();
                        }
                        let parent = section_stack
                            .last()
                            .map(|(_, id)| id.clone())
                            .unwrap_or_else(|| root_id.clone());
                        let node_id = node.id.clone();
                        let _ = tree.add_node(&parent, node);
                        section_stack.push((heading_level, node_id.clone()));
                        current_parent = node_id;
                        current_text.clear();
                        in_heading = false;
                    }
                }
                Event::Start(Tag::Paragraph) => { current_text.clear(); in_paragraph = true; }
                Event::End(TagEnd::Paragraph) => {
                    if in_paragraph && !current_text.trim().is_empty() {
                        let mut node = TreeNode::new(NodeType::Paragraph, current_text.trim().to_string());
                        let wc = current_text.split_whitespace().count();
                        node.metadata.insert("word_count".to_string(), serde_json::json!(wc));
                        let _ = tree.add_node(&current_parent, node);
                    }
                    current_text.clear();
                    in_paragraph = false;
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    current_text.clear();
                    in_code_block = true;
                    code_language = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.to_string(),
                        pulldown_cmark::CodeBlockKind::Indented => String::new(),
                    };
                }
                Event::End(TagEnd::CodeBlock) => {
                    if in_code_block {
                        let mut node = TreeNode::new(NodeType::CodeBlock, current_text.clone());
                        if !code_language.is_empty() {
                            node.metadata.insert("language".to_string(), serde_json::json!(code_language));
                        }
                        let _ = tree.add_node(&current_parent, node);
                    }
                    current_text.clear();
                    in_code_block = false;
                    code_language.clear();
                }
                Event::Start(Tag::Item) => { current_text.clear(); in_list_item = true; }
                Event::End(TagEnd::Item) => {
                    if in_list_item && !current_text.trim().is_empty() {
                        let node = TreeNode::new(NodeType::ListItem, current_text.trim().to_string());
                        let _ = tree.add_node(&current_parent, node);
                    }
                    current_text.clear();
                    in_list_item = false;
                }
                Event::Start(Tag::Image { dest_url, title, .. }) => {
                    in_image = true;
                    image_url = dest_url.to_string();
                    let _ = title;
                    current_text.clear();
                }
                Event::End(TagEnd::Image) => {
                    if in_image {
                        let mut node = TreeNode::new(NodeType::Image, current_text.trim().to_string());
                        node.raw_image_path = Some(image_url.clone());
                        node.metadata.insert("url".to_string(), serde_json::json!(image_url));
                        let _ = tree.add_node(&current_parent, node);
                    }
                    current_text.clear();
                    in_image = false;
                    image_url.clear();
                }
                Event::Text(text) => current_text.push_str(&text),
                Event::Code(code) => {
                    current_text.push('`');
                    current_text.push_str(&code);
                    current_text.push('`');
                }
                Event::SoftBreak | Event::HardBreak => current_text.push('\n'),
                _ => {}
            }
        }

        if !current_text.trim().is_empty() {
            let node = TreeNode::new(NodeType::Paragraph, current_text.trim().to_string());
            let _ = tree.add_node(&current_parent, node);
        }

        Ok(tree)
    }
}

// ── Plain text ────────────────────────────────────────────────────────────────

pub struct PlainTextParser;

impl DocumentParser for PlainTextParser {
    fn parse(&self, file_path: &str) -> Result<DocumentTree, ParseError> {
        let content = std::fs::read_to_string(file_path)?;
        let file_name = file_name_of(file_path);
        let mut tree = DocumentTree::new(file_name, DocType::PlainText);
        let root_id = tree.root_id.clone();

        for para in content.split("\n\n") {
            let trimmed = para.trim();
            if !trimmed.is_empty() {
                let node = TreeNode::new(NodeType::Paragraph, trimmed.to_string());
                let _ = tree.add_node(&root_id, node);
            }
        }
        Ok(tree)
    }
}

// ── Code files ────────────────────────────────────────────────────────────────

pub struct CodeParser {
    pub language: String,
}

impl DocumentParser for CodeParser {
    fn parse(&self, file_path: &str) -> Result<DocumentTree, ParseError> {
        let content = std::fs::read_to_string(file_path)?;
        let file_name = file_name_of(file_path);
        let mut tree = DocumentTree::new(file_name, DocType::Code);
        let root_id = tree.root_id.clone();

        let lines: Vec<&str> = content.lines().collect();
        for (idx, chunk) in lines.chunks(60).enumerate() {
            let chunk_text = chunk.join("\n");
            if !chunk_text.trim().is_empty() {
                let wc = chunk_text.split_whitespace().count();
                let mut node = TreeNode::new(NodeType::CodeBlock, chunk_text);
                node.metadata.insert("language".to_string(), serde_json::json!(self.language));
                node.metadata.insert("line_start".to_string(), serde_json::json!(idx * 60 + 1));
                node.metadata.insert("line_end".to_string(), serde_json::json!((idx + 1) * 60));
                node.metadata.insert("word_count".to_string(), serde_json::json!(wc));
                let _ = tree.add_node(&root_id, node);
            }
        }
        Ok(tree)
    }
}

// ── PDF ───────────────────────────────────────────────────────────────────────

pub struct PdfParser;

/// Heuristic: detect if a line looks like a heading.
/// Returns Some(level) if it looks like a heading, None otherwise.
fn detect_heading(line: &str) -> Option<u32> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.len() > 120 {
        return None;
    }
    // Numbered headings: "1.", "1.1", "Chapter 1", "Section 2.3"
    let lower = trimmed.to_lowercase();
    if lower.starts_with("chapter ") {
        return Some(1);
    }
    if lower.starts_with("section ") {
        return Some(2);
    }
    // ALL CAPS short line (likely a heading)
    if trimmed.len() <= 80
        && trimmed.len() >= 3
        && trimmed.chars().all(|c| c.is_uppercase() || c.is_whitespace() || c.is_ascii_punctuation() || c.is_ascii_digit())
        && trimmed.chars().any(|c| c.is_alphabetic())
    {
        return Some(2);
    }
    // Short line (< 60 chars) ending without period — often a heading
    if trimmed.len() <= 60 && !trimmed.ends_with('.') && !trimmed.ends_with(',') && !trimmed.contains("  ") {
        // Must have at least one letter, and first char is uppercase or digit
        let first = trimmed.chars().next().unwrap_or(' ');
        if (first.is_uppercase() || first.is_ascii_digit()) && trimmed.chars().any(|c| c.is_alphabetic()) {
            return Some(3);
        }
    }
    None
}

impl DocumentParser for PdfParser {
    fn parse(&self, file_path: &str) -> Result<DocumentTree, ParseError> {
        let file_name = file_name_of(file_path);
        let bytes = std::fs::read(file_path)?;

        let text = pdf_extract::extract_text_from_mem(&bytes).unwrap_or_default();

        let mut tree = DocumentTree::new(file_name, DocType::Pdf);
        let root_id = tree.root_id.clone();

        // Store total page estimate in root metadata
        // pdf_extract joins pages with form-feed; count them for approximate page count
        let page_count = text.matches('\u{000C}').count().max(1);
        if let Some(root) = tree.nodes.get_mut(&root_id) {
            root.metadata.insert("page_count".to_string(), serde_json::json!(page_count));
        }

        if text.trim().is_empty() {
            let node = TreeNode::new(
                NodeType::Paragraph,
                "This PDF contains scanned images or no extractable text. \
                 Use a vision-capable provider to describe image content."
                    .to_string(),
            );
            let _ = tree.add_node(&root_id, node);
            return Ok(tree);
        }

        // Split by form-feed to get per-page text; fall back to whole doc as page 1
        let pages: Vec<&str> = if text.contains('\u{000C}') {
            text.split('\u{000C}').collect()
        } else {
            vec![&text]
        };

        let mut section_stack: Vec<(u32, String)> = vec![(0, root_id.clone())];
        let mut current_parent = root_id.clone();
        let mut para_index = 0u32;

        for (page_idx, page_text) in pages.iter().enumerate() {
            let page_num = page_idx + 1;

            let paras: Vec<&str> = page_text
                .split("\n\n")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();

            for para in &paras {
                let first_line = para.lines().next().unwrap_or(para).trim();

                // Check if this paragraph starts with a heading-like line
                if let Some(level) = detect_heading(first_line) {
                    let title = {
                        let t = first_line;
                        if t.len() > 80 {
                            let mut end = 80;
                            while end > 0 && !t.is_char_boundary(end) { end -= 1; }
                            format!("{}...", &t[..end])
                        } else {
                            t.to_string()
                        }
                    };

                    // Pop stack to correct nesting level
                    while section_stack.last().is_some_and(|(l, _)| *l >= level) {
                        section_stack.pop();
                    }
                    let parent = section_stack
                        .last()
                        .map(|(_, id)| id.clone())
                        .unwrap_or_else(|| root_id.clone());

                    let mut section = TreeNode::new(NodeType::Section, title);
                    section.metadata.insert("heading_level".to_string(), serde_json::json!(level));
                    section.metadata.insert("page_number".to_string(), serde_json::json!(page_num));
                    let section_id = section.id.clone();
                    let _ = tree.add_node(&parent, section);
                    section_stack.push((level, section_id.clone()));
                    current_parent = section_id;

                    // If the paragraph has more lines beyond the heading, add them as content
                    let rest: String = para.lines().skip(1).collect::<Vec<_>>().join("\n");
                    let rest = rest.trim();
                    if !rest.is_empty() {
                        let mut node = TreeNode::new(NodeType::Paragraph, rest.to_string());
                        node.metadata.insert("page_number".to_string(), serde_json::json!(page_num));
                        let wc = rest.split_whitespace().count();
                        node.metadata.insert("word_count".to_string(), serde_json::json!(wc));
                        let _ = tree.add_node(&current_parent, node);
                    }
                } else {
                    let mut node = TreeNode::new(NodeType::Paragraph, para.to_string());
                    node.metadata.insert("page_number".to_string(), serde_json::json!(page_num));
                    let wc = para.split_whitespace().count();
                    node.metadata.insert("word_count".to_string(), serde_json::json!(wc));
                    node.metadata.insert("para_index".to_string(), serde_json::json!(para_index));
                    let _ = tree.add_node(&current_parent, node);
                }
                para_index += 1;
            }
        }

        Ok(tree)
    }
}

// ── DOCX ──────────────────────────────────────────────────────────────────────

pub struct DocxParser;

impl DocumentParser for DocxParser {
    fn parse(&self, file_path: &str) -> Result<DocumentTree, ParseError> {
        use std::io::Read;

        let file_name = file_name_of(file_path);
        let file = std::fs::File::open(file_path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| ParseError::Other(format!("Failed to open DOCX: {}", e)))?;

        let mut xml_content = String::new();
        {
            let mut entry = archive
                .by_name("word/document.xml")
                .map_err(|e| ParseError::Other(format!("Missing document.xml: {}", e)))?;
            entry.read_to_string(&mut xml_content)?;
        }

        let mut tree = DocumentTree::new(file_name, DocType::Word);
        let root_id = tree.root_id.clone();
        parse_docx_xml(&xml_content, &mut tree, &root_id);
        Ok(tree)
    }
}

fn parse_docx_xml(xml: &str, tree: &mut DocumentTree, root_id: &str) {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut current_text = String::new();
    let mut para_style = String::new();
    let mut in_para = false;
    let mut in_table = false;
    let mut table_id: Option<String> = None;
    let mut row_cells: Vec<String> = Vec::new();
    let mut cell_text = String::new();
    let mut current_parent = root_id.to_string();
    let mut section_stack: Vec<(u32, String)> = vec![(0, root_id.to_string())];

    while let Ok(event) = reader.read_event_into(&mut buf) {
        match event {
            Event::Start(ref e) => match e.local_name().as_ref() {
                b"p" if !in_table => {
                    in_para = true;
                    current_text.clear();
                    para_style.clear();
                }
                b"tbl" => {
                    in_table = true;
                    let node = TreeNode::new(NodeType::Table, "Table".to_string());
                    let id = node.id.clone();
                    let _ = tree.add_node(&current_parent, node);
                    table_id = Some(id);
                }
                b"tr" => { row_cells.clear(); }
                b"tc" => { cell_text.clear(); }
                _ => {}
            },

            Event::Empty(ref e) => {
                if e.local_name().as_ref() == b"pStyle" {
                    for attr in e.attributes().flatten() {
                        let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                        if key == "w:val" || key == "val" {
                            let val = std::str::from_utf8(&attr.value).unwrap_or("").to_string();
                            if val.to_lowercase().starts_with("heading") {
                                para_style = val;
                            }
                        }
                    }
                }
            }

            Event::End(ref e) => match e.local_name().as_ref() {
                b"p" if !in_table => {
                    let trimmed = current_text.trim().to_string();
                    if !trimmed.is_empty() {
                        if para_style.to_lowercase().starts_with("heading") {
                            let level = para_style
                                .chars()
                                .rev()
                                .find(|c| c.is_ascii_digit())
                                .and_then(|c| c.to_digit(10))
                                .unwrap_or(1);
                            let mut node = TreeNode::new(NodeType::Section, trimmed);
                            node.metadata.insert("heading_level".to_string(), serde_json::json!(level));
                            while section_stack.last().is_some_and(|(l, _)| *l >= level) {
                                section_stack.pop();
                            }
                            let parent = section_stack
                                .last()
                                .map(|(_, id)| id.clone())
                                .unwrap_or_else(|| root_id.to_string());
                            let node_id = node.id.clone();
                            let _ = tree.add_node(&parent, node);
                            section_stack.push((level, node_id.clone()));
                            current_parent = node_id;
                        } else {
                            let node = TreeNode::new(NodeType::Paragraph, trimmed);
                            let _ = tree.add_node(&current_parent, node);
                        }
                    }
                    in_para = false;
                    para_style.clear();
                    current_text.clear();
                }
                b"tc" => {
                    row_cells.push(cell_text.trim().to_string());
                    cell_text.clear();
                }
                b"tr" => {
                    if !row_cells.is_empty() {
                        if let Some(ref tid) = table_id.clone() {
                            let row_node = TreeNode::new(NodeType::TableRow, row_cells.join(" | "));
                            let row_id = row_node.id.clone();
                            let _ = tree.add_node(tid, row_node);
                            for val in &row_cells {
                                if !val.is_empty() {
                                    let cell = TreeNode::new(NodeType::TableCell, val.clone());
                                    let _ = tree.add_node(&row_id, cell);
                                }
                            }
                        }
                    }
                    row_cells.clear();
                }
                b"tbl" => { in_table = false; table_id = None; }
                _ => {}
            },

            Event::Text(ref e) => {
                let text = e.unescape().unwrap_or_default();
                if in_table {
                    cell_text.push_str(&text);
                } else if in_para {
                    current_text.push_str(&text);
                }
            }

            Event::Eof => break,
            _ => {}
        }

        buf.clear();
    }
}

// ── CSV ───────────────────────────────────────────────────────────────────────

pub struct CsvParser;

impl DocumentParser for CsvParser {
    fn parse(&self, file_path: &str) -> Result<DocumentTree, ParseError> {
        let file_name = file_name_of(file_path);
        let mut rdr = csv::Reader::from_path(file_path)
            .map_err(|e| ParseError::Other(format!("CSV error: {}", e)))?;

        let headers: Vec<String> = rdr
            .headers()
            .map_err(|e| ParseError::Other(format!("CSV headers: {}", e)))?
            .iter()
            .map(str::to_string)
            .collect();

        let mut rows: Vec<Vec<String>> = Vec::new();
        for result in rdr.records() {
            let record = result.map_err(|e| ParseError::Other(format!("CSV record: {}", e)))?;
            rows.push(record.iter().map(str::to_string).collect());
            if rows.len() >= 500 {
                break;
            }
        }

        let summary = format!("{} rows × {} columns", rows.len(), headers.len());
        let mut tree = DocumentTree::new(file_name.clone(), DocType::Csv);
        let root_id = tree.root_id.clone();

        if let Some(root) = tree.nodes.get_mut(&root_id) {
            root.content = format!("{} — {}", file_name, summary);
        }

        let table = TreeNode::new(NodeType::Table, summary);
        let table_id = table.id.clone();
        let _ = tree.add_node(&root_id, table);

        // Header row
        let hrow = TreeNode::new(NodeType::TableRow, headers.join(" | "));
        let hrow_id = hrow.id.clone();
        let _ = tree.add_node(&table_id, hrow);
        for h in &headers {
            let _ = tree.add_node(&hrow_id, TreeNode::new(NodeType::TableCell, h.clone()));
        }

        // Data rows
        for row in &rows {
            let rnode = TreeNode::new(NodeType::TableRow, row.join(" | "));
            let rid = rnode.id.clone();
            let _ = tree.add_node(&table_id, rnode);
            for val in row {
                let _ = tree.add_node(&rid, TreeNode::new(NodeType::TableCell, val.clone()));
            }
        }

        Ok(tree)
    }
}

// ── XLSX / XLS / ODS ─────────────────────────────────────────────────────────

pub struct XlsxParser;

impl DocumentParser for XlsxParser {
    fn parse(&self, file_path: &str) -> Result<DocumentTree, ParseError> {
        use calamine::{open_workbook_auto, Data, Reader};

        let file_name = file_name_of(file_path);
        let mut workbook = open_workbook_auto(file_path)
            .map_err(|e| ParseError::Other(format!("Excel error: {}", e)))?;

        let mut tree = DocumentTree::new(file_name, DocType::Spreadsheet);
        let root_id = tree.root_id.clone();

        let sheet_names = workbook.sheet_names().to_vec();
        for sheet_name in sheet_names {
            let range = match workbook.worksheet_range(&sheet_name) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let (row_count, col_count) = range.get_size();
            let label = format!("{} ({} rows × {} cols)", sheet_name, row_count, col_count);

            let section = TreeNode::new(NodeType::Section, label.clone());
            let section_id = section.id.clone();
            let _ = tree.add_node(&root_id, section);

            let table = TreeNode::new(NodeType::Table, label);
            let table_id = table.id.clone();
            let _ = tree.add_node(&section_id, table);

            for (idx, row) in range.rows().enumerate() {
                if idx >= 500 { break; }

                let cells: Vec<String> = row.iter().map(|c| match c {
                    Data::Empty => String::new(),
                    Data::String(s) => s.clone(),
                    Data::Float(f) => f.to_string(),
                    Data::Int(i) => i.to_string(),
                    Data::Bool(b) => b.to_string(),
                    Data::DateTime(dt) => dt.to_string(),
                    Data::DateTimeIso(s) => s.clone(),
                    Data::DurationIso(s) => s.clone(),
                    Data::Error(e) => format!("#ERR:{:?}", e),
                }).collect();

                let rnode = TreeNode::new(NodeType::TableRow, cells.join(" | "));
                let rid = rnode.id.clone();
                let _ = tree.add_node(&table_id, rnode);
                for val in &cells {
                    if !val.is_empty() {
                        let _ = tree.add_node(&rid, TreeNode::new(NodeType::TableCell, val.clone()));
                    }
                }
            }
        }

        Ok(tree)
    }
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

pub fn get_parser_for_file(path: &str) -> Box<dyn DocumentParser> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "md" | "markdown" => Box::new(MarkdownParser),
        "txt" | "text" | "log" => Box::new(PlainTextParser),
        "pdf" => Box::new(PdfParser),
        "docx" => Box::new(DocxParser),
        "csv" => Box::new(CsvParser),
        "xlsx" | "xls" | "ods" => Box::new(XlsxParser),
        "rs" => Box::new(CodeParser { language: "rust".into() }),
        "py" => Box::new(CodeParser { language: "python".into() }),
        "js" | "mjs" | "cjs" => Box::new(CodeParser { language: "javascript".into() }),
        "ts" | "mts" => Box::new(CodeParser { language: "typescript".into() }),
        "jsx" => Box::new(CodeParser { language: "jsx".into() }),
        "tsx" => Box::new(CodeParser { language: "tsx".into() }),
        "go" => Box::new(CodeParser { language: "go".into() }),
        "java" => Box::new(CodeParser { language: "java".into() }),
        "c" | "h" => Box::new(CodeParser { language: "c".into() }),
        "cpp" | "cc" | "cxx" | "hpp" => Box::new(CodeParser { language: "cpp".into() }),
        "cs" => Box::new(CodeParser { language: "csharp".into() }),
        "rb" => Box::new(CodeParser { language: "ruby".into() }),
        "php" => Box::new(CodeParser { language: "php".into() }),
        "swift" => Box::new(CodeParser { language: "swift".into() }),
        "kt" | "kts" => Box::new(CodeParser { language: "kotlin".into() }),
        "sql" => Box::new(CodeParser { language: "sql".into() }),
        "sh" | "bash" | "zsh" => Box::new(CodeParser { language: "shell".into() }),
        "toml" => Box::new(CodeParser { language: "toml".into() }),
        "yaml" | "yml" => Box::new(CodeParser { language: "yaml".into() }),
        "json" => Box::new(CodeParser { language: "json".into() }),
        "xml" => Box::new(CodeParser { language: "xml".into() }),
        "html" | "htm" => Box::new(CodeParser { language: "html".into() }),
        "css" | "scss" | "sass" | "less" => Box::new(CodeParser { language: "css".into() }),
        _ => Box::new(PlainTextParser),
    }
}
