use crate::render::wgpu;
use std::collections::HashMap;
use std::rc::Rc;

pub struct ImageCache {
    images: HashMap<u64, CachedImage>,
    frame: u64,
}

struct CachedImage {
    pixmaps: Vec<Rc<tiny_skia::Pixmap>>,
    full_w: u32,
    full_h: u32,
    last_frame: u64,
}

impl ImageCache {
    const MAX_ENTRIES: usize = 32;

    pub fn new() -> Self {
        Self {
            images: HashMap::new(),
            frame: 0,
        }
    }

    /// Returns the closest mip Pixmap for the given target output size.
    /// `target_w` and `target_h` are physical (output) pixels.
    pub fn ensure_mip(
        &mut self,
        hash: u64,
        target_w: u32,
        target_h: u32,
    ) -> Option<Rc<tiny_skia::Pixmap>> {
        self.frame += 1;

        // Fast path: already cached
        if let Some(img) = self.images.get_mut(&hash) {
            let idx = select_mip(
                img.pixmaps.len(),
                img.full_w,
                img.full_h,
                target_w,
                target_h,
            );
            img.last_frame = self.frame;
            return Some(Rc::clone(&img.pixmaps[idx]));
        }

        // Evict oldest if at capacity
        if self.images.len() >= Self::MAX_ENTRIES {
            let oldest_key = self
                .images
                .iter()
                .min_by_key(|(_, c)| c.last_frame)
                .map(|(k, _)| *k);
            if let Some(k) = oldest_key {
                self.images.remove(&k);
            }
        }

        // Load mip chain from shared IMAGE_REGISTRY
        let mips = wgpu::lookup_image_mips(hash)?;
        let full_w = mips.width;
        let full_h = mips.height;
        let mut pixmaps: Vec<Rc<tiny_skia::Pixmap>> = Vec::with_capacity(mips.levels.len());
        for (i, level_data) in mips.levels.iter().enumerate() {
            let lw = (full_w >> i).max(1);
            let lh = (full_h >> i).max(1);
            let mut pixmap = tiny_skia::Pixmap::new(lw, lh)?;
            let dst = bytemuck::cast_slice_mut::<u8, u32>(pixmap.data_mut());
            let src_flat: &[u8] = level_data;
            // Registry data is straight-alpha RGBA; tiny-skia pixmaps are
            // premultiplied RGBA (canonical format: u32 = A<<24|B<<16|G<<8|R).
            for y in 0..lh as usize {
                for x in 0..lw as usize {
                    let si = (y * lw as usize + x) * 4;
                    let di = y * lw as usize + x;
                    if si + 4 <= src_flat.len() {
                        let r = src_flat[si] as u32;
                        let g = src_flat[si + 1] as u32;
                        let b = src_flat[si + 2] as u32;
                        let a = src_flat[si + 3] as u32;
                        let pr = r * a / 255;
                        let pg = g * a / 255;
                        let pb = b * a / 255;
                        dst[di] = (a << 24) | (pb << 16) | (pg << 8) | pr;
                    }
                }
            }
            pixmaps.push(Rc::new(pixmap));
        }
        let idx = select_mip(pixmaps.len(), full_w, full_h, target_w, target_h);
        self.images.insert(
            hash,
            CachedImage {
                pixmaps,
                full_w,
                full_h,
                last_frame: self.frame,
            },
        );
        self.images.get(&hash).map(|c| Rc::clone(&c.pixmaps[idx]))
    }
}

/// Pick the highest (smallest) mip level where both dimensions are >= target.
fn select_mip(level_count: usize, full_w: u32, full_h: u32, target_w: u32, target_h: u32) -> usize {
    if level_count <= 1 {
        return 0;
    }
    let mut best = 0usize;
    for i in 0..level_count {
        let mw = (full_w >> i).max(1);
        let mh = (full_h >> i).max(1);
        if mw >= target_w && mh >= target_h {
            best = i;
        } else {
            break;
        }
    }
    best.min(level_count - 1)
}
