//! Clipboard integration (feature `clipboard`).
//!
//! Zero-sized handle wrapping a lazily-initialized thread-local
//! `arboard::Clipboard`.  Text operations are always available when
//! the feature is enabled; image operations additionally require
//! the `ext-image` feature.

use std::cell::RefCell;
use thiserror::Error;

#[cfg(feature = "clipboard")]
thread_local! {
    static CLIPBOARD: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) };
}

// ── Errors ────────────────────────────────────────────────────

/// Clipboard operation error.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum ClipboardError {
    /// Clipboard is not available (feature disabled, platform unsupported,
    /// or initialisation failed — e.g. no display on Linux).
    #[error("Clipboard not available")]
    NotAvailable,

    /// Clipboard contents are not a supported image.
    #[error("Clipboard does not contain a valid image")]
    NoImage,

    /// Low-level platform error.
    #[error("Clipboard error: {0}")]
    Platform(String),
}

// ── Image data ────────────────────────────────────────────────

/// RGBA8 image data for clipboard read / write operations.
///
/// Requires both `clipboard` and `ext-image` features.
#[cfg(all(feature = "clipboard", feature = "ext-image"))]
#[derive(Clone, Debug)]
pub struct ClipboardImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA8 pixel data, row-major, 4 bytes per pixel.
    pub pixels: Vec<u8>,
}

// ── Clipboard provider trait ────────────────────────────────────

/// Trait for custom clipboard providers.
///
/// Third-party crates can implement this trait to provide alternative
/// clipboard backends (e.g., remote desktop, VNC, custom sandbox).
/// Register via [`set_clipboard_provider`].
pub trait ClipboardProvider: 'static {
    fn read_text(&self) -> Result<Option<String>, ClipboardError>;
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError>;
    #[cfg(all(feature = "clipboard", feature = "ext-image"))]
    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardError> {
        Err(ClipboardError::NoImage)
    }
    #[cfg(all(feature = "clipboard", feature = "ext-image"))]
    fn write_image(&mut self, _image: &ClipboardImage) -> Result<(), ClipboardError> {
        Err(ClipboardError::NotAvailable)
    }
}

// ── Global provider ────────────────────────────────────────────

#[cfg(feature = "clipboard")]
thread_local! {
    static CLIPBOARD_PROVIDER: RefCell<Option<Box<dyn ClipboardProvider>>> = const { RefCell::new(None) };
}

/// Set a custom clipboard provider. All clipboard operations will
/// route through this provider. Pass `None` to restore the default
/// platform clipboard.
#[cfg(feature = "clipboard")]
pub fn set_clipboard_provider(provider: Option<Box<dyn ClipboardProvider>>) {
    CLIPBOARD_PROVIDER.with(|cell| {
        *cell.borrow_mut() = provider;
    });
}

// ── Default platform provider (arboard) ────────────────────────

#[cfg(feature = "clipboard")]
struct ArboardProvider;

#[cfg(feature = "clipboard")]
impl ClipboardProvider for ArboardProvider {
    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        init()?;
        CLIPBOARD.with(|cell| {
            let mut cb = cell.borrow_mut();
            let cb = cb.as_mut().ok_or(ClipboardError::NotAvailable)?;
            match cb.get_text() {
                Ok(t) => Ok(Some(t)),
                Err(arboard::Error::ContentNotAvailable) => Ok(None),
                Err(e) => Err(ClipboardError::Platform(e.to_string())),
            }
        })
    }
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        init()?;
        CLIPBOARD.with(|cell| {
            let mut cb = cell.borrow_mut();
            let cb = cb.as_mut().ok_or(ClipboardError::NotAvailable)?;
            cb.set_text(text)
                .map_err(|e| ClipboardError::Platform(e.to_string()))
        })
    }
    #[cfg(all(feature = "clipboard", feature = "ext-image"))]
    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardError> {
        init()?;
        CLIPBOARD.with(|cell| {
            let mut cb = cell.borrow_mut();
            let cb = cb.as_mut().ok_or(ClipboardError::NotAvailable)?;
            match cb.get_image() {
                Ok(img) => Ok(Some(ClipboardImage {
                    width: img.width as u32,
                    height: img.height as u32,
                    pixels: img.bytes.into_owned(),
                })),
                Err(arboard::Error::ContentNotAvailable) => Ok(None),
                Err(_) => Err(ClipboardError::NoImage),
            }
        })
    }
    #[cfg(all(feature = "clipboard", feature = "ext-image"))]
    fn write_image(&mut self, image: &ClipboardImage) -> Result<(), ClipboardError> {
        init()?;
        let arboard_img = arboard::ImageData {
            width: image.width as usize,
            height: image.height as usize,
            bytes: std::borrow::Cow::Borrowed(&image.pixels),
        };
        CLIPBOARD.with(|cell| {
            let mut cb = cell.borrow_mut();
            let cb = cb.as_mut().ok_or(ClipboardError::NotAvailable)?;
            cb.set_image(arboard_img)
                .map_err(|e| ClipboardError::Platform(e.to_string()))
        })
    }
}

