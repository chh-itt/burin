//! Form + Field widgets with O(k) validation.
//!
//! Form collects fields via the per-window FormDomain active-form slot.
//! Field registers a validator + value getter during mount_box.
//! On submit, Form iterates its field ElementIds, calls each getter + validator.

use std::cell::RefCell;
use std::rc::Rc;

use crate::core::context::MountContext;
use crate::core::dirty_registry;
use crate::core::element::{DirtyFlags, ElementId};
use crate::core::widget::Widget;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::widgets::display::Text;
use crate::widgets::layout::VStack;

// ── AutovalidateMode ──

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
/// When to automatically validate a form field.
pub enum AutovalidateMode {
    Disabled,
    OnChange,
    OnBlur,
    OnSubmit,
}

// ── ValidationComponent (thread_local) ──

type ValidatorFn = Box<dyn Fn(&str) -> Option<String>>;
type ValueGetter = Rc<dyn Fn() -> String>;

struct ValidationState {
    element_id: ElementId,
    validator: Option<ValidatorFn>,
    getter: Option<ValueGetter>,
    error: Option<String>,
    dirty: bool,
    #[allow(dead_code)] // stored but read path not yet built
    autovalidate: AutovalidateMode,
}

// Per-window form-aggregation state (audit 2026-07-18 multi-window pass):
// validators, the active-form mount stack and form→field links are
// per-AppContext via the extension anymap — window A's submit no longer
// validates window B's fields. Value types stay widget-local (no core
// dependency).
//
// Lifecycle (audit 2026-07-17 round 3, Finding A): entries of torn-down
// elements are reclaimed via `dirty_registry::register_teardown_hook`
// (`teardown_cleanup` below) — mount/unmount cycles no longer grow these
// registries.
#[derive(Default)]
struct FormDomain {
    validators: RefCell<Vec<ValidationState>>,
    /// Active form being mounted (single slot; forms don't nest in practice —
    /// mount is synchronous and the slot is save/restored around children).
    active_form: RefCell<Option<ElementId>>,
    /// Map form_id → Vec of field ElementIds
    fields: RefCell<std::collections::HashMap<ElementId, Vec<ElementId>>>,
}

fn form_domain() -> Rc<FormDomain> {
    crate::core::app_context::current_app().extension::<FormDomain>()
}

/// Register a validator for a form field.
pub fn register_validator(
    element_id: ElementId,
    validator: Option<impl Fn(&str) -> Option<String> + 'static>,
    getter: Option<impl Fn() -> String + 'static>,
    autovalidate: AutovalidateMode,
) {
    dirty_registry::register_teardown_hook(teardown_cleanup);
    form_domain().validators.borrow_mut().push(ValidationState {
        element_id,
        validator: validator.map(|f| Box::new(f) as ValidatorFn),
        getter: getter.map(|f| Rc::new(f) as ValueGetter),
        error: None,
        dirty: false,
        autovalidate,
    });
}

/// Teardown hook (audit round 3, Finding A): drop the validator and any
/// form-field links of a removed element. Registered lazily on first
/// `register_validator`/`register_field`; idempotent.
fn teardown_cleanup(id: ElementId) {
    let dom = form_domain();
    dom.validators.borrow_mut().retain(|s| s.element_id != id);
    {
        let mut f = dom.fields.borrow_mut();
        f.remove(&id);
        for fields in f.values_mut() {
            fields.retain(|&fid| fid != id);
        }
    }
}

/// Test-only introspection: (validators, form entries, total field links).
#[doc(hidden)]
pub fn debug_registry_sizes() -> (usize, usize, usize) {
    let dom = form_domain();
    let v = dom.validators.borrow().len();
    let f = dom.fields.borrow();
    (v, f.len(), f.values().map(|v| v.len()).sum())
}

/// Remove a previously registered validator.
pub fn unregister_validator(element_id: ElementId) {
    form_domain()
        .validators
        .borrow_mut()
        .retain(|s| s.element_id != element_id);
}

