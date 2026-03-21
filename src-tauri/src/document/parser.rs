use super::tree::{DocType, DocumentTree, NodeType, TreeNode};
use crate::llm::local;
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

/// Common document section keywords that are almost always headings.
/// Matched case-insensitively against standalone lines.
const SECTION_KEYWORDS: &[&str] = &[
    // Academic / report
    "abstract", "introduction", "background", "methodology", "methods",
    "results", "discussion", "conclusion", "conclusions", "references",
    "bibliography", "acknowledgements", "acknowledgments", "appendix",
    "literature review", "related work", "future work",
    // CV / resume
    "education", "experience", "work experience", "professional experience",
    "skills", "technical skills", "projects", "certifications", "certificates",
    "achievements", "awards", "publications", "languages", "interests",
    "hobbies", "objective", "summary", "profile", "contact", "contact information",
    "personal information", "personal details", "qualifications",
    // Legal / business
    "overview", "scope", "definitions", "terms and conditions",
    "responsibilities", "requirements", "deliverables", "timeline",
    "budget", "risk assessment", "recommendations", "executive summary",
    // General
    "table of contents", "glossary", "index", "preface", "foreword",
];

/// Heuristic: detect if a line looks like a heading.
/// Returns Some(level) if it looks like a heading, None otherwise.
fn detect_heading(line: &str) -> Option<u32> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.len() > 120 {
        return None;
    }
    // Reject lines that are clearly sentences (end with period, comma, etc.)
    if trimmed.ends_with('.') || trimmed.ends_with(',') || trimmed.ends_with(';') {
        // Exception: abbreviations like "Ph.D." or "U.S." in short lines
        if trimmed.len() > 40 {
            return None;
        }
    }

    let lower = trimmed.to_lowercase();

    // 1. Exact keyword match — strongest signal
    if SECTION_KEYWORDS.contains(&lower.as_str()) {
        return Some(2);
    }
    // Also match "keyword:" variants (e.g., "Skills:" "Education:")
    let lower_no_colon = lower.trim_end_matches(':').trim();
    if SECTION_KEYWORDS.contains(&lower_no_colon) {
        return Some(2);
    }

    // 2. Explicit numbered heading patterns: "Chapter 1", "Part II", "Section 2.3"
    if lower.starts_with("chapter ") || lower.starts_with("part ") {
        return Some(1);
    }
    if lower.starts_with("section ") || lower.starts_with("appendix ") {
        return Some(2);
    }

    // 3. Numbered headings: "1. Introduction", "2.3 Methods", "1.2.1 Overview"
    let re_numbered = trimmed.split_whitespace().next().unwrap_or("");
    if !re_numbered.is_empty() {
        let stripped = re_numbered.trim_end_matches('.');
        let is_numbered = stripped.split('.').all(|s| s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty());
        if is_numbered && trimmed.len() <= 80 {
            let depth = stripped.matches('.').count();
            return Some((depth as u32 + 1).min(4));
        }
    }

    // 4. ALL CAPS short line (likely a heading)
    if trimmed.len() <= 80
        && trimmed.len() >= 3
        && trimmed.chars().all(|c| c.is_uppercase() || c.is_whitespace() || c.is_ascii_punctuation() || c.is_ascii_digit())
        && trimmed.chars().any(|c| c.is_alphabetic())
    {
        return Some(2);
    }

    // 5. Short title-case line (< 60 chars) without sentence endings
    if trimmed.len() <= 60
        && !trimmed.ends_with('.')
        && !trimmed.ends_with(',')
        && !trimmed.ends_with(';')
    {
        let first = trimmed.chars().next().unwrap_or(' ');
        if (first.is_uppercase() || first.is_ascii_digit())
            && trimmed.chars().any(|c| c.is_alphabetic())
            // Must not look like a regular sentence (has few words)
            && trimmed.split_whitespace().count() <= 5
        {
            return Some(3);
        }
    }
    None
}

