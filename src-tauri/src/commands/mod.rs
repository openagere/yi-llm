//! Thin Tauri command layer: argument plumbing only. Behaviour lives in `service`.
//! Tauri injects `State` and deserialized arguments by value.
#![allow(clippy::needless_pass_by_value)]

pub mod logs;
pub mod models;
pub mod providers;
pub mod proxy;
pub mod settings;
pub mod terminal;
pub mod usage;
