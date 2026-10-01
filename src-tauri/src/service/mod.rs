//! Application services: business operations shared by Tauri commands. Each function
//! takes the [`AppState`](crate::app::AppState), runs blocking work off the async runtime
//! and keeps derived caches (routes, catalog) consistent.

pub mod catalog;
pub mod logs;
pub mod providers;
pub mod proxy;
pub mod settings;
pub mod terminal;
pub mod usage;
