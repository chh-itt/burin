//! Unified asset store: images, SVGs, animations.
//!
//! All loaded assets are keyed by [`AssetId`] and stored in the thread-local
//! `ASSETS` registry.  Both GPU and CPU rendering backends read from here.
//!
//! # Lifetime protocol (audit 2026-07-18)
//!
//! Every entry is reference-counted through the same element-teardown
//! protocol as the GPU pixel registry (`render::wgpu::register_image_for`):
//!
//! - **Element-owned** (`load_*_owned` + [`retain_asset_for`]): widgets
//!   load through an `AssetGuard` (RAII, covers the construct→mount
//!   window) and attach the asset to their element at mount. The entry is
//!   freed when the last referencing element is torn down. Identical bytes
//!   dedup to one entry by content hash.
//! - **Pinned** (public `load_*`): the historical semantics — the entry
//!   stays alive until an explicit [`unload`]. Use for app-lifetime assets
//!   or externally managed caches.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::core::id::ElementId;

#[cfg(feature = "ext-image")]
mod animated;
mod image;
#[cfg(feature = "ext-svg")]
mod svg;

pub use image::ImageAsset;

#[cfg(feature = "ext-svg")]
pub use svg::SvgAsset;

#[cfg(feature = "ext-image")]
pub use animated::{AnimatedAsset, AnimatedFrame};

/// Opaque handle for a loaded asset.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AssetId(pub u64);

thread_local! {
    pub(crate) static ASSETS: RefCell<AssetStore> = RefCell::new(AssetStore::new());
}

/// A stored asset plus its lifetime bookkeeping.
struct Entry<T> {
    asset: Rc<T>,
    /// Content hash for dedup (first 512 bytes + length).
    hash: u64,
    /// Live references: element attachments + in-flight [`AssetGuard`]s.
    refs: usize,
    /// Pinned entries (public `load_*` API) survive `refs == 0`.
    pinned: bool,
}

pub struct AssetStore {
    next_id: u64,
    images: HashMap<AssetId, Entry<ImageAsset>>,
    #[cfg(feature = "ext-svg")]
    svgs: HashMap<AssetId, Entry<SvgAsset>>,
    #[cfg(feature = "ext-image")]
    animations: HashMap<AssetId, Entry<AnimatedAsset>>,
    /// Reverse index for the element-teardown protocol.
    by_element: HashMap<ElementId, Vec<AssetId>>,
}

impl AssetStore {
    fn new() -> Self {
        Self {
            next_id: 1,
            images: HashMap::new(),
            #[cfg(feature = "ext-svg")]
            svgs: HashMap::new(),
            #[cfg(feature = "ext-image")]
            animations: HashMap::new(),
            by_element: HashMap::new(),
        }
    }

    fn alloc_id(&mut self) -> AssetId {
        let id = AssetId(self.next_id);
        self.next_id += 1;
        id
    }

    /// `refs += 1` on whichever kind map holds `id`. Returns `false` when
    /// the id is unknown (already freed or never loaded).
    fn bump(&mut self, id: AssetId) -> bool {
        if let Some(e) = self.images.get_mut(&id) {
            e.refs += 1;
            return true;
        }
        #[cfg(feature = "ext-svg")]
        if let Some(e) = self.svgs.get_mut(&id) {
            e.refs += 1;
            return true;
        }
        #[cfg(feature = "ext-image")]
        if let Some(e) = self.animations.get_mut(&id) {
            e.refs += 1;
            return true;
        }
        false
    }

    /// `refs -= 1`; frees the entry when it reaches zero and is not pinned.
    fn unbump(&mut self, id: AssetId) {
        macro_rules! release_in {
            ($map:expr) => {
                if let Some(e) = $map.get_mut(&id) {
                    e.refs = e.refs.saturating_sub(1);
                    if e.refs == 0 && !e.pinned {
                        $map.remove(&id);
                    }
                    return;
                }
            };
        }
        release_in!(self.images);
        #[cfg(feature = "ext-svg")]
        release_in!(self.svgs);
        #[cfg(feature = "ext-image")]
        release_in!(self.animations);
    }
}

// ── Element-teardown protocol ───────────────────────────────────────

