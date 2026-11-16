//! LiteWall library crate.
//!
//! `core` is pure Rust (no Win32, no libmpv) and is unit-tested on any OS.
//! `logger` is std-only. `platform` (Windows-only) holds all Win32 `unsafe`;
//! `mpv` (Windows-only) holds the libmpv FFI (`unsafe` only in `mpv/ffi.rs`);
//! `runtime` (Windows-only, safe Rust) ties windows and players to the config.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(windows)]
pub mod controller;
pub mod core;
pub mod logger;
#[cfg(windows)]
pub mod mpv;
#[cfg(windows)]
pub mod platform;
#[cfg(windows)]
pub mod runtime;