/// SLM-assisted heading classification for ambiguous lines.
/// Only called when heuristic `detect_heading` returned None but the line
/// looks like it *could* be a heading (short, no sentence endings).
/// Returns Some(level) if the SLM classifies it as a heading, None otherwise.
fn slm_classify_heading(line: &str, context: &str) -> Option<u32> {
    if !local::is_engine_loaded() {
        return None;
    }
    let trimmed = line.trim();
    // Only attempt for lines that are plausibly headings:
    // - Not too long (≤80 chars)
    // - Not ending with sentence punctuation
    // - Has at least 2 characters
    if trimmed.len() > 80 || trimmed.len() < 2 {
        return None;
    }
    if trimmed.ends_with('.') || trimmed.ends_with(',') || trimmed.ends_with(';') {
        return None;
    }

    let system = "Classify the given line as heading or body text. Reply ONLY with: heading:1 OR heading:2 OR heading:3 OR body";
    let user = format!(
        "Line: \"{}\"\nContext: \"{}\"",
        trimmed,
        &context[..context.len().min(200)]
    );

    let result = match local::chat_inference(system, &user, 10) {
        Ok(r) => r,
        Err(_) => return None,
    };

    let lower = result.to_lowercase();
    if lower.contains("heading:1") || lower.contains("heading: 1") {
        Some(1)
    } else if lower.contains("heading:2") || lower.contains("heading: 2") {
        Some(2)
    } else if lower.contains("heading:3") || lower.contains("heading: 3") {
        Some(3)
    } else if lower.starts_with("heading") {
        Some(3) // generic heading without level → subsection
    } else {
        None
    }
}

/// Detect whether a line contains fused section keywords (no space between keyword and
/// the following content) and split it into separate lines.
/// Handles BOTH start-of-line fusions and mid-line fusions, recursively.
/// e.g. "EducationLovely Professional University…" → ["Education", "Lovely Professional University…"]
/// e.g. "some textSkillsPython, Rust" → ["some text", "Skills", "Python, Rust"]
fn split_fused_heading(line: &str) -> Vec<String> {
    if line.is_empty() {
        return vec![line.to_string()];
    }
    let lower = line.to_lowercase();

    // Find the earliest fused keyword match anywhere in the line.
    // A "fused" match means: keyword appears case-insensitively, the char before it
    // (if any) is NOT a space, OR the char after it is uppercase/digit with no space.
    let mut best_match: Option<(usize, usize)> = None; // (byte_offset, keyword_len)

    for kw in SECTION_KEYWORDS {
        if kw.len() < 3 {
            continue;
        }
        // Search for all occurrences of this keyword in the line
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(kw) {
            let abs_pos = search_from + pos;
            let kw_end = abs_pos + kw.len();

            if kw_end >= line.len() {
                // Keyword at end of line with nothing after — check if fused from left
                if abs_pos > 0 {
                    let prev_char = line[..abs_pos].chars().last().unwrap_or(' ');
                    if prev_char != ' ' && prev_char != '\n'
                        && (best_match.is_none() || abs_pos < best_match.unwrap().0) {
                            best_match = Some((abs_pos, kw.len()));
                    }
                }
                break;
            }

            let next_char = line[kw_end..].chars().next().unwrap_or(' ');
            let is_fused_right = next_char != ' ' && (next_char.is_uppercase() || next_char.is_ascii_digit());
            let is_fused_left = abs_pos > 0 && {
                let prev_char = line[..abs_pos].chars().last().unwrap_or(' ');
                prev_char != ' ' && prev_char != '\n'
            };

            // It's a fused keyword if content runs into it from either side
            if (is_fused_right || is_fused_left)
                && (best_match.is_none() || abs_pos < best_match.unwrap().0) {
                    best_match = Some((abs_pos, kw.len()));
            }

            search_from = abs_pos + 1;
        }
    }

    match best_match {
        None => vec![line.to_string()],
        Some((pos, kw_len)) => {
            let mut parts = Vec::new();
            // Text before the keyword
            let before = line[..pos].trim();
            if !before.is_empty() {
                parts.push(before.to_string());
            }
            // The keyword itself
            let keyword = &line[pos..pos + kw_len];
            parts.push(keyword.to_string());
            // Recursively split the rest (may contain more fused keywords)
            let rest = line[pos + kw_len..].trim();
            if !rest.is_empty() {
                parts.extend(split_fused_heading(rest));
            }
            parts
        }
    }
}

