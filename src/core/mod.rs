//! Pure logic: no Win32, no libmpv. Everything here is unit-tested on any OS.
#![forbid(unsafe_code)]

pub mod app;
pub mod audio;
pub mod config;
pub mod coverage;
pub mod geometry;
pub mod media;
pub mod menu;
pub mod monitor;
pub mod pause;
pub mod power;
pub mod reconcile;
pub mod vsync;
pub mod zoom;
