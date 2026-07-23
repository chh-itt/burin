use std::time::Duration;

/// A single frame in an animated image.
pub struct AnimatedFrame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub delay: Duration,
}

/// A parsed animated image (GIF, WebP, APNG).
pub struct AnimatedAsset {
    pub frames: Vec<AnimatedFrame>,
    pub total_duration: Duration,
}

impl AnimatedAsset {
    /// Decode an animated image from raw bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        // Try GIF animation decoder
        if let Ok(decoder) = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)) {
            use image::AnimationDecoder;
            if let Ok(frames) = decoder.into_frames().collect_frames() {
                let mut result = Vec::new();
                for frame in frames {
                    let delay = frame.delay().into();
                    let buf = frame.into_buffer();
                    let (fw, fh) = buf.dimensions();
                    result.push(AnimatedFrame {
                        pixels: buf.into_raw(),
                        width: fw,
                        height: fh,
                        delay,
                    });
                }
                if !result.is_empty() {
                    let total = result.iter().map(|f| f.delay).sum();
                    return Ok(Self {
                        frames: result,
                        total_duration: total,
                    });
                }
            }
        }

        // Fallback: decode as static image
        let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Ok(Self {
            frames: vec![AnimatedFrame {
                pixels: rgba.into_raw(),
                width: w,
                height: h,
                delay: Duration::ZERO,
            }],
            total_duration: Duration::ZERO,
        })
    }
}
