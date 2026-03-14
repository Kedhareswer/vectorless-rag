use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImageNode {
    pub id: String,
    pub path: String,
    pub mime_type: String,
    pub description: Option<String>,
    pub dimensions: Option<(u32, u32)>,
}

/// Determine MIME type from file extension.
fn mime_from_ext(filename: &str) -> &'static str {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "tiff" | "tif" => "image/tiff",
        "webp" => "image/webp",
        "emf" => "image/emf",
        "wmf" => "image/wmf",
        _ => "application/octet-stream",
    }
}

/// Resolve the image storage directory for a given document.
/// Creates `{app_data}/images/{doc_id}/` if it doesn't exist.
fn image_dir_for_doc(doc_id: &str) -> Option<PathBuf> {
    let data_dir = app_data_dir()?;
    let dir = data_dir.join("images").join(doc_id);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Get the platform-specific app data directory (same logic as lib.rs).
fn app_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("vectorless-rag"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .ok()
            .map(|p| PathBuf::from(p).join("Library/Application Support/vectorless-rag"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .or_else(|| std::env::var("HOME").ok().map(|h| format!("{}/.local/share", h)))
            .map(|p| PathBuf::from(p).join("vectorless-rag"))
    }
}

/// Extract images from a DOCX file's `word/media/` folder.
/// Writes images to `{app_data}/images/{doc_id}/` and returns ImageNode descriptors.
pub fn extract_images_from_docx(file_path: &str, doc_id: &str) -> Vec<ImageNode> {
    let out_dir = match image_dir_for_doc(doc_id) {
        Some(d) => d,
        None => return Vec::new(),
    };

    let file = match std::fs::File::open(file_path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(_) => return Vec::new(),
    };

    let mut images = Vec::new();
    let media_prefix = "word/media/";

    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let name = match entry.enclosed_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };

        if !name.starts_with(media_prefix) {
            continue;
        }

        let filename = name.strip_prefix(media_prefix).unwrap_or(&name);
        let mime = mime_from_ext(filename);

        // Only extract actual image types
        if !mime.starts_with("image/") {
            continue;
        }

        let out_path = out_dir.join(filename);
        if let Ok(mut out_file) = std::fs::File::create(&out_path) {
            if std::io::copy(&mut entry, &mut out_file).is_ok() {
                let id = uuid::Uuid::new_v4().to_string();
                images.push(ImageNode {
                    id,
                    path: out_path.to_string_lossy().to_string(),
                    mime_type: mime.to_string(),
                    description: None,
                    dimensions: None,
                });
            }
        }
    }

    images
}