/// Detect when a line starts with a known section keyword followed by content.
/// PDF extractors often merge heading text with the first line of body text on the same line.
/// e.g. "Education Lovely Professional University..." → ["Education", "Lovely Professional University..."]
/// Only splits when the remaining content is substantial (>15 chars) and starts with an
/// uppercase letter or digit, to avoid false positives like "Skills Overview" or "Summary of".
fn split_leading_keyword(line: &str) -> Vec<String> {
    if line.len() < 15 {
        return vec![line.to_string()];
    }
    let lower = line.to_lowercase();

    // Sort keywords longest-first to prefer "work experience" over "experience"
    let mut sorted_kws: Vec<&str> = SECTION_KEYWORDS.to_vec();
    sorted_kws.sort_by_key(|b| std::cmp::Reverse(b.len()));

    for kw in &sorted_kws {
        if !lower.starts_with(kw) {
            continue;
        }
        let kw_end = kw.len();
        if kw_end >= line.len() {
            continue; // keyword IS the whole line — leave it for detect_heading
        }

        let next_char = line[kw_end..].chars().next().unwrap_or(' ');

        // Keyword followed by space or colon
        if next_char == ' ' || next_char == ':' {
            let skip = if next_char == ':' {
                // "Skills: Python..." → skip colon + any whitespace
                1 + line[kw_end + 1..].len() - line[kw_end + 1..].trim_start().len()
            } else {
                1 // skip the space
            };
            if kw_end + skip >= line.len() {
                continue;
            }
            let rest = line[kw_end + skip..].trim();

            // Only split if rest is substantial and starts with uppercase/digit
            if rest.len() > 15
                && rest
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_uppercase() || c.is_ascii_digit())
            {
                return vec![line[..kw_end].to_string(), rest.to_string()];
            }
        }
    }

    vec![line.to_string()]
}

/// Flush accumulated paragraph text into the tree as a Paragraph node.
fn flush_paragraph(
    tree: &mut DocumentTree,
    parent_id: &str,
    buffer: &mut String,
    page_num: usize,
) {
    let trimmed = buffer.trim();
    if !trimmed.is_empty() {
        let mut node = TreeNode::new(NodeType::Paragraph, trimmed.to_string());
        node.metadata.insert("page_number".to_string(), serde_json::json!(page_num));
        let wc = trimmed.split_whitespace().count();
        node.metadata.insert("word_count".to_string(), serde_json::json!(wc));
        let _ = tree.add_node(parent_id, node);
    }
    buffer.clear();
}

impl DocumentParser for PdfParser {
    fn parse(&self, file_path: &str) -> Result<DocumentTree, ParseError> {
        // Try LiteParse first if available — better layout-aware extraction.
        // This is safe to call during ingest because metadata enrichment (the
        // slow part) now runs in the background, not here.
        if super::liteparse::is_available() {
            if let Ok(tree) = self.parse_with_liteparse(file_path) {
                return Ok(tree);
            }
        }

        self.parse_with_pdf_extract(file_path)
    }
}

