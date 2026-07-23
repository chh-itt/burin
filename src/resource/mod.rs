//! Resource management: fonts, images, icons.

use std::collections::HashMap;

/// A handle to a loaded image.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ImageId(u64);

/// A cache for decoded RGBA8 images with reference counting.
pub struct ImageCache {
    images: HashMap<ImageId, CachedImage>,
    #[cfg_attr(not(feature = "ext-image"), allow(dead_code))]
    next_id: u64,
}

struct CachedImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    ref_count: usize,
}

impl ImageCache {
    pub fn new() -> Self {
        Self {
            images: HashMap::new(),
            next_id: 1,
        }
    }

    /// Load an image from encoded bytes (PNG, JPEG, etc.).
    ///
    /// Requires the `ext-image` feature.
    #[cfg(feature = "ext-image")]
    pub fn load_from_bytes(&mut self, bytes: &[u8]) -> Result<ImageId, ImageError> {
        let img = image::load_from_memory(bytes).map_err(|e| ImageError::Decode(e.to_string()))?;
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        let pixels = rgba.into_raw();

        let id = ImageId(self.next_id);
        self.next_id += 1;
        self.images.insert(
            id,
            CachedImage {
                width,
                height,
                pixels,
                ref_count: 1,
            },
        );
        Ok(id)
    }

    #[cfg(not(feature = "ext-image"))]
    pub fn load_from_bytes(&mut self, _bytes: &[u8]) -> Result<ImageId, ImageError> {
        Err(ImageError::Unsupported(
            "image loading requires the `ext-image` feature. Enable it in Cargo.toml.".into(),
        ))
    }

    /// Get image dimensions.
    pub fn size(&self, id: ImageId) -> Option<(u32, u32)> {
        self.images.get(&id).map(|img| (img.width, img.height))
    }

    /// Get the raw RGBA8 pixel data for an image.
    pub fn pixels(&self, id: ImageId) -> Option<&[u8]> {
        self.images.get(&id).map(|img| img.pixels.as_slice())
    }

    /// Increment reference count.
    pub fn retain(&mut self, id: ImageId) {
        if let Some(img) = self.images.get_mut(&id) {
            img.ref_count += 1;
        }
    }

    /// Decrement reference count and remove if zero.
    pub fn release(&mut self, id: ImageId) {
        if let Some(img) = self.images.get_mut(&id) {
            img.ref_count -= 1;
            if img.ref_count == 0 {
                self.images.remove(&id);
            }
        }
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum ImageError {
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("decode error: {0}")]
    Decode(String),
}

/// Built-in icon set (Lucide, MIT license).
///
/// Icons are rendered from SVG path data at the requested size.
pub mod icons {
    /// Icon identifiers.
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
    pub enum Icon {
        Save,
        Delete,
        Edit,
        Copy,
        Paste,
        Cut,
        Undo,
        Redo,
        Search,
        Filter,
        Menu,
        Settings,
        Refresh,
        ArrowLeft,
        ArrowRight,
        ArrowUp,
        ArrowDown,
        Home,
        User,
        Folder,
        File,
        Image,
        Check,
        X,
        AlertCircle,
        Info,
        Play,
        Pause,
        Volume,
        Mail,
        MessageCircle,
        Phone,
        Link,
        Plus,
        Minus,
        Calendar,
    }

    impl Icon {
        /// Get the SVG path data for this icon.
        pub fn path_data(&self) -> &'static str {
            match self {
                Self::Check => "M20 6L9 17l-5-5",
                Self::X => "M18 6L6 18M6 6l12 12",
                Self::Plus => "M12 5v14M5 12h14",
                Self::Minus => "M5 12h14",
                Self::Search => "M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16zM21 21l-4.35-4.35",
                Self::ArrowRight => "M5 12h14M12 5l7 7-7 7",
                Self::ArrowLeft => "M19 12H5M12 19l-7-7 7-7",
                Self::ArrowUp => "M12 19V5M5 12l7-7 7 7",
                Self::ArrowDown => "M12 5v14M19 12l-7 7-7-7",
                Self::Save => "M19 21H5a2 2 0 01-2-2V5a2 2 0 012-2h11l5 5v11a2 2 0 01-2 2zM17 21v-8H7v8M7 3v5h8",
                Self::Delete => "M3 6h18M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2",
                Self::Edit => "M17 3a2.828 2.828 0 114 4L7.5 20.5 2 22l1.5-5.5L17 3z",
                Self::Home => "M3 9l9-7 9 7v11a2 2 0 01-2 2H5a2 2 0 01-2-2zM9 22V12h6v10",
                Self::User => "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2M12 3a4 4 0 1 0 0 8 4 4 0 0 0 0-8z",
                Self::Settings => "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
                Self::Folder => "M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z",
                Self::File => "M14.5 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V7.5L14.5 2zM14 2v6h6",
                Self::Image => "M21 15l-3.086-3.086a2 2 0 0 0-2.828 0L6 21M21 5v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2zM9 10a1 1 0 1 0 0-2 1 1 0 0 0 0 2z",
                Self::Menu => "M4 12h16M4 6h16M4 18h16",
                Self::Refresh => "M1 4v6h6M23 20v-6h-6M20.49 9A9 9 0 0 0 5.64 5.64L1 10m22 4l-4.64 4.36A9 9 0 0 1 5.64 18.36",
                Self::Mail => "M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2zM22 6l-10 7L2 6",
                Self::AlertCircle => "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zM12 8v4m0 4h.01",
                Self::Info => "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zM12 16v-4m0-4h.01",
                Self::Play => "M5 3l14 9-14 9z",
                Self::Pause => "M6 4h4v16H6zM14 4h4v16h-4z",
                Self::Volume => "M11 5L6 9H2v6h4l5 4V5zM18.07 4.93a10 10 0 0 1 0 14.14M14.54 8.46a5 5 0 0 1 0 7.07",
                Self::Filter => "M22 3H2l8 9.46V19l4 2v-8.54L22 3z",
                Self::Copy => "M9 9h13a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H9a2 2 0 0 1-2-2V11a2 2 0 0 1 2-2zM5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1",
                Self::Undo => "M3 10h13a6 6 0 0 1 6 6v0a6 6 0 0 1-6 6H9m-6-12l5-5m-5 5l5 5",
                Self::Redo => "M21 10H8a6 6 0 0 0-6 6v0a6 6 0 0 0 6 6h4m9-12l-5-5m5 5l-5 5",
                Self::MessageCircle => "M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z",
                Self::Phone => "M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z",
                Self::Link => "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71",
                Self::Paste => "M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2m4-2h-4v2h4z",
                Self::Cut => "M6 3a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM6 15a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM20 4L8.12 15.88m6.35-1.4L20 20M8.12 8.12L12 12",
                Self::Calendar => "M19 4h-1V2h-2v2H8V2H6v2H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2zm0 16H5V10h14v10zm0-12H5V6h14v2z",
            }
        }

        /// Build a kurbo::BezPath for this icon.
        ///
        /// Uses the SVG path parser from `render::path` to convert
        /// the stored path data string to a BezPath.
        pub fn build_path(&self) -> Option<kurbo::BezPath> {
            let d = self.path_data();
            if d.is_empty() {
                return None;
            }
            crate::render::path::parse_svg_path(d).ok()
        }
    }
}
