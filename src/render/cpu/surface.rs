use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use std::num::NonZeroU32;
use std::sync::Arc;

/// A wrapper around `Arc<dyn winit::window::Window>` that implements the
/// `HasWindowHandle` + `HasDisplayHandle` traits required by softbuffer.
pub struct WindowRef(pub Arc<dyn winit::window::Window>);

impl HasWindowHandle for WindowRef {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for WindowRef {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

impl Clone for WindowRef {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

pub struct WindowSurface {
    surface: softbuffer::Surface<WindowRef, WindowRef>,
    #[allow(dead_code)]
    context: softbuffer::Context<WindowRef>,
}

impl WindowSurface {
    pub fn new(
        window: Arc<dyn winit::window::Window>,
        width: u32,
        height: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let wr = WindowRef(window);
        let context = softbuffer::Context::new(wr.clone())?;
        let mut surface = softbuffer::Surface::new(&context, wr)?;

        if width > 0 && height > 0 {
            surface.resize(
                NonZeroU32::new(width).unwrap(),
                NonZeroU32::new(height).unwrap(),
            )?;
        }

        Ok(Self { surface, context })
    }

    pub fn buffer_mut(
        &mut self,
    ) -> Result<softbuffer::Buffer<'_, WindowRef, WindowRef>, softbuffer::SoftBufferError> {
        self.surface.buffer_mut()
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), softbuffer::SoftBufferError> {
        if width > 0 && height > 0 {
            self.surface.resize(
                NonZeroU32::new(width).unwrap(),
                NonZeroU32::new(height).unwrap(),
            )
        } else {
            Ok(())
        }
    }
}
