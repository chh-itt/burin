// ═══════════════════════ WindowIcon ═══════════════════════

/// RGBA pixel data for a window icon.
#[derive(Clone)]
pub struct WindowIcon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl WindowIcon {
    pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba,
        }
    }

    #[cfg(feature = "ext-image")]
    pub fn from_image_bytes(bytes: &[u8]) -> Result<Self, String> {
        let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
        let rgba = img.into_rgba8();
        Ok(Self {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        })
    }

    pub(crate) fn to_winit_icon(&self) -> winit::icon::Icon {
        let ri = winit::icon::RgbaIcon::new(self.rgba.clone(), self.width, self.height)
            .expect("WindowIcon: invalid RGBA data");
        winit::icon::Icon::from(ri)
    }
}

impl std::fmt::Debug for WindowIcon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowIcon")
            .field("dimensions", &(self.width, self.height))
            .finish_non_exhaustive()
    }
}