// ── Clipboard handle ──────────────────────────────────────────

/// Clipboard handle for reading and writing system clipboard contents.
///
/// This is a **zero-sized type** — all state lives in a thread-local
/// `arboard::Clipboard` that is lazily initialised on first use.
///
/// # Feature gating
///
/// | Method | Requires |
/// |--------|----------|
/// | `read_text` / `write_text` | `clipboard` |
/// | `read_image` / `write_image` | `clipboard` + `ext-image` |
///
/// When required features are missing every method returns
/// [`ClipboardError::NotAvailable`] — no conditional compilation is
/// needed in user code for basic usage.
pub struct Clipboard;

impl Clipboard {
    /// Create a clipboard handle.
    ///
    /// Always succeeds — the underlying platform clipboard is opened
    /// lazily on the first read/write call.
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl Default for Clipboard {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

// ── Text operations ───────────────────────────────────────────

impl Clipboard {
    /// Read text from the system clipboard.
    ///
    /// Returns `Ok(None)` when the clipboard is empty or does not
    /// contain text.  Returns `Err(ClipboardError::NotAvailable)`
    /// when the `clipboard` feature is disabled or the platform
    /// clipboard is unavailable.
    pub fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        #[cfg(feature = "clipboard")]
        {
            if CLIPBOARD_PROVIDER.with(|c| c.borrow().is_some()) {
                return CLIPBOARD_PROVIDER.with(|c| c.borrow().as_ref().unwrap().read_text());
            }
            ArboardProvider.read_text()
        }
        #[cfg(not(feature = "clipboard"))]
        Err(ClipboardError::NotAvailable)
    }

    /// Write text to the system clipboard.
    pub fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        #[cfg(feature = "clipboard")]
        {
            if CLIPBOARD_PROVIDER.with(|c| c.borrow().is_some()) {
                return CLIPBOARD_PROVIDER
                    .with(|c| c.borrow_mut().as_mut().unwrap().write_text(text));
            }
            ArboardProvider.write_text(text)
        }
        #[cfg(not(feature = "clipboard"))]
        Err(ClipboardError::NotAvailable)
    }
}

// ── Image operations ──────────────────────────────────────────

#[cfg(all(feature = "clipboard", feature = "ext-image"))]
impl Clipboard {
    /// Read an image from the system clipboard.
    ///
    /// Returns `Ok(None)` when the clipboard is empty or contains
    /// a non-image format.  Requires both `clipboard` and
    /// `ext-image` features.
    pub fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardError> {
        if CLIPBOARD_PROVIDER.with(|c| c.borrow().is_some()) {
            return CLIPBOARD_PROVIDER.with(|c| c.borrow().as_ref().unwrap().read_image());
        }
        ArboardProvider.read_image()
    }

    /// Write an RGBA8 image to the system clipboard.
    pub fn write_image(&self, image: &ClipboardImage) -> Result<(), ClipboardError> {
        if CLIPBOARD_PROVIDER.with(|c| c.borrow().is_some()) {
            return CLIPBOARD_PROVIDER
                .with(|c| c.borrow_mut().as_mut().unwrap().write_image(image));
        }
        ArboardProvider.write_image(image)
    }
}

// ── Internal helpers ──────────────────────────────────────────

#[cfg(feature = "clipboard")]
fn init() -> Result<(), ClipboardError> {
    CLIPBOARD.with(|cell| {
        let mut cb = cell.borrow_mut();
        if cb.is_some() {
            return Ok(());
        }
        match arboard::Clipboard::new() {
            Ok(c) => {
                *cb = Some(c);
                Ok(())
            }
            Err(e) => {
                // Store None so we don't retry every time
                *cb = None;
                Err(ClipboardError::Platform(e.to_string()))
            }
        }
    })
}