thread_local! {
    static TEARDOWN_HOOK_INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn ensure_teardown_hook() {
    TEARDOWN_HOOK_INSTALLED.with(|installed| {
        if !installed.get() {
            installed.set(true);
            crate::core::dirty_registry::register_teardown_hook(asset_teardown_cleanup);
        }
    });
}

fn asset_teardown_cleanup(eid: ElementId) {
    ASSETS.with(|a| {
        let mut store = a.borrow_mut();
        if let Some(ids) = store.by_element.remove(&eid) {
            for id in ids {
                store.unbump(id);
            }
        }
    });
}

/// Attach `id` to `eid`: the asset now lives at least as long as the
/// element. Released automatically by the unified teardown protocol
/// (`teardown_subtree` → teardown hooks).
///
/// Pinned entries accept attachments too — the element reference is
/// tracked, but the entry still survives `refs == 0` until [`unload`].
pub fn retain_asset_for(eid: ElementId, id: AssetId) {
    ensure_teardown_hook();
    ASSETS.with(|a| {
        let mut store = a.borrow_mut();
        if store.bump(id) {
            store.by_element.entry(eid).or_default().push(id);
        }
    });
}

/// RAII reference covering the widget-construction → mount window.
///
/// `load_*_owned` returns one; the widget stores it. If the widget is
/// mounted, [`retain_asset_for`] adds the element's reference before the
/// guard drops with the consumed widget. If the widget is never mounted,
/// the guard's drop releases the only reference and the entry is freed.
pub(crate) struct AssetGuard(AssetId);

impl Drop for AssetGuard {
    fn drop(&mut self) {
        ASSETS.with(|a| a.borrow_mut().unbump(self.0));
    }
}

/// Release a **pinned** asset loaded through the public `load_*` API.
///
/// The entry is freed immediately when no element references remain;
/// otherwise it is unpinned and freed when the last element dies.
pub fn unload(id: AssetId) {
    ASSETS.with(|a| {
        let mut store = a.borrow_mut();
        macro_rules! unpin_in {
            ($map:expr) => {
                if let Some(e) = $map.get_mut(&id) {
                    e.pinned = false;
                    if e.refs == 0 {
                        $map.remove(&id);
                    }
                    return;
                }
            };
        }
        unpin_in!(store.images);
        #[cfg(feature = "ext-svg")]
        unpin_in!(store.svgs);
        #[cfg(feature = "ext-image")]
        unpin_in!(store.animations);
    });
}

/// Live entry counts `(images, svgs, animations)` — leak-guard observability.
#[doc(hidden)]
pub fn debug_asset_counts() -> (usize, usize, usize) {
    ASSETS.with(|a| {
        let s = a.borrow();
        let images = s.images.len();
        #[cfg(feature = "ext-svg")]
        let svgs = s.svgs.len();
        #[cfg(not(feature = "ext-svg"))]
        let svgs = 0;
        #[cfg(feature = "ext-image")]
        let animations = s.animations.len();
        #[cfg(not(feature = "ext-image"))]
        let animations = 0;
        (images, svgs, animations)
    })
}

/// Load a raster image (PNG, JPEG, GIF static frame, WebP).
///
/// **Pinned**: stays loaded until [`unload`]. Identical bytes dedup to
/// the existing entry.
///
/// On decode failure a 1×1 transparent fallback is stored instead, so the
/// caller always receives a valid [`AssetId`] (no white-screen from missing
/// or corrupt image data).
#[cfg(feature = "ext-image")]
pub fn load_image(bytes: &[u8]) -> Result<AssetId, String> {
    let hash = hash_bytes(bytes);
    if let Some(id) = find_image_by_hash(&hash) {
        ASSETS.with(|a| {
            if let Some(e) = a.borrow_mut().images.get_mut(&id) {
                e.pinned = true;
            }
        });
        return Ok(id);
    }
    let img = match image::ImageAsset::from_bytes(bytes) {
        Ok(img) => img,
        Err(err) => {
            // Fallback: 1×1 transparent RGBA pixel prevents white-screen when
            // image data is corrupt or in an unsupported format.
            let fallback = image::ImageAsset::from_rgba(1, 1, vec![0, 0, 0, 0]);
            let fallback_hash = hash_bytes(&[0u8; 16]); // collision-resistant sentinel
            let id = ASSETS.with(|a| {
                let mut store = a.borrow_mut();
                let id = store.alloc_id();
                store.images.insert(
                    id,
                    Entry {
                        asset: Rc::new(fallback),
                        hash: fallback_hash,
                        refs: 0,
                        pinned: true,
                    },
                );
                id
            });
            crate::core::error::push_error(crate::core::error::UiError::Image(
                crate::resource::ImageError::Decode(err),
            ));
            return Ok(id);
        }
    };
    let id = ASSETS.with(|a| {
        let mut store = a.borrow_mut();
        let id = store.alloc_id();
        store.images.insert(
            id,
            Entry {
                asset: Rc::new(img),
                hash,
                refs: 0,
                pinned: true,
            },
        );
        id
    });
    Ok(id)
}

/// Load an SVG image (requires `ext-svg` feature).
///
/// **Pinned**: stays loaded until [`unload`]. Identical bytes dedup to
/// the existing entry.
#[cfg(feature = "ext-svg")]
pub fn load_svg(bytes: &[u8]) -> Result<AssetId, String> {
    let hash = hash_bytes(bytes);
    if let Some(id) = find_svg_by_hash(&hash) {
        ASSETS.with(|a| {
            if let Some(e) = a.borrow_mut().svgs.get_mut(&id) {
                e.pinned = true;
            }
        });
        return Ok(id);
    }
    let svg = svg::SvgAsset::from_bytes(bytes)?;
    let id = ASSETS.with(|a| {
        let mut store = a.borrow_mut();
        let id = store.alloc_id();
        store.svgs.insert(
            id,
            Entry {
                asset: Rc::new(svg),
                hash,
                refs: 0,
                pinned: true,
            },
        );
        id
    });
    Ok(id)
}

/// Load an SVG with element-owned lifetime (widget-internal path).
///
/// Dedups by content hash against ALL entries (owned or pinned). The
/// returned [`AssetGuard`] holds one reference for the construct→mount
/// window; widgets call [`retain_asset_for`] at mount to hand ownership
/// to their element.
#[cfg(feature = "ext-svg")]
pub(crate) fn load_svg_owned(bytes: &[u8]) -> Result<(AssetId, AssetGuard), String> {
    let hash = hash_bytes(bytes);
    if let Some(id) = find_svg_by_hash(&hash) {
        ASSETS.with(|a| a.borrow_mut().bump(id));
        return Ok((id, AssetGuard(id)));
    }
    let svg = svg::SvgAsset::from_bytes(bytes)?;
    let id = ASSETS.with(|a| {
        let mut store = a.borrow_mut();
        let id = store.alloc_id();
        store.svgs.insert(
            id,
            Entry {
                asset: Rc::new(svg),
                hash,
                refs: 1,
                pinned: false,
            },
        );
        id
    });
    Ok((id, AssetGuard(id)))
}

/// Load an animated image (GIF, WebP, APNG).
///
/// **Pinned**: stays loaded until [`unload`]. Identical bytes dedup to
/// the existing entry.
#[cfg(feature = "ext-image")]
pub fn load_animated(bytes: &[u8]) -> Result<AssetId, String> {
    let hash = hash_bytes(bytes);
    let existing = ASSETS.with(|a| {
        a.borrow().animations.iter().find_map(
            |(&id, e)| {
                if e.hash == hash {
                    Some(id)
                } else {
                    None
                }
            },
        )
    });
    if let Some(id) = existing {
        ASSETS.with(|a| {
            if let Some(e) = a.borrow_mut().animations.get_mut(&id) {
                e.pinned = true;
            }
        });
        return Ok(id);
    }
    let anim = animated::AnimatedAsset::from_bytes(bytes)?;
    let id = ASSETS.with(|a| {
        let mut store = a.borrow_mut();
        let id = store.alloc_id();
        store.animations.insert(
            id,
            Entry {
                asset: Rc::new(anim),
                hash,
                refs: 0,
                pinned: true,
            },
        );
        id
    });
    Ok(id)
}

/// Look up a loaded raster image by id.
pub fn image_asset(id: AssetId) -> Option<Rc<ImageAsset>> {
    ASSETS.with(|a| a.borrow().images.get(&id).map(|e| Rc::clone(&e.asset)))
}

/// Look up a loaded SVG by id.
#[cfg(feature = "ext-svg")]
pub fn svg_asset(id: AssetId) -> Option<Rc<SvgAsset>> {
    ASSETS.with(|a| a.borrow().svgs.get(&id).map(|e| Rc::clone(&e.asset)))
}

/// Look up a loaded animation by id.
#[cfg(feature = "ext-image")]
pub fn animated_asset(id: AssetId) -> Option<Rc<AnimatedAsset>> {
    ASSETS.with(|a| a.borrow().animations.get(&id).map(|e| Rc::clone(&e.asset)))
}

/// Rasterize an SVG to RGBA pixels at the given size.
#[cfg(feature = "ext-svg")]
pub fn rasterize_svg(id: AssetId, width: u32, height: u32) -> Option<(u32, u32, Vec<u8>)> {
    let svg = svg_asset(id)?;
    let pixels = svg.rasterize(width, height);
    Some((width, height, pixels))
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let len = bytes.len().min(512);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes[..len].hash(&mut hasher);
    bytes.len().hash(&mut hasher);
    hasher.finish()
}

#[cfg(feature = "ext-image")]
fn find_image_by_hash(hash: &u64) -> Option<AssetId> {
    ASSETS.with(|a| {
        a.borrow()
            .images
            .iter()
            .find_map(|(&id, e)| if e.hash == *hash { Some(id) } else { None })
    })
}

#[cfg(feature = "ext-svg")]
fn find_svg_by_hash(hash: &u64) -> Option<AssetId> {
    ASSETS.with(|a| {
        a.borrow()
            .svgs
            .iter()
            .find_map(|(&id, e)| if e.hash == *hash { Some(id) } else { None })
    })
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "ext-svg")]
    mod svg_lifecycle {
        use super::super::*;

        const SVG_A: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="red"/></svg>"#;
        const SVG_B: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><circle cx="10" cy="10" r="9" fill="blue"/></svg>"#;

        fn svg_count() -> usize {
            debug_asset_counts().1
        }

        #[test]
        fn load_svg_dedups_identical_bytes() {
            let base = svg_count();
            let a = load_svg(SVG_A).unwrap();
            let b = load_svg(SVG_A).unwrap();
            assert_eq!(a, b, "identical bytes must resolve to one AssetId");
            assert_eq!(svg_count(), base + 1);
            let c = load_svg(SVG_B).unwrap();
            assert_ne!(a, c);
            assert_eq!(svg_count(), base + 2);
            unload(a);
            unload(c);
            assert_eq!(svg_count(), base);
        }

        #[test]
        fn owned_guard_drop_without_element_frees_entry() {
            let base = svg_count();
            let (id, guard) = load_svg_owned(SVG_B).unwrap();
            assert!(svg_asset(id).is_some());
            assert_eq!(svg_count(), base + 1);
            drop(guard);
            assert_eq!(
                svg_count(),
                base,
                "an owned asset whose widget was never mounted must be freed on guard drop"
            );
        }

        #[test]
        fn owned_asset_survives_until_element_teardown() {
            let base = svg_count();
            let eid = crate::core::id::ElementId::allocate();
            let (id, guard) = load_svg_owned(SVG_B).unwrap();
            retain_asset_for(eid, id);
            drop(guard); // widget consumed by mount — element ref keeps it alive
            assert_eq!(svg_count(), base + 1);

            crate::core::dirty_registry::run_teardown_hooks(eid);
            assert_eq!(
                svg_count(),
                base,
                "element teardown must release its asset reference"
            );
        }

        #[test]
        fn shared_owned_asset_freed_only_after_last_element_dies() {
            let base = svg_count();
            let e1 = crate::core::id::ElementId::allocate();
            let e2 = crate::core::id::ElementId::allocate();
            let (id1, g1) = load_svg_owned(SVG_A).unwrap();
            let (id2, g2) = load_svg_owned(SVG_A).unwrap();
            assert_eq!(id1, id2, "owned loads dedup by content hash");
            retain_asset_for(e1, id1);
            retain_asset_for(e2, id2);
            drop(g1);
            drop(g2);
            assert_eq!(svg_count(), base + 1);

            crate::core::dirty_registry::run_teardown_hooks(e1);
            assert_eq!(svg_count(), base + 1, "second element still owns the asset");
            crate::core::dirty_registry::run_teardown_hooks(e2);
            assert_eq!(svg_count(), base);
        }

        #[test]
        fn pinned_load_survives_element_teardown_until_unload() {
            let base = svg_count();
            let eid = crate::core::id::ElementId::allocate();
            let id = load_svg(SVG_B).unwrap();
            retain_asset_for(eid, id);
            crate::core::dirty_registry::run_teardown_hooks(eid);
            assert_eq!(
                svg_count(),
                base + 1,
                "pinned assets (external load_svg) outlive element references"
            );
            unload(id);
            assert_eq!(svg_count(), base);
        }
    }
}
