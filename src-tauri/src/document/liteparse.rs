//! Layout-aware PDF parsing via the native LiteParse crate (PDFium-based).
//!
//! LiteParse classifies each page into layout blocks — headings with levels,
//! paragraphs, list items, tables with cells, code — which map directly onto
//! the Universal Document Tree. It needs the PDFium shared library at runtime;
//! the build bundles it next to the executable (see `build.rs`). If PDFium can't
//! be loaded, `is_available()` is false and the caller falls back to pdf-extract.
//!
//! A scanned (image-only) PDF yields no blocks on the first pass, so it is
//! parsed a second time with the ONNX OCR backend (see `ocr_engine`).

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use liteparse::layout::LayoutBlock;
use liteparse::ocr::OcrEngine;
use liteparse::ocr::oar::OarOcrEngine;
use liteparse::{LiteParse, LiteParseConfig, OutputFormat};

use super::tree::{DocType, DocumentTree, NodeType, TreeNode};

#[cfg(target_os = "windows")]
const PDFIUM_LIB_NAME: &str = "pdfium.dll";
#[cfg(target_os = "macos")]
const PDFIUM_LIB_NAME: &str = "libpdfium.dylib";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const PDFIUM_LIB_NAME: &str = "libpdfium.so";

/// Cached load result for the PDFium shared library.
static AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Load PDFium from the app's bundled resources. Call once at startup, before
/// any parsing; if it isn't found there, the default search (env var, build
/// directory, next to the executable, system paths) still applies.
pub fn configure_library_dir(dir: &Path) {
    let _ = RESOURCE_DIR.set(dir.to_path_buf());
    let path = dir.join(PDFIUM_LIB_NAME);
    if path.exists()
        && let Err(e) = pdfium_sys::dynamic::load(&path)
    {
        eprintln!("LiteParse: {}", e);
    }
}

/// Whether the PDFium library could be loaded. Cached for the session.
pub fn is_available() -> bool {
    *AVAILABLE.get_or_init(|| match pdfium_sys::dynamic::load_default() {
        Ok(()) => true,
        Err(e) => {
            eprintln!("LiteParse unavailable, using fallback PDF parser: {}", e);
            false
        }
    })
}

/// Cached OCR engine. Built on the first scanned PDF, not at startup: the
/// constructor loads the ONNX models (downloading them on first use).
static OCR_ENGINE: Mutex<Option<Arc<dyn OcrEngine>>> = Mutex::new(None);

/// The app's `resources/` directory, recorded by `configure_library_dir`.
static RESOURCE_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

#[cfg(target_os = "windows")]
const ORT_LIB_NAME: &str = "onnxruntime.dll";
#[cfg(target_os = "macos")]
const ORT_LIB_NAME: &str = "libonnxruntime.dylib";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const ORT_LIB_NAME: &str = "libonnxruntime.so";

/// Point `ort` at the bundled ONNX Runtime.
///
/// `ort` is built with `load-dynamic`, so it loads the library by path at
/// runtime and ships none itself. If the library is missing, say so instead of
/// letting `ort` block while it searches: a scanned PDF then falls back to the
/// pdf-extract parser, the same as before OCR existed.
fn configure_ort() -> Result<(), String> {
    let dir = RESOURCE_DIR
        .get()
        .ok_or("OCR unavailable: resource directory not configured")?;
    let lib = dir.join(ORT_LIB_NAME);
    if !lib.exists() {
        return Err(format!(
            "OCR unavailable: {} not found in {}",
            ORT_LIB_NAME,
            dir.display()
        ));
    }
    // SAFETY: single-threaded startup path for this engine; `ort` reads the
    // variable when it first loads the library.
    unsafe { std::env::set_var("ORT_DYLIB_PATH", &lib) };
    Ok(())
}

