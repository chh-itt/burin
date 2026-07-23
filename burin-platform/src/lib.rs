//! Safe wrappers around platform-specific FFI.
//!
//! This crate exists to isolate `unsafe` platform FFI calls behind safe
//! Rust APIs, so that downstream crates (notably `burin`) can keep
//! `#![forbid(unsafe_code)]`.

#![allow(unsafe_code)]

#[cfg(feature = "screensaver")]
pub mod screensaver;

#[cfg(feature = "display")]
pub mod display;

#[cfg(feature = "accessibility")]
pub mod accessibility;

#[cfg(feature = "screensaver")]
pub use screensaver::ScreensaverInhibit;

// macOS a11y unsafe block has been migrated to `accessibility` module.
