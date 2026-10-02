use crate::{
    app::AppState,
    domain::terminal::{DirectProfile, Profile},
    error::Result,
    terminal::{self, ApplyResult, Preview, Status},
};

pub async fn list_status(state: &AppState) -> Result<Vec<Status>> {
    state.refresh_catalog().await?;
    state
        .db
        .run(|conn| {
            terminal::CLIENTS
                .into_iter()
                .map(|client| terminal::status(conn, client))
                .collect()
        })
        .await
}

pub async fn preview(state: &AppState, profile: Profile) -> Result<Preview> {
    state.refresh_catalog().await?;
    state
        .db
        .run(move |conn| terminal::preview(conn, &profile))
        .await
}

pub async fn preview_direct(state: &AppState, profile: DirectProfile) -> Result<Preview> {
    state
        .db
        .run(move |conn| terminal::preview_direct(conn, &profile))
        .await
}

pub async fn apply_direct(state: &AppState, profile: DirectProfile) -> Result<ApplyResult> {
    let result = state
        .db
        .run(move |conn| terminal::apply_direct(conn, &profile))
        .await;
    result
}

pub async fn apply(state: &AppState, profile: Profile) -> Result<ApplyResult> {
    state.refresh_catalog().await?;
    let result = state
        .db
        .run(move |conn| terminal::apply(conn, &profile))
        .await;
    state.invalidate_routes();
    result
}
