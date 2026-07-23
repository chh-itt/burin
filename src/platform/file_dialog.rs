//! Native file dialogs (requires `file-dialog` feature).

use std::path::PathBuf;

/// Open a file picker dialog for selecting an existing file.
#[cfg(feature = "file-dialog")]
pub fn pick_file() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_file()
}

#[cfg(not(feature = "file-dialog"))]
pub fn pick_file() -> Option<PathBuf> {
    None
}

/// Open a file picker dialog for selecting multiple files.
#[cfg(feature = "file-dialog")]
pub fn pick_files() -> Option<Vec<PathBuf>> {
    rfd::FileDialog::new().pick_files()
}

#[cfg(not(feature = "file-dialog"))]
pub fn pick_files() -> Option<Vec<PathBuf>> {
    None
}

/// Open a folder picker dialog.
#[cfg(feature = "file-dialog")]
pub fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

#[cfg(not(feature = "file-dialog"))]
pub fn pick_folder() -> Option<PathBuf> {
    None
}

/// Open a save file dialog.
#[cfg(feature = "file-dialog")]
pub fn save_file() -> Option<PathBuf> {
    rfd::FileDialog::new().save_file()
}

#[cfg(not(feature = "file-dialog"))]
pub fn save_file() -> Option<PathBuf> {
    None
}

/// File dialog result — a single selected path with metadata.
#[derive(Clone, Debug)]
pub struct SelectedFile {
    pub path: PathBuf,
}

impl SelectedFile {
    pub fn name(&self) -> Option<&str> {
        self.path.file_name().and_then(|n| n.to_str())
    }

    pub fn extension(&self) -> Option<&str> {
        self.path.extension().and_then(|e| e.to_str())
    }
}

/// File dialog builder with full configuration.
pub struct FileDialogBuilder {
    title: Option<String>,
    filters: Vec<(String, Vec<String>)>,
    directory: Option<PathBuf>,
    default_filename: Option<String>,
}

impl FileDialogBuilder {
    pub fn new() -> Self {
        Self {
            title: None,
            filters: Vec::new(),
            directory: None,
            default_filename: None,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn filter(mut self, name: impl Into<String>, extensions: Vec<impl Into<String>>) -> Self {
        self.filters.push((
            name.into(),
            extensions.into_iter().map(|e| e.into()).collect(),
        ));
        self
    }

    pub fn directory(mut self, dir: impl Into<PathBuf>) -> Self {
        self.directory = Some(dir.into());
        self
    }

    pub fn default_filename(mut self, name: impl Into<String>) -> Self {
        self.default_filename = Some(name.into());
        self
    }

    #[cfg(feature = "file-dialog")]
    fn apply(self, dialog: rfd::FileDialog) -> rfd::FileDialog {
        let mut d = dialog;
        if let Some(ref t) = self.title {
            d = d.set_title(t);
        }
        if let Some(ref dir) = self.directory {
            d = d.set_directory(dir);
        }
        if let Some(ref name) = self.default_filename {
            d = d.set_file_name(name);
        }
        for (name, exts) in &self.filters {
            d = d.add_filter(name, exts);
        }
        d
    }

    #[cfg(feature = "file-dialog")]
    pub fn pick_file(self) -> Option<SelectedFile> {
        self.apply(rfd::FileDialog::new())
            .pick_file()
            .map(|p| SelectedFile { path: p })
    }

    #[cfg(not(feature = "file-dialog"))]
    pub fn pick_file(self) -> Option<SelectedFile> {
        None
    }

    #[cfg(feature = "file-dialog")]
    pub fn pick_files(self) -> Option<Vec<SelectedFile>> {
        self.apply(rfd::FileDialog::new())
            .pick_files()
            .map(|paths| {
                paths
                    .into_iter()
                    .map(|p| SelectedFile { path: p })
                    .collect()
            })
    }

    #[cfg(not(feature = "file-dialog"))]
    pub fn pick_files(self) -> Option<Vec<SelectedFile>> {
        None
    }

    #[cfg(feature = "file-dialog")]
    pub fn pick_folder(self) -> Option<SelectedFile> {
        self.apply(rfd::FileDialog::new())
            .pick_folder()
            .map(|p| SelectedFile { path: p })
    }

    #[cfg(not(feature = "file-dialog"))]
    pub fn pick_folder(self) -> Option<SelectedFile> {
        None
    }

    #[cfg(feature = "file-dialog")]
    pub fn save_file(self) -> Option<SelectedFile> {
        self.apply(rfd::FileDialog::new())
            .save_file()
            .map(|p| SelectedFile { path: p })
    }

    #[cfg(not(feature = "file-dialog"))]
    pub fn save_file(self) -> Option<SelectedFile> {
        None
    }
}

impl Default for FileDialogBuilder {
    fn default() -> Self {
        Self::new()
    }
}
