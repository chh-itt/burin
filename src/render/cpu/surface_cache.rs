use crate::core::ElementId;
use crate::style::Color;
use std::collections::HashMap;
use std::rc::Rc;

pub struct CachedSurface {
    pub pixmap: Rc<tiny_skia::Pixmap>,
    pub left: f32,
    pub top: f32,
    pub generation: u64,
    /// Text colour baked into the pixmap. Colour-only changes (state layers,
    /// theme switches) don't bump `text_generation`, so the cache must also
    /// invalidate on colour mismatch (audit 2026-07-16 C7 — stale text colour
    /// on the CPU backend; the GPU applies colour per frame).
    pub color: [u8; 4],
    pub last_used_frame: u64,
}

pub struct TextSurfaceCache {
    entries: HashMap<ElementId, CachedSurface>,
    total_bytes: u64,
    frame_count: u64,
}

fn color_key(c: Color) -> [u8; 4] {
    [
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
        // Alpha applied at blit time (not baked) — excluded from the key.
        0,
    ]
}

impl TextSurfaceCache {
    const MAX_BYTES: u64 = 8 * 1024 * 1024;

    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            total_bytes: 0,
            frame_count: 0,
        }
    }

    pub fn get_or_render(
        &mut self,
        eid: ElementId,
        generation: u64,
        color: Color,
        render: impl FnOnce() -> Option<(tiny_skia::Pixmap, f32, f32)>,
    ) -> Option<(f32, f32, Rc<tiny_skia::Pixmap>)> {
        let ck = color_key(color);
        if let Some(cached) = self.entries.get(&eid) {
            if cached.generation == generation && cached.color == ck {
                let left = cached.left;
                let top = cached.top;
                let pixmap = Rc::clone(&cached.pixmap);
                self.entries.get_mut(&eid).unwrap().last_used_frame = self.frame_count;
                return Some((left, top, pixmap));
            }
        }

        // Stale entry (generation or colour changed): release its bytes.
        if self.entries.contains_key(&eid) {
            self.evict(eid);
        }

        let (pixmap, left, top) = render()?;
        let byte_count = (pixmap.width() * pixmap.height() * 4) as u64;

        while self.total_bytes + byte_count > Self::MAX_BYTES && !self.entries.is_empty() {
            let (&oldest_id, _) = self
                .entries
                .iter()
                .min_by_key(|(_, v)| v.last_used_frame)
                .unwrap();
            self.evict(oldest_id);
        }

        let rc = Rc::new(pixmap);
        self.entries.insert(
            eid,
            CachedSurface {
                pixmap: Rc::clone(&rc),
                left,
                top,
                generation,
                color: ck,
                last_used_frame: self.frame_count,
            },
        );
        self.total_bytes += byte_count;
        Some((left, top, rc))
    }

    fn evict(&mut self, eid: ElementId) {
        if let Some(entry) = self.entries.remove(&eid) {
            self.total_bytes = self
                .total_bytes
                .saturating_sub((entry.pixmap.width() * entry.pixmap.height() * 4) as u64);
        }
    }

    pub fn end_frame(&mut self) {
        self.frame_count = self.frame_count.wrapping_add(1);
    }
}