impl PdfParser {
    /// Parse using LiteParse (external npx tool) for better layout-aware extraction.
    /// Not called automatically — available via explicit "Re-parse with LiteParse" action.
    pub fn parse_with_liteparse(&self, file_path: &str) -> Result<DocumentTree, ParseError> {
        let json = super::liteparse::parse_pdf(file_path)
            .map_err(|e| ParseError::Other(e))?;
        let blocks = super::liteparse::extract_text_blocks(&json);
        if blocks.is_empty() {
            return Err(ParseError::Other("LiteParse returned no content".to_string()));
        }

        let file_name = file_name_of(file_path);
        let mut tree = DocumentTree::new(file_name, DocType::Pdf);
        let root_id = tree.root_id.clone();

        if let Some(root) = tree.nodes.get_mut(&root_id) {
            root.metadata.insert("page_count".to_string(), serde_json::json!(blocks.len()));
            root.metadata.insert("parse_source".to_string(), serde_json::json!("liteparse"));
        }

        // Process each page's text through the same heading detection pipeline.
        let mut section_stack: Vec<(u32, String)> = Vec::new();
        let mut current_parent = root_id.clone();
        let mut para_buffer = String::new();

        for (text, page_num) in &blocks {
            let lines: Vec<&str> = text.lines().collect();
            for raw_line in lines {
                let trimmed = raw_line.trim();
                if trimmed.is_empty() {
                    flush_paragraph(&mut tree, &current_parent, &mut para_buffer, *page_num);
                    continue;
                }

                if let Some(level) = detect_heading(trimmed) {
                    flush_paragraph(&mut tree, &current_parent, &mut para_buffer, *page_num);
                    let title = trimmed[..trimmed.len().min(80)].to_string();

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
                } else if let Some(level) = slm_classify_heading(trimmed, &para_buffer) {
                    flush_paragraph(&mut tree, &current_parent, &mut para_buffer, *page_num);
                    let title = trimmed[..trimmed.len().min(80)].to_string();

                    while section_stack.last().is_some_and(|(l, _)| *l >= level) {
                        section_stack.pop();
                    }
                    let parent = section_stack
                        .last()
                        .map(|(_, id)| id.clone())
                        .unwrap_or_else(|| root_id.clone());

                    let mut section = TreeNode::new(NodeType::Section, title);
                    section.metadata.insert("heading_level".to_string(), serde_json::json!(level));
                    section.metadata.insert("heading_source".to_string(), serde_json::json!("slm"));
                    section.metadata.insert("page_number".to_string(), serde_json::json!(page_num));
                    let section_id = section.id.clone();
                    let _ = tree.add_node(&parent, section);
                    section_stack.push((level, section_id.clone()));
                    current_parent = section_id;
                } else {
                    if !para_buffer.is_empty() {
                        para_buffer.push('\n');
                    }
                    para_buffer.push_str(trimmed);
                }
            }
        }

        flush_paragraph(&mut tree, &current_parent, &mut para_buffer, blocks.len());
        Ok(tree)
    }

