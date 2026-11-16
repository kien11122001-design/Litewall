//! libmpv playback. `unsafe` is confined to `ffi.rs`; `player.rs` is safe Rust.
pub mod ffi;
pub mod player;