/// The ONNX OCR pipeline (PP-OCRv6 small). `oar-ocr` downloads the detection
/// model, recognition model and dictionary on first use, verifies their
/// SHA-256 and caches them under `$OAR_HOME` (default `~/.oar`).
fn ocr_engine() -> Result<Arc<dyn OcrEngine>, String> {
    let mut slot = OCR_ENGINE
        .lock()
        .map_err(|_| "OCR engine lock poisoned".to_string())?;
    if let Some(engine) = slot.as_ref() {
        return Ok(engine.clone());
    }
    configure_ort()?;
    let engine: Arc<dyn OcrEngine> = Arc::new(
        OarOcrEngine::ppocr_v6_small().map_err(|e| format!("OCR engine unavailable: {}", e))?,
    );
    *slot = Some(engine.clone());
    Ok(engine)
}

/// Parse a PDF into a document tree. Pages with a text layer are read
/// directly; a document that yields nothing that way is scanned, so it is
/// parsed again with OCR. Errors let the caller fall back to another parser.
pub fn parse_pdf(file_path: &str, file_name: String) -> Result<DocumentTree, String> {
    if !is_available() {
        return Err("LiteParse not available".to_string());
    }

    match parse_with(file_path, file_name.clone(), false) {
        Ok(tree) => Ok(tree),
        Err(e) if e == NO_CONTENT => {
            eprintln!("LiteParse: no text layer in {}, running OCR", file_path);
            parse_with(file_path, file_name, true)
        }
        Err(e) => Err(e),
    }
}

/// Error returned when a parse produced no usable nodes.
const NO_CONTENT: &str = "LiteParse returned no content";

fn parse_with(file_path: &str, file_name: String, ocr: bool) -> Result<DocumentTree, String> {
    let config = LiteParseConfig {
        ocr_enabled: ocr,
        output_format: OutputFormat::Json,
        extract_blocks: true,
        extract_links: false,
        continue_on_page_error: true,
        quiet: true,
        ..Default::default()
    };
    // Built before the worker thread so a model download failure is reported
    // as itself rather than as a parse error.
    let engine = if ocr { Some(ocr_engine()?) } else { None };

    // `LiteParse::parse` is async. Run it on its own thread + runtime so this
    // works whether or not the caller is already inside a Tokio runtime.
    let path = file_path.to_string();
    let result = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("LiteParse runtime error: {}", e))?;
        let parser = match engine {
            Some(engine) => LiteParse::new(config).with_ocr_engine(engine),
            None => LiteParse::new(config),
        };
        runtime
            .block_on(parser.parse(&path))
            .map_err(|e| format!("LiteParse error: {}", e))
    })
    .join()
    .map_err(|_| "LiteParse panicked".to_string())??;

    let pages: Vec<PageBlocks> = result
        .pages
        .into_iter()
        .map(|p| PageBlocks {
            page_number: p.page_number,
            blocks: p.blocks.unwrap_or_default(),
            text: p.text,
        })
        .collect();

    let tree = build_tree(file_name, result.total_pages as usize, &pages);
    if tree.nodes.len() <= 1 {
        return Err(NO_CONTENT.to_string());
    }
    Ok(tree)
}

/// One page's layout blocks, plus its plain text for pages the classifier
/// could not decompose.
struct PageBlocks {
    page_number: usize,
    blocks: Vec<LayoutBlock>,
    text: String,
}

fn content_node(node_type: NodeType, content: String, page: usize) -> TreeNode {
    let mut node = TreeNode::new(node_type, content);
    node.metadata.insert("page_number".to_string(), serde_json::json!(page));
    node
}

fn paragraph_node(text: &str, page: usize) -> TreeNode {
    let mut node = content_node(NodeType::Paragraph, text.to_string(), page);
    let wc = text.split_whitespace().count();
    node.metadata.insert("word_count".to_string(), serde_json::json!(wc));
    node
}

fn cells_text(cells: &[liteparse::layout::LayoutCell]) -> String {
    cells
        .iter()
        .map(|c| c.text.trim())
        .collect::<Vec<_>>()
        .join(" | ")
}

