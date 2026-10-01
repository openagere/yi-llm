use crate::{
    app::AppState,
    catalog::file::CatalogInfo,
    db::repo::catalog,
    domain::standard_model::StandardModel,
    error::{AppError, Result},
};

pub async fn list(state: &AppState) -> Result<Vec<StandardModel>> {
    state.refresh_catalog().await?;
    state.db.run(|conn| catalog::list(conn)).await
}

pub fn info(state: &AppState) -> Result<CatalogInfo> {
    state
        .catalog
        .as_ref()
        .ok_or_else(|| AppError::not_found("未配置模型目录文件"))?
        .info()
}

pub async fn save(state: &AppState, model: StandardModel) -> Result<()> {
    let service = state.catalog.clone();
    let result = state
        .db
        .run(move |conn| match &service {
            Some(service) => service.save(conn, &model),
            None => catalog::save(conn, &model),
        })
        .await;
    state.invalidate_routes();
    result
}

pub async fn delete(state: &AppState, id: String) -> Result<()> {
    let service = state.catalog.clone();
    let result = state
        .db
        .run(move |conn| match &service {
            Some(service) => service.delete(conn, &id),
            None => catalog::delete(conn, &id),
        })
        .await;
    state.invalidate_routes();
    result
}
