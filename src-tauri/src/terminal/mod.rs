//! Terminal clients (Codex, Claude Code, OpenCode): per-client model sets, native config
//! generation, and client-scoped model resolution for the proxy.

mod apply;
mod changes;
mod clients;
mod direct;
mod plan;
mod profile;
mod resolve;

pub use crate::domain::terminal::{protocol, DirectProfile, Profile, Selection, CLIENTS};
pub use apply::{apply, apply_at, preview, preview_at, ApplyResult, Preview, PreviewFile};
pub use direct::{
    active as direct_active, apply as apply_direct, apply_at as apply_direct_at,
    preview as preview_direct, preview_at as preview_direct_at,
};
pub use plan::{config_path, endpoint, status, Status};
pub use profile::{load, save};
pub use resolve::{
    model_list, model_list_in, resolve, resolve_in, validate, validate_in, ResolvedModel,
};

#[cfg(test)]
mod tests;
