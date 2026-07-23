//! FilePicker — button that opens a native OS file dialog (feature-gated `file-dialog`).

#[cfg(not(feature = "file-dialog"))]
use std::cell::RefCell;
use std::rc::Rc;

#[cfg(feature = "file-dialog")]
use std::path::PathBuf;

use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
#[cfg(feature = "file-dialog")]
use crate::platform::{FileDialogBuilder, SelectedFile};
use crate::style::styled::{StyleRefinement, Styled};
#[cfg(feature = "file-dialog")]
use crate::theme::{Appearance, ControlShape, ControlSize, Intent};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FilePickerMode {
    Open,
    OpenMultiple,
    Save,
    Folder,
}

pub struct FilePickerButton {
    label: String,
    #[cfg(feature = "file-dialog")]
    mode: FilePickerMode,
    #[cfg(feature = "file-dialog")]
    filter_name: Option<String>,
    #[cfg(feature = "file-dialog")]
    filter_exts: Vec<String>,
    #[cfg(feature = "file-dialog")]
    default_dir: Option<PathBuf>,
    #[cfg(feature = "file-dialog")]
    default_filename: Option<String>,
    #[cfg(feature = "file-dialog")]
    dialog_title: Option<String>,
    #[cfg(feature = "file-dialog")]
    on_file_selected: Option<Rc<dyn Fn(SelectedFile)>>,
    #[cfg(feature = "file-dialog")]
    on_files_selected: Option<Rc<dyn Fn(Vec<SelectedFile>)>>,
    #[cfg(feature = "file-dialog")]
    on_cancelled: Option<Rc<dyn Fn()>>,
    #[cfg(feature = "file-dialog")]
    path_signal: Option<auralis_signal::Signal<Option<PathBuf>>>,
    #[cfg(feature = "file-dialog")]
    disabled: bool,
    #[cfg(feature = "file-dialog")]
    intent: Intent,
    #[cfg(feature = "file-dialog")]
    appearance: Appearance,
    #[cfg(feature = "file-dialog")]
    size: ControlSize,
    #[cfg(feature = "file-dialog")]
    shape: ControlShape,
    style: StyleRefinement,
}

impl FilePickerButton {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            #[cfg(feature = "file-dialog")]
            mode: FilePickerMode::Open,
            #[cfg(feature = "file-dialog")]
            filter_name: None,
            #[cfg(feature = "file-dialog")]
            filter_exts: Vec::new(),
            #[cfg(feature = "file-dialog")]
            default_dir: None,
            #[cfg(feature = "file-dialog")]
            default_filename: None,
            #[cfg(feature = "file-dialog")]
            dialog_title: None,
            #[cfg(feature = "file-dialog")]
            on_file_selected: None,
            #[cfg(feature = "file-dialog")]
            on_files_selected: None,
            #[cfg(feature = "file-dialog")]
            on_cancelled: None,
            #[cfg(feature = "file-dialog")]
            path_signal: None,
            #[cfg(feature = "file-dialog")]
            disabled: false,
            #[cfg(feature = "file-dialog")]
            intent: Intent::Default,
            #[cfg(feature = "file-dialog")]
            appearance: Appearance::Filled,
            #[cfg(feature = "file-dialog")]
            size: ControlSize::Medium,
            #[cfg(feature = "file-dialog")]
            shape: ControlShape::Rounded,
            style: StyleRefinement::default(),
        }
    }

    #[cfg(feature = "file-dialog")]
    pub fn mode(mut self, m: FilePickerMode) -> Self {
        self.mode = m;
        self
    }

    #[cfg(feature = "file-dialog")]
    pub fn filter(mut self, name: impl Into<String>, exts: &[&str]) -> Self {
        self.filter_name = Some(name.into());
        self.filter_exts = exts.iter().map(|s| s.to_string()).collect();
        self
    }

    #[cfg(feature = "file-dialog")]
    pub fn directory(mut self, dir: impl Into<PathBuf>) -> Self {
        self.default_dir = Some(dir.into());
        self
    }

    #[cfg(feature = "file-dialog")]
    pub fn default_filename(mut self, name: impl Into<String>) -> Self {
        self.default_filename = Some(name.into());
        self
    }

    #[cfg(feature = "file-dialog")]
    pub fn dialog_title(mut self, title: impl Into<String>) -> Self {
        self.dialog_title = Some(title.into());
        self
    }

    #[cfg(feature = "file-dialog")]
    pub fn open_mode(self) -> Self {
        self.mode(FilePickerMode::Open)
    }

    #[cfg(feature = "file-dialog")]
    pub fn save_mode(self) -> Self {
        self.mode(FilePickerMode::Save)
    }

    #[cfg(feature = "file-dialog")]
    pub fn folder_mode(self) -> Self {
        self.mode(FilePickerMode::Folder)
    }

    #[cfg(feature = "file-dialog")]
    pub fn multi_mode(self) -> Self {
        self.mode(FilePickerMode::OpenMultiple)
    }

    #[cfg(feature = "file-dialog")]
    pub fn on_file_selected(mut self, f: impl Fn(SelectedFile) + 'static) -> Self {
        self.on_file_selected = Some(Rc::new(f));
        self
    }

    #[cfg(feature = "file-dialog")]
    pub fn on_files_selected(mut self, f: impl Fn(Vec<SelectedFile>) + 'static) -> Self {
        self.on_files_selected = Some(Rc::new(f));
        self
    }

    #[cfg(feature = "file-dialog")]
    pub fn on_cancelled(mut self, f: impl Fn() + 'static) -> Self {
        self.on_cancelled = Some(Rc::new(f));
        self
    }

    #[cfg(feature = "file-dialog")]
    pub fn bind_path(mut self, signal: auralis_signal::Signal<Option<PathBuf>>) -> Self {
        self.path_signal = Some(signal);
        self
    }

    #[cfg(feature = "file-dialog")]
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    #[cfg(feature = "file-dialog")]
    pub fn intent(mut self, i: Intent) -> Self {
        self.intent = i;
        self
    }

    #[cfg(feature = "file-dialog")]
    pub fn appearance(mut self, a: Appearance) -> Self {
        self.appearance = a;
        self
    }

    #[cfg(feature = "file-dialog")]
    pub fn size(mut self, s: ControlSize) -> Self {
        self.size = s;
        self
    }

    #[cfg(feature = "file-dialog")]
    pub fn shape(mut self, s: ControlShape) -> Self {
        self.shape = s;
        self
    }
}

