use std::collections::HashMap;
use std::path::PathBuf;

/// Collects images extracted during import and writes them as sidecar files
/// under `<import_dir>/assets/`, returning relative Markdown paths that stay
/// valid regardless of where the import directory is eventually relocated to.
pub struct MediaSink {
    assets_dir: PathBuf,
    next_index: usize,
    /// Maps the original archive part name (e.g. `word/media/image3.png`) to
    /// the relative markdown path already written for it, so a media part
    /// referenced multiple times is only written once.
    written: HashMap<String, String>,
    /// Optional cap on the longest edge of a written raster, in pixels. When
    /// `None`, images are written verbatim (no downscale). Off by default.
    max_dimension: Option<u32>,
}

/// Extensions that the Tauri webview can render directly as `<img>` sources.
const RENDERABLE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

impl MediaSink {
    pub fn new(assets_dir: PathBuf) -> Self {
        MediaSink {
            assets_dir,
            next_index: 1,
            written: HashMap::new(),
            max_dimension: None,
        }
    }

    /// Enable downscaling: any written raster whose longest edge exceeds
    /// `max_dimension` pixels is resized down before being written to disk.
    pub fn with_max_dimension(mut self, max_dimension: Option<u32>) -> Self {
        self.max_dimension = max_dimension;
        self
    }

    /// Register an image part. `orig_name` is the archive/part name (used both
    /// for de-duplication and to infer the file extension). Returns the
    /// relative Markdown image path (e.g. `assets/image1.png`) on success, or
    /// `None` if the format isn't renderable in the webview (e.g. EMF/WMF) —
    /// callers should fall back to a text note in that case.
    pub fn add(&mut self, orig_name: &str, bytes: &[u8]) -> Option<String> {
        if let Some(existing) = self.written.get(orig_name) {
            return Some(existing.clone());
        }

        let ext = orig_name
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();

        if !RENDERABLE_EXTS.contains(&ext.as_str()) {
            return None;
        }

        let filename = format!("image{}.{}", self.next_index, ext);
        let dest = self.assets_dir.join(&filename);

        let write_result = if let Some(max_dim) = self.max_dimension {
            self.write_downscaled(&dest, bytes, max_dim)
        } else {
            std::fs::create_dir_all(&self.assets_dir).and_then(|_| std::fs::write(&dest, bytes))
        };

        if write_result.is_err() {
            return None;
        }

        self.next_index += 1;
        let rel_path = format!("assets/{}", filename);
        self.written.insert(orig_name.to_string(), rel_path.clone());
        Some(rel_path)
    }

    fn write_downscaled(
        &self,
        dest: &PathBuf,
        bytes: &[u8],
        max_dim: u32,
    ) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.assets_dir)?;
        match image::load_from_memory(bytes) {
            Ok(img) => {
                let (w, h) = (img.width(), img.height());
                let resized = if w > max_dim || h > max_dim {
                    img.resize(max_dim, max_dim, image::imageops::FilterType::Lanczos3)
                } else {
                    img
                };
                resized.save(dest).map_err(std::io::Error::other)
            }
            // If decoding fails (unusual raster/corrupt data), fall back to
            // writing the original bytes verbatim rather than dropping the image.
            Err(_) => std::fs::write(dest, bytes),
        }
    }

    /// True if any image was actually written (used to decide whether the
    /// `assets/` directory should be kept or cleaned up).
    pub fn is_empty(&self) -> bool {
        self.written.is_empty()
    }
}