fn build_tree(file_name: String, total_pages: usize, pages: &[PageBlocks]) -> DocumentTree {
    let mut tree = DocumentTree::new(file_name, DocType::Pdf);
    let root_id = tree.root_id.clone();
    if let Some(root) = tree.nodes.get_mut(&root_id) {
        root.metadata.insert("page_count".to_string(), serde_json::json!(total_pages));
        root.metadata.insert("parse_source".to_string(), serde_json::json!("liteparse"));
    }

    // (heading level, node id); the root acts as level 0.
    let mut section_stack: Vec<(u8, String)> = vec![(0, root_id.clone())];

    for page in pages {
        let page_num = page.page_number;

        if page.blocks.is_empty() {
            let text = page.text.trim();
            if !text.is_empty() {
                let parent = section_stack.last().map(|(_, id)| id.clone()).unwrap_or_default();
                let _ = tree.add_node(&parent, paragraph_node(text, page_num));
            }
            continue;
        }

        for block in &page.blocks {
            let parent = section_stack.last().map(|(_, id)| id.clone()).unwrap_or_default();
            let text = block.text.as_deref().map(str::trim).unwrap_or("");

            match block.kind.as_str() {
                "heading" if !text.is_empty() => {
                    let level = block.level.unwrap_or(1).max(1);
                    while section_stack.len() > 1
                        && section_stack.last().is_some_and(|(l, _)| *l >= level)
                    {
                        section_stack.pop();
                    }
                    let parent = section_stack.last().map(|(_, id)| id.clone()).unwrap_or_default();
                    let mut node = content_node(NodeType::Section, text.to_string(), page_num);
                    node.metadata.insert("heading_level".to_string(), serde_json::json!(level));
                    let id = node.id.clone();
                    if tree.add_node(&parent, node).is_ok() {
                        section_stack.push((level, id));
                    }
                }
                "paragraph" if !text.is_empty() => {
                    let _ = tree.add_node(&parent, paragraph_node(text, page_num));
                }
                "list_item" if !text.is_empty() => {
                    let mut node = content_node(NodeType::ListItem, text.to_string(), page_num);
                    if let Some(depth) = block.level {
                        node.metadata.insert("list_depth".to_string(), serde_json::json!(depth));
                    }
                    let _ = tree.add_node(&parent, node);
                }
                "code" | "grid_fallback" => {
                    let lines = block.lines.as_deref().unwrap_or_default().join("\n");
                    if !lines.trim().is_empty() {
                        let mut node = content_node(NodeType::CodeBlock, lines, page_num);
                        if let Some(ref lang) = block.lang {
                            node.metadata.insert("language".to_string(), serde_json::json!(lang));
                        }
                        let _ = tree.add_node(&parent, node);
                    }
                }
                "table" | "merged_table" => {
                    let mut rows: Vec<String> = Vec::new();
                    if let Some(ref header) = block.header {
                        rows.push(cells_text(header));
                    }
                    for row in block.rows.as_deref().unwrap_or_default() {
                        rows.push(cells_text(row));
                    }
                    rows.retain(|r| !r.replace('|', "").trim().is_empty());
                    if rows.is_empty() {
                        continue;
                    }
                    let table = content_node(NodeType::Table, "Table".to_string(), page_num);
                    let table_id = table.id.clone();
                    if tree.add_node(&parent, table).is_ok() {
                        for row in rows {
                            let _ = tree.add_node(&table_id, content_node(NodeType::TableRow, row, page_num));
                        }
                    }
                }
                // Rules carry no text; figures are extracted separately (lopdf).
                _ => {}
            }
        }
    }

    tree
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(json: &str) -> Vec<LayoutBlock> {
        serde_json::from_str(json).unwrap()
    }

    fn children_of<'a>(tree: &'a DocumentTree, id: &str) -> Vec<&'a TreeNode> {
        tree.get_node(id)
            .map(|n| n.children.iter().filter_map(|c| tree.get_node(c)).collect())
            .unwrap_or_default()
    }

    #[test]
    fn builds_nested_sections_with_content_and_pages() {
        let pages = vec![
            PageBlocks {
                page_number: 1,
                blocks: blocks(
                    r#"[
                    {"kind":"heading","text":"Introduction","level":1},
                    {"kind":"paragraph","text":"Opening text here."},
                    {"kind":"heading","text":"Scope","level":2},
                    {"kind":"list_item","text":"First point","level":1}
                ]"#,
                ),
                text: String::new(),
            },
            PageBlocks {
                page_number: 2,
                blocks: blocks(
                    r#"[
                    {"kind":"heading","text":"Results","level":1},
                    {"kind":"table","header":[{"text":"Year"},{"text":"Revenue"}],
                     "rows":[[{"text":"2025"},{"text":"$4M"}]]},
                    {"kind":"rule"}
                ]"#,
                ),
                text: String::new(),
            },
        ];
        let tree = build_tree("report.pdf".to_string(), 2, &pages);
        let top = children_of(&tree, &tree.root_id);
        let titles: Vec<&str> = top.iter().map(|n| n.content.as_str()).collect();
        assert_eq!(titles, vec!["Introduction", "Results"]);

        let intro = children_of(&tree, &top[0].id);
        assert_eq!(intro[0].node_type, NodeType::Paragraph);
        assert_eq!(intro[1].content, "Scope");
        let scope = children_of(&tree, &intro[1].id);
        assert_eq!(scope[0].node_type, NodeType::ListItem);

        let results = children_of(&tree, &top[1].id);
        assert_eq!(results.len(), 1, "rule blocks are skipped");
        assert_eq!(results[0].node_type, NodeType::Table);
        assert_eq!(results[0].metadata["page_number"], serde_json::json!(2));
        let rows: Vec<&str> = children_of(&tree, &results[0].id)
            .iter()
            .map(|n| n.content.as_str())
            .collect();
        assert_eq!(rows, vec!["Year | Revenue", "2025 | $4M"]);
    }

    #[test]
    fn page_without_blocks_falls_back_to_page_text() {
        let pages = vec![PageBlocks {
            page_number: 3,
            blocks: Vec::new(),
            text: "  Plain page text  ".to_string(),
        }];
        let tree = build_tree("x.pdf".to_string(), 3, &pages);
        let top = children_of(&tree, &tree.root_id);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].content, "Plain page text");
    }

    #[test]
    fn empty_document_has_only_root() {
        let tree = build_tree("empty.pdf".to_string(), 1, &[]);
        assert_eq!(tree.nodes.len(), 1);
    }

    #[test]
    fn parses_a_real_pdf_when_pdfium_is_present() {
        // Skipped where the PDFium library isn't available to the test binary.
        if !is_available() {
            return;
        }
        let pdf = minimal_pdf("Hello LiteParse");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello.pdf");
        std::fs::write(&path, pdf).unwrap();

        let tree = parse_pdf(path.to_str().unwrap(), "hello.pdf".to_string()).unwrap();
        let all_text: String = tree.nodes.values().map(|n| n.content.clone()).collect();
        assert!(all_text.contains("Hello LiteParse"), "got: {}", all_text);
    }

    #[test]
    fn ocr_reads_a_scanned_pdf() {
        // Opt-in: point `TGG_SCANNED_PDF` at an image-only PDF. Downloads the
        // OCR models on first run, so it stays out of the default test run.
        let Ok(path) = std::env::var("TGG_SCANNED_PDF") else {
            return;
        };
        if !is_available() {
            return;
        }
        let tree = parse_pdf(&path, "scanned.pdf".to_string()).unwrap();
        let all_text: String = tree.nodes.values().map(|n| n.content.clone()).collect();
        assert!(
            all_text.split_whitespace().count() > 5,
            "OCR produced no text: {}",
            all_text
        );
    }

    /// Build a one-page PDF with a single line of Helvetica text.
    fn minimal_pdf(text: &str) -> Vec<u8> {
        let stream = format!("BT /F1 24 Tf 72 700 Td ({}) Tj ET", text);
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{}\nendstream", stream.len(), stream),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, obj) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, obj).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for off in offsets {
            out.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                objects.len() + 1,
                xref
            )
            .as_bytes(),
        );
        out
    }
}