impl Styled for FilePickerButton {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for FilePickerButton {
    #[cfg(feature = "file-dialog")]
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let dialog_title = self.dialog_title.clone();
        let filter_name = self.filter_name.clone();
        let filter_exts = self.filter_exts.clone();
        let default_dir = self.default_dir.clone();
        let default_filename = self.default_filename.clone();
        let mode = self.mode;
        let path_signal = self.path_signal.clone();
        let on_selected = self.on_file_selected.clone();
        let on_multi_selected = self.on_files_selected.clone();
        let on_cancelled = self.on_cancelled.clone();

        let on_click = move || {
            let mut builder = FileDialogBuilder::new();
            if let Some(ref t) = dialog_title {
                builder = builder.title(t.clone());
            }
            if let Some(ref n) = filter_name {
                let exts: Vec<String> = filter_exts.clone();
                builder = builder.filter(n.clone(), exts);
            }
            if let Some(ref d) = default_dir {
                builder = builder.directory(d.clone());
            }
            if let Some(ref n) = default_filename {
                builder = builder.default_filename(n.clone());
            }

            let path_sig = path_signal.clone();
            let cb = on_selected.clone();
            let cb_multi = on_multi_selected.clone();
            let cb_cancel = on_cancelled.clone();

            #[cfg(not(target_arch = "wasm32"))]
            auralis_task::spawn_global(async move {
                let result = match mode {
                    FilePickerMode::Open => builder.pick_file().map(|f| (Some(f), None)),
                    FilePickerMode::OpenMultiple => match builder.pick_files() {
                        Some(files) if !files.is_empty() => {
                            Some((Some(files[0].clone()), Some(files)))
                        }
                        _ => None,
                    },
                    FilePickerMode::Folder => builder.pick_folder().map(|f| (Some(f), None)),
                    FilePickerMode::Save => builder.save_file().map(|f| (Some(f), None)),
                };

                match result {
                    Some((single, multi)) => {
                        if let Some(sig) = &path_sig {
                            sig.set(Some(single.as_ref().unwrap().path.clone()));
                        }
                        if let Some(ref multi_files) = multi {
                            if let Some(ref cb) = cb_multi {
                                cb(multi_files.clone());
                            }
                        } else if let Some(sf) = single {
                            if let Some(ref cb) = cb {
                                cb(sf);
                            }
                        }
                    }
                    None => {
                        if let Some(ref cb) = cb_cancel {
                            cb();
                        }
                    }
                }
            });
        };

        let mut btn = crate::widgets::input::Button::new(&self.label)
            .on_click(on_click)
            .intent(self.intent)
            .appearance(self.appearance)
            .size(self.size)
            .shape(self.shape);

        if self.disabled {
            btn = btn.disabled();
        }

        {
            let btn_style = btn.style_refinement();
            if let Some(w) = self.style.width {
                btn_style.width = Some(w);
            }
            if let Some(h) = self.style.height {
                btn_style.height = Some(h);
            }
            if let Some(mw) = self.style.min_width {
                btn_style.min_width = Some(mw);
            }
            if let Some(mw) = self.style.max_width {
                btn_style.max_width = Some(mw);
            }
            if let Some(p) = self.style.padding {
                btn_style.padding = Some(p);
            }
            if let Some(m) = self.style.margin {
                btn_style.margin = Some(m);
            }
        }

        Box::new(btn).mount_box(ctx)
    }

    #[cfg(not(feature = "file-dialog"))]
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        let Some(el) = ctx.arena.get_mut(id) else {
            return id;
        };
        el.set_preferred_height(32.0);
        el.set_font_size(14.0);
        let bl = format!("{} (needs file-dialog)", self.label);
        let buf = Rc::new(RefCell::new(
            crate::render::wgpu::glyphon_bridge::create_buffer(
                &bl,
                14.0,
                1.5,
                400,
                None,
                None,
                crate::style::TextAlign::Start,
            ),
        ));
        el.set_text_buffer(buf);
        id
    }
}

impl std::fmt::Debug for FilePickerButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dbg = f.debug_struct("FilePickerButton");
        dbg.field("label", &self.label);
        #[cfg(feature = "file-dialog")]
        {
            dbg.field("mode", &self.mode);
            dbg.field("disabled", &self.disabled);
            dbg.field("intent", &self.intent);
            dbg.field("appearance", &self.appearance);
        }
        dbg.finish_non_exhaustive()
    }
}
