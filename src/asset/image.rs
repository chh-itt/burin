/// A raster image asset (RGBA8 pixel data).
pub struct ImageAsset {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl ImageAsset {
    /// Decode an image from raw bytes (PNG, JPEG, GIF, WebP, etc.).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Ok(Self {
            width: w,
            height: h,
            pixels: rgba.into_raw(),
        })
    }

    /// Create an image from raw RGBA pixels.
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }

    /// RGBA pixel bytes.
    pub fn data(&self) -> &[u8] {
        &self.pixels
    }
}