    /// Parse using the built-in pdf-extract crate (default/fallback).
    fn parse_with_pdf_extract(&self, file_path: &str) -> Result<DocumentTree, ParseError> {
        let file_name = file_name_of(file_path);
        let bytes = std::fs::read(file_path)?;

        let text = pdf_extract::extract_text_from_mem(&bytes).unwrap_or_default();

        let mut tree = DocumentTree::new(file_name, DocType::Pdf);
        let root_id = tree.root_id.clone();

        // Store total page estimate in root metadata
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

        let use_page_sections = pages.len() > 1;

        // section_stack tracks heading nesting: (level, node_id)
        let mut section_stack: Vec<(u32, String)> = vec![(0, root_id.clone())];
        let mut current_parent = root_id.clone();
        let mut para_buffer = String::new();

        for (page_idx, page_text) in pages.iter().enumerate() {
            let page_num = page_idx + 1;

            // Create a page-level section node for multi-page PDFs
            if use_page_sections {
                // Flush any leftover paragraph from previous page
                flush_paragraph(&mut tree, &current_parent, &mut para_buffer, page_num.saturating_sub(1));

                let mut page_node = TreeNode::new(
                    NodeType::Section,
                    format!("Page {}", page_num),
                );
                page_node.metadata.insert("page_number".to_string(), serde_json::json!(page_num));
                page_node.metadata.insert("is_page_section".to_string(), serde_json::json!(true));
                let page_node_id = page_node.id.clone();
                let _ = tree.add_node(&root_id, page_node);
                section_stack = vec![(0, page_node_id.clone())];
                current_parent = page_node_id;
            }

            // Process line-by-line instead of splitting on \n\n.
            // This catches headings that pdf-extract separates with only single newlines.
            for raw_line in page_text.lines() {
                // Two-pass split for PDF artefacts:
                // 1. split_fused_heading: "EducationLovely Prof..." → ["Education", "Lovely Prof..."]
                // 2. split_leading_keyword: "Education Lovely Prof..." → ["Education", "Lovely Prof..."]
                let fused_parts = split_fused_heading(raw_line.trim());
                let mut all_parts: Vec<String> = Vec::new();
                for part in &fused_parts {
                    all_parts.extend(split_leading_keyword(part.trim()));
                }
                for split_line in &all_parts {
                let trimmed = split_line.trim();

                // Blank line: flush accumulated paragraph
                if trimmed.is_empty() {
                    flush_paragraph(&mut tree, &current_parent, &mut para_buffer, page_num);
                    continue;
                }

                // Check if this line is a heading
                if let Some(level) = detect_heading(trimmed) {
                    // Flush any accumulated paragraph content before the heading
                    flush_paragraph(&mut tree, &current_parent, &mut para_buffer, page_num);

                    let title = if trimmed.len() > 80 {
                        let mut end = 80;
                        while end > 0 && !trimmed.is_char_boundary(end) { end -= 1; }
                        format!("{}...", &trimmed[..end])
                    } else {
                        trimmed.to_string()
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
                } else if let Some(level) = slm_classify_heading(trimmed, &para_buffer) {
                    // SLM classified an ambiguous line as a heading
                    flush_paragraph(&mut tree, &current_parent, &mut para_buffer, page_num);

                    let title = if trimmed.len() > 80 {
                        let mut end = 80;
                        while end > 0 && !trimmed.is_char_boundary(end) { end -= 1; }
                        format!("{}...", &trimmed[..end])
                    } else {
                        trimmed.to_string()
                    };

                    while section_stack.last().is_some_and(|(l, _)| *l >= level) {
                        section_stack.pop();
                    }
                    let parent = section_stack
                        .last()
                        .map(|(_, id)| id.clone())
                        .unwrap_or_else(|| root_id.clone());

                    let mut section = TreeNode::new(NodeType::Section, title);
                    section.metadata.insert("heading_level".to_string(), serde_json::json!(level));
                    section.metadata.insert("heading_source".to_string(), serde_json::json!("slm"));
                    section.metadata.insert("page_number".to_string(), serde_json::json!(page_num));
                    let section_id = section.id.clone();
                    let _ = tree.add_node(&parent, section);
                    section_stack.push((level, section_id.clone()));
                    current_parent = section_id;
                } else {
                    // Accumulate into current paragraph
                    if !para_buffer.is_empty() {
                        para_buffer.push('\n');
                    }
                    para_buffer.push_str(trimmed);
                }
            } // end split_line
        } // end split_lines
        }

        // Flush any remaining paragraph
        flush_paragraph(&mut tree, &current_parent, &mut para_buffer, pages.len());

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

        // Extract embedded images and add as Image nodes
        let images = super::image::extract_images_from_docx(file_path, &tree.id);
        for img in images {
            let mut node = TreeNode::new(NodeType::Image, img.path.clone());
            node.raw_image_path = Some(img.path);
            node.metadata.insert("mime_type".to_string(), serde_json::json!(img.mime_type));
            let _ = tree.add_node(&root_id, node);
        }

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
                            let _ = tree.add_node(tid, row_node);
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

        let total_rows = rows.len();
        let col_count = headers.len();
        let summary = format!("{} rows × {} columns", total_rows, col_count);
        let mut tree = DocumentTree::new(file_name.clone(), DocType::Csv);
        let root_id = tree.root_id.clone();

        if let Some(root) = tree.nodes.get_mut(&root_id) {
            root.content = format!("{} — {}", file_name, summary);
        }

        // Build a schema description: column names + sample values from first 3 rows
        let schema_lines: Vec<String> = headers.iter().enumerate().map(|(i, col)| {
            let samples: Vec<&str> = rows.iter().take(3)
                .filter_map(|r| r.get(i).map(|s| s.as_str()))
                .filter(|s| !s.is_empty())
                .collect();
            if samples.is_empty() {
                col.clone()
            } else {
                format!("{} (e.g. {})", col, samples.join(", "))
            }
        }).collect();
        let schema = format!(
            "Columns: {}\nSchema:\n{}",
            headers.join(", "),
            schema_lines.join("\n")
        );

        let mut table = TreeNode::new(NodeType::Table, summary.clone());
        // Store columns and schema on the table node for retrieval
        table.metadata.insert("columns".to_string(), serde_json::json!(headers));
        table.metadata.insert("total_rows".to_string(), serde_json::json!(total_rows));
        table.metadata.insert("schema".to_string(), serde_json::json!(schema.clone()));
        // Use schema as the table's summary so fetch_summarize uses it instead of raw rows
        table.summary = Some(format!("{}\n{}", summary, schema));
        let table_id = table.id.clone();
        let _ = tree.add_node(&root_id, table);

        // Header row
        let mut hrow = TreeNode::new(NodeType::TableRow, headers.join(" | "));
        hrow.metadata.insert("is_header".to_string(), serde_json::json!(true));
        let _ = tree.add_node(&table_id, hrow);

        // Data rows — store as "ColName: value | ColName: value" so the LLM always
        // knows which column each value belongs to, even when a single row is fetched.
        for (idx, row) in rows.iter().enumerate() {
            let keyed: Vec<String> = headers.iter().zip(row.iter())
                .filter(|(_, v)| !v.is_empty())
                .map(|(col, val)| format!("{}: {}", col, val))
                .collect();
            let content = if keyed.is_empty() {
                row.join(" | ")
            } else {
                keyed.join(" | ")
            };
            let mut rnode = TreeNode::new(NodeType::TableRow, content);
            rnode.metadata.insert("row_index".to_string(), serde_json::json!(idx));
            let _ = tree.add_node(&table_id, rnode);
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

            let mut table = TreeNode::new(NodeType::Table, label);
            let table_id = table.id.clone();

            // First pass: collect headers and data rows
            let mut headers: Vec<String> = Vec::new();
            let mut data_rows: Vec<Vec<String>> = Vec::new();
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
                if idx == 0 {
                    headers = cells;
                } else {
                    data_rows.push(cells);
                }
            }

            // Build schema summary with sample values
            let schema_lines: Vec<String> = headers.iter().enumerate().map(|(i, col)| {
                let samples: Vec<&str> = data_rows.iter().take(3)
                    .filter_map(|r| r.get(i).map(|s| s.as_str()))
                    .filter(|s| !s.is_empty())
                    .collect();
                if samples.is_empty() { col.clone() } else {
                    format!("{} (e.g. {})", col, samples.join(", "))
                }
            }).collect();
            let schema = format!(
                "Columns: {}\nSchema:\n{}",
                headers.join(", "),
                schema_lines.join("\n")
            );
            table.metadata.insert("columns".to_string(), serde_json::json!(headers));
            table.metadata.insert("total_rows".to_string(), serde_json::json!(data_rows.len()));
            table.metadata.insert("schema".to_string(), serde_json::json!(schema.clone()));
            table.summary = Some(format!("{} rows × {} cols\n{}", data_rows.len(), headers.len(), schema));
            let _ = tree.add_node(&section_id, table);

            // Header row node
            let mut hrow = TreeNode::new(NodeType::TableRow, headers.join(" | "));
            hrow.metadata.insert("is_header".to_string(), serde_json::json!(true));
            let _ = tree.add_node(&table_id, hrow);

            // Data rows with column-keyed content
            for (idx, row) in data_rows.iter().enumerate() {
                let keyed: Vec<String> = headers.iter().zip(row.iter())
                    .filter(|(_, v)| !v.is_empty())
                    .map(|(col, val)| format!("{}: {}", col, val))
                    .collect();
                let content = if keyed.is_empty() { row.join(" | ") } else { keyed.join(" | ") };
                let mut rnode = TreeNode::new(NodeType::TableRow, content);
                rnode.metadata.insert("row_index".to_string(), serde_json::json!(idx));
                let _ = tree.add_node(&table_id, rnode);
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- detect_heading tests ---

    #[test]
    fn heading_chapter() {
        assert_eq!(detect_heading("Chapter 1"), Some(1));
        assert_eq!(detect_heading("chapter 2: introduction"), Some(1));
    }

    #[test]
    fn heading_part() {
        assert_eq!(detect_heading("Part I"), Some(1));
    }

    #[test]
    fn heading_section_keyword() {
        assert_eq!(detect_heading("Section 3.2"), Some(2));
        assert_eq!(detect_heading("Appendix A"), Some(2));
    }

    #[test]
    fn heading_numbered() {
        assert_eq!(detect_heading("1. Introduction"), Some(1));
        assert_eq!(detect_heading("2.3 Methods"), Some(2));
        assert_eq!(detect_heading("1.2.1 Overview"), Some(3));
    }

    #[test]
    fn heading_all_caps() {
        assert_eq!(detect_heading("ABSTRACT"), Some(2));
        assert_eq!(detect_heading("RESULTS AND DISCUSSION"), Some(2));
    }

    #[test]
    fn heading_short_title_case() {
        // "Introduction" and "Related Work" match SECTION_KEYWORDS → level 2
        assert_eq!(detect_heading("Introduction"), Some(2));
        assert_eq!(detect_heading("Related Work"), Some(2));
        // Non-keyword title-case lines → level 3
        assert_eq!(detect_heading("Data Analysis"), Some(3));
    }

    #[test]
    fn heading_section_keywords() {
        // CV sections
        assert_eq!(detect_heading("Education"), Some(2));
        assert_eq!(detect_heading("Work Experience"), Some(2));
        assert_eq!(detect_heading("Skills"), Some(2));
        assert_eq!(detect_heading("Projects"), Some(2));
        assert_eq!(detect_heading("Skills:"), Some(2));
        // Academic
        assert_eq!(detect_heading("methodology"), Some(2));
        assert_eq!(detect_heading("REFERENCES"), Some(2));
    }

    #[test]
    fn not_a_heading_sentence() {
        assert_eq!(detect_heading("This is a normal sentence with details."), None);
        assert_eq!(detect_heading("The results show a clear trend,"), None);
        // Too many words for short title-case rule
        assert_eq!(detect_heading("Lovely Professional University Punjab India Campus"), None);
    }

    #[test]
    fn not_a_heading_empty() {
        assert_eq!(detect_heading(""), None);
        assert_eq!(detect_heading("   "), None);
    }

    #[test]
    fn not_a_heading_too_long() {
        let long = "A".repeat(130);
        assert_eq!(detect_heading(&long), None);
    }

    // --- MarkdownParser tests ---

    #[test]
    fn markdown_parser_basic() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_md_basic.md");
        std::fs::write(&path, "# Title\n\nParagraph one.\n\n## Sub\n\nParagraph two.\n").unwrap();

        let tree = MarkdownParser.parse(path.to_str().unwrap()).unwrap();
        assert_eq!(tree.doc_type, DocType::Markdown);

        let root = tree.get_node(&tree.root_id).unwrap();
        // Should have at least the heading section as child
        assert!(!root.children.is_empty());
    }

    // --- PlainTextParser tests ---

    #[test]
    fn plaintext_parser_splits_paragraphs() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_plain.txt");
        std::fs::write(&path, "First paragraph.\n\nSecond paragraph.\n\nThird.\n").unwrap();

        let tree = PlainTextParser.parse(path.to_str().unwrap()).unwrap();
        let root = tree.get_node(&tree.root_id).unwrap();
        assert_eq!(root.children.len(), 3);
    }

    // --- CodeParser tests ---

    #[test]
    fn code_parser_chunks() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_code.rs");
        let lines: Vec<String> = (1..=120).map(|i| format!("// line {}", i)).collect();
        std::fs::write(&path, lines.join("\n")).unwrap();

        let tree = CodeParser { language: "rust".into() }.parse(path.to_str().unwrap()).unwrap();
        let root = tree.get_node(&tree.root_id).unwrap();
        // 120 lines / 60 per chunk = 2 chunks
        assert_eq!(root.children.len(), 2);
    }

    // --- get_parser_for_file tests ---

    #[test]
    fn parser_dispatch() {
        // Just verify it doesn't panic for various extensions
        let _ = get_parser_for_file("test.md");
        let _ = get_parser_for_file("test.pdf");
        let _ = get_parser_for_file("test.docx");
        let _ = get_parser_for_file("test.csv");
        let _ = get_parser_for_file("test.xlsx");
        let _ = get_parser_for_file("test.rs");
        let _ = get_parser_for_file("test.unknown");
    }
}