/// Validate a single field.
pub fn validate_field(element_id: ElementId, value: &str) -> Option<String> {
    let dom = form_domain();
    let mut validators = dom.validators.borrow_mut();
    if let Some(state) = validators.iter_mut().find(|s| s.element_id == element_id) {
        state.dirty = true;
        let error = state.validator.as_ref().and_then(|f| f(value));
        state.error = error.clone();
        dirty_registry::mark_dirty(element_id, DirtyFlags::REPAINT);
        dirty_registry::register_dirty(element_id, DirtyFlags::REPAINT);
        error
    } else {
        None
    }
}

/// Validate all fields belonging to a form. Returns true if all valid.
pub fn validate_form(form_id: ElementId) -> bool {
    let dom = form_domain();
    let field_ids: Vec<ElementId> = dom
        .fields
        .borrow()
        .get(&form_id)
        .cloned()
        .unwrap_or_default();
    let mut all_valid = true;
    {
        let mut validators = dom.validators.borrow_mut();
        for state in validators.iter_mut() {
            if !field_ids.contains(&state.element_id) {
                continue;
            }
            let value = state.getter.as_ref().map_or(String::new(), |g| g());
            state.dirty = true;
            let error = state.validator.as_ref().and_then(|f| f(&value));
            state.error = error.clone();
            if error.is_some() {
                all_valid = false;
            }
            dirty_registry::mark_dirty(state.element_id, DirtyFlags::REPAINT);
            dirty_registry::register_dirty(state.element_id, DirtyFlags::REPAINT);
        }
    }
    all_valid
}

/// Get the current validation error for a field.
pub fn get_error(element_id: ElementId) -> Option<String> {
    form_domain()
        .validators
        .borrow()
        .iter()
        .find(|s| s.element_id == element_id)
        .and_then(|s| s.error.clone())
}

/// Clear the validation error for a field.
pub fn clear_error(element_id: ElementId) {
    if let Some(state) = form_domain()
        .validators
        .borrow_mut()
        .iter_mut()
        .find(|s| s.element_id == element_id)
    {
        state.error = None;
        state.dirty = false;
    }
}

/// Reset all validation errors for a form.
pub fn reset_form_validators(form_id: ElementId) {
    let dom = form_domain();
    let field_ids: Vec<ElementId> = dom
        .fields
        .borrow()
        .get(&form_id)
        .cloned()
        .unwrap_or_default();
    for state in dom.validators.borrow_mut().iter_mut() {
        if field_ids.contains(&state.element_id) {
            state.error = None;
            state.dirty = false;
        }
    }
}

// ── Form-Field connection ──

fn register_field(form_id: ElementId, field_id: ElementId) {
    dirty_registry::register_teardown_hook(teardown_cleanup);
    form_domain()
        .fields
        .borrow_mut()
        .entry(form_id)
        .or_default()
        .push(field_id);
}

// ── Field widget ──

/// A form field with label, validation, and error display.
pub struct Field {
    label: Option<String>,
    description: Option<String>,
    required: bool,
    validator: Option<Rc<dyn Fn(&str) -> Option<String>>>,
    value_getter: Option<Rc<dyn Fn() -> String>>,
    autovalidate: AutovalidateMode,
    child: Option<Box<dyn Widget>>,
    style: StyleRefinement,
}

impl Field {
    pub fn new() -> Self {
        Self {
            label: None,
            description: None,
            required: false,
            validator: None,
            value_getter: None,
            autovalidate: AutovalidateMode::OnSubmit,
            child: None,
            style: StyleRefinement::default(),
        }
    }
    pub fn label(mut self, s: impl Into<String>) -> Self {
        self.label = Some(s.into());
        self
    }
    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.description = Some(s.into());
        self
    }
    pub fn required(mut self, v: bool) -> Self {
        self.required = v;
        self
    }
    pub fn validator(mut self, f: impl Fn(&str) -> Option<String> + 'static) -> Self {
        self.validator = Some(Rc::new(f));
        self
    }
    /// Provide a closure that returns the field's current value at any time (for form submission).
    pub fn value(mut self, f: impl Fn() -> String + 'static) -> Self {
        self.value_getter = Some(Rc::new(f));
        self
    }
    pub fn autovalidate(mut self, mode: AutovalidateMode) -> Self {
        self.autovalidate = mode;
        self
    }
    pub fn child(mut self, widget: impl Widget + 'static) -> Self {
        self.child = Some(Box::new(widget));
        self
    }
}