/// Extract embedded images from a PDF using lopdf.
/// Walks all page resource dictionaries, collects XObject image streams,
/// decodes them (JPEG/PNG/raw), writes them to `{app_data}/images/{doc_id}/`,
/// and returns ImageNode descriptors.
pub fn extract_images_from_pdf(file_path: &str, doc_id: &str) -> Vec<ImageNode> {
    use lopdf::{Document, Object};

    let out_dir = match image_dir_for_doc(doc_id) {
        Some(d) => d,
        None => return Vec::new(),
    };

    let doc = match Document::load(file_path) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut images = Vec::new();
    let mut img_counter: u32 = 0;

    // Iterate over all pages
    for page_id in doc.page_iter() {
        // Get the Resources dictionary for this page
        let resources = match doc.get_page_resources(page_id) {
            Ok((Some(res), _)) => res.clone(),
            _ => continue,
        };

        // XObject sub-dictionary holds images
        let xobjects = match resources.get(b"XObject") {
            Ok(xobj_ref) => {
                match doc.dereference(xobj_ref) {
                    Ok((_, Object::Dictionary(d))) => d.clone(),
                    _ => continue,
                }
            }
            Err(_) => continue,
        };

        for (_name, xobj_ref) in xobjects.iter() {
            let (_, xobj) = match doc.dereference(xobj_ref) {
                Ok(pair) => (pair.0, pair.1.clone()),
                Err(_) => continue,
            };

            let stream = match xobj {
                Object::Stream(s) => s,
                _ => continue,
            };

            // Only process image XObjects (Subtype = /Image)
            let subtype = stream.dict.get(b"Subtype")
                .ok()
                .and_then(|v| v.as_name().ok().map(|n| String::from_utf8_lossy(n).to_string()));
            if subtype.as_deref() != Some("Image") {
                continue;
            }

            // Determine color space and bit depth for format detection
            let filter = stream.dict.get(b"Filter")
                .ok()
                .and_then(|v| match v {
                    Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                    Object::Array(arr) => arr.first().and_then(|f| {
                        if let Object::Name(n) = f {
                            Some(String::from_utf8_lossy(n).to_string())
                        } else {
                            None
                        }
                    }),
                    _ => None,
                });

            let stream_bytes = match stream.decompressed_content() {
                Ok(b) => b,
                Err(_) => continue,
            };

            if stream_bytes.is_empty() {
                continue;
            }

            // Determine file extension based on filter
            let (ext, mime) = match filter.as_deref() {
                Some("DCTDecode") => ("jpg", "image/jpeg"),
                Some("JPXDecode") => ("jp2", "image/jp2"),
                Some("FlateDecode") | Some("LZWDecode") | None => {
                    // Raw decoded pixels — convert to PNG using dimensions
                    let width = stream.dict.get(b"Width")
                        .ok()
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(0) as u32;
                    let height = stream.dict.get(b"Height")
                        .ok()
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(0) as u32;

                    if width == 0 || height == 0 {
                        continue;
                    }

                    let color_space = stream.dict.get(b"ColorSpace")
                        .ok()
                        .and_then(|v| match v {
                            Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                            Object::Array(arr) => arr.first().and_then(|f| {
                                if let Object::Name(n) = f { Some(String::from_utf8_lossy(n).to_string()) } else { None }
                            }),
                            _ => None,
                        });

                    let is_rgb = color_space.as_deref() == Some("DeviceRGB");
                    let is_gray = color_space.as_deref() == Some("DeviceGray");

                    if is_rgb && stream_bytes.len() == (width * height * 3) as usize {
                        // Convert raw RGB to PNG
                        img_counter += 1;
                        let filename = format!("img_{:04}.png", img_counter);
                        let out_path = out_dir.join(&filename);
                        let img = image::RgbImage::from_raw(width, height, stream_bytes.clone());
                        if let Some(img) = img {
                            if img.save(&out_path).is_ok() {
                                images.push(ImageNode {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    path: out_path.to_string_lossy().to_string(),
                                    mime_type: "image/png".to_string(),
                                    description: None,
                                    dimensions: Some((width, height)),
                                });
                            }
                        }
                        continue;
                    } else if is_gray && stream_bytes.len() == (width * height) as usize {
                        // Convert raw grayscale to PNG
                        img_counter += 1;
                        let filename = format!("img_{:04}.png", img_counter);
                        let out_path = out_dir.join(&filename);
                        let img = image::GrayImage::from_raw(width, height, stream_bytes.clone());
                        if let Some(img) = img {
                            if img.save(&out_path).is_ok() {
                                images.push(ImageNode {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    path: out_path.to_string_lossy().to_string(),
                                    mime_type: "image/png".to_string(),
                                    description: None,
                                    dimensions: Some((width, height)),
                                });
                            }
                        }
                        continue;
                    } else {
                        continue; // unsupported raw format
                    }
                }
                _ => continue, // unknown filter
            };

            // Write the raw compressed bytes directly (JPEG/JP2)
            img_counter += 1;
            let filename = format!("img_{:04}.{}", img_counter, ext);
            let out_path = out_dir.join(&filename);

            if std::fs::write(&out_path, &stream_bytes).is_ok() {
                // Try to read dimensions for JPEG
                let dimensions = if ext == "jpg" {
                    read_jpeg_dimensions(&stream_bytes)
                } else {
                    None
                };
                images.push(ImageNode {
                    id: uuid::Uuid::new_v4().to_string(),
                    path: out_path.to_string_lossy().to_string(),
                    mime_type: mime.to_string(),
                    description: None,
                    dimensions,
                });
            }
        }
    }

    images
}

/// Parse JPEG SOF marker to extract image dimensions without fully decoding.
fn read_jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let mut i = 0;
    while i + 3 < data.len() {
        if data[i] != 0xFF {
            break;
        }
        let marker = data[i + 1];
        // SOF markers: 0xC0..0xC3, 0xC5..0xC7, 0xC9..0xCB, 0xCD..0xCF
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF)
            && i + 9 < data.len() {
                let height = u32::from(data[i + 5]) << 8 | u32::from(data[i + 6]);
                let width = u32::from(data[i + 7]) << 8 | u32::from(data[i + 8]);
                return Some((width, height));
        }
        if i + 3 >= data.len() {
            break;
        }
        let len = (u16::from(data[i + 2]) << 8 | u16::from(data[i + 3])) as usize;
        i += 2 + len;
    }
    None
}

/// Extract images from a file path. Dispatches based on file extension.
pub fn extract_images_from_path(path: &str, doc_id: &str) -> Vec<ImageNode> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "docx" => extract_images_from_docx(path, doc_id),
        "pdf" => extract_images_from_pdf(path, doc_id),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_from_ext_common_types() {
        assert_eq!(mime_from_ext("photo.png"), "image/png");
        assert_eq!(mime_from_ext("photo.jpg"), "image/jpeg");
        assert_eq!(mime_from_ext("photo.jpeg"), "image/jpeg");
        assert_eq!(mime_from_ext("photo.gif"), "image/gif");
        assert_eq!(mime_from_ext("icon.svg"), "image/svg+xml");
        assert_eq!(mime_from_ext("unknown.xyz"), "application/octet-stream");
    }

    #[test]
    fn extract_from_nonexistent_path_returns_empty() {
        let result = extract_images_from_path("/nonexistent/file.docx", "test-doc-id");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_from_non_docx_returns_empty() {
        let result = extract_images_from_path("test.txt", "test-doc-id");
        assert!(result.is_empty());
    }
}
