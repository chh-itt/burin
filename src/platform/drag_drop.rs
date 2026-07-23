//! Drag-and-drop data types for OS file drag and drop events.

use crate::style::Point;
use std::path::PathBuf;

/// The payload of a drag-and-drop operation (OS file drop, etc.).
#[derive(Clone, Debug)]
pub struct DropData {
    pub position: Point,
    pub files: Vec<PathBuf>,
    pub text: Option<String>,
}