impl Default for Field {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for Field {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Field {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let container_id = ctx.arena.allocate();
        ctx.preallocate(container_id, self.component_mask());

        // Register with active form
        if let Some(form_id) = *form_domain().active_form.borrow() {
            register_field(form_id, container_id);
        }

        // Label + required marker
        if let Some(label) = self.label {
            let label_text = if self.required {
                format!("{} *", label)
            } else {
                label
            };
            let label_id =
                Box::new(Text::new(&label_text).font_size(13.0).font_weight(600)).mount_box(ctx);
            ctx.arena.add_child(container_id, label_id);
        }

        // Child input
        if let Some(child) = self.child {
            let mut child_ctx = ctx.child_with_events(container_id);
            let child_id = child.mount_box(&mut child_ctx);
            ctx.arena.add_child(container_id, child_id);

            if let Some(ref validator_fn) = self.validator {
                let vfn = validator_fn.clone();
                let getter = self.value_getter.clone();
                register_validator(
                    child_id,
                    Some(move |value: &str| vfn(value)),
                    getter.map(|g| move || g()),
                    self.autovalidate,
                );
            }
        }

        // Error text element (shown when error is present)
        if self.validator.is_some() {
            let err_id = ctx.arena.allocate();
            {
                let Some(el) = ctx.arena.get_mut(err_id) else {
                    return container_id;
                };
                el.set_font_size(11.0);
                el.set_foreground(crate::style::Color::rgba8(220, 38, 38, 255));
                el.set_visible(false);
            }
            ctx.arena.add_child(container_id, err_id);
        } else if let Some(desc) = self.description {
            let desc_id = Box::new(Text::new(&desc).font_size(11.0)).mount_box(ctx);
            ctx.arena.add_child(container_id, desc_id);
        }

        container_id
    }
}

// ── Form widget ──

/// A form container that groups fields and handles submission.
pub struct Form {
    fields: Vec<Field>,
    on_submit: Option<Rc<dyn Fn()>>,
    form_id: Option<ElementId>,
    style: StyleRefinement,
}

impl Form {
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            on_submit: None,
            form_id: None,
            style: StyleRefinement::default(),
        }
    }
    pub fn child(mut self, field: Field) -> Self {
        self.fields.push(field);
        self
    }
    pub fn on_submit(mut self, f: impl Fn() + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }
    /// Validate all fields and submit if valid.
    pub fn submit(&self) -> bool {
        if validate_form(self.form_id.expect("Form not mounted")) {
            if let Some(ref cb) = self.on_submit {
                cb();
            }
            true
        } else {
            false
        }
    }
    /// Reset all field errors.
    pub fn reset(&self) {
        if let Some(fid) = self.form_id {
            reset_form_validators(fid);
        }
    }
}

impl Default for Form {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for Form {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Form {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        self.form_id = Some(id);

        // Push onto the active-form slot so child Field widgets can register
        form_domain().active_form.replace(Some(id));
        let mut stack = VStack::new().gap(12.0);
        for field in self.fields {
            stack = stack.push(field);
        }
        let stack_id = Box::new(stack).mount_box(ctx);
        ctx.arena.add_child(id, stack_id);

        // Pop self from the active-form slot
        *form_domain().active_form.borrow_mut() = None;

        id
    }
}

impl std::fmt::Debug for Form {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Form")
            .field("fields", &self.fields.len())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Field").finish_non_exhaustive()
    }
}
