use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::element::{DirtyFlags, ElementId, LazyFontParams};
use crate::render::wgpu::glyphon_bridge::create_buffer;

/// Parameters needed to lazily update a text cell on data change.
pub struct TextCellState {
    pub eid: ElementId,
    pub lazy_label: Rc<Cell<String>>,
    pub text_gen: Rc<Cell<u64>>,
    pub font_size: f32,
    pub line_height: f32,
    pub font_weight: u16,
    pub font_family: Option<String>,
    pub max_width: Option<f32>,
    pub text_align: crate::style::TextAlign,
}

impl TextCellState {
    /// Build a fully-initialised cell: create buffer, wire lazy label/gen.
    pub fn mount(
        eid: ElementId,
        el: &mut crate::core::element::Element,
        text: &str,
        font_size: f32,
        line_height: f32,
        font_weight: u16,
        font_family: Option<String>,
        max_width: Option<f32>,
        text_align: crate::style::TextAlign,
    ) -> Self {
        let buf = Rc::new(RefCell::new(create_buffer(
            text,
            font_size,
            line_height,
            font_weight,
            font_family.as_deref(),
            max_width,
            text_align,
        )));
        el.set_text_buffer(buf);
        let tg = Rc::new(Cell::new(1u64));
        el.set_text_generation(tg.clone());
        el.set_buffer_gen(Rc::new(Cell::new(1u64)));

        let ll = Rc::new(Cell::new(text.to_owned()));
        el.set_lazy_label(ll.clone());
        el.set_lazy_font_params(Rc::new(LazyFontParams {
            font_size,
            line_height,
            font_weight,
            font_family: font_family.clone(),
            max_width,
            text_align,
        }));

        Self {
            eid,
            lazy_label: ll,
            text_gen: tg,
            font_size,
            line_height,
            font_weight,
            font_family,
            max_width,
            text_align,
        }
    }

    /// Push new text into the cell.  The actual text-buffer rebuild is
    /// deferred to the paint phase via the lazy-label mechanism.
    pub fn set_text(&self, text: &str) {
        // Early-out: identical text — skip the gen bump (which would force a
        // re-shape at paint) and the repaint marks entirely.
        let cur = self.lazy_label.take();
        let same = cur == text;
        if same {
            self.lazy_label.set(cur);
            return;
        }
        self.lazy_label.set(text.to_owned());
        self.text_gen.set(self.text_gen.get().wrapping_add(1));
        crate::core::dirty_registry::mark_dirty(self.eid, DirtyFlags::REPAINT);
        crate::core::dirty_registry::register_dirty(self.eid, DirtyFlags::REPAINT);
        crate::core::dirty_registry::bump_subtree_gen(self.eid);
    }
}
