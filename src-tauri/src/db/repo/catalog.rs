use crate::{
    domain::standard_model::{validate_definition, StandardModel},
    error::{AppError, Result},
};
use rusqlite::{params, Connection, OptionalExtension};

fn row_to_standard(row: &rusqlite::Row<'_>) -> rusqlite::Result<StandardModel> {
    let raw: String = row.get("capabilities")?;
    Ok(StandardModel {
        id: row.get("id")?,
        name: row.get("name")?,
        protocol: row.get("protocol")?,
        brand: row.get("brand")?,
        capabilities: serde_json::from_str(&raw).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?,
        provider_count: row.get("provider_count")?,
    })
}

const SELECT: &str = "SELECT s.id,s.name,s.protocol,s.brand,s.capabilities,(SELECT COUNT(DISTINCT m.provider_id) FROM models m WHERE m.standard_model_id=s.id) AS provider_count FROM standard_models s";

pub fn list(conn: &Connection) -> Result<Vec<StandardModel>> {
    let mut stmt = conn.prepare(&format!(
        "{SELECT} ORDER BY s.name COLLATE NOCASE,s.protocol,s.id"
    ))?;
    let rows = stmt.query_map([], row_to_standard)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<StandardModel>> {
    Ok(conn
        .query_row(&format!("{SELECT} WHERE s.id=?1"), [id], row_to_standard)
        .optional()?)
}

pub fn save(conn: &mut Connection, model: &StandardModel) -> Result<()> {
    validate_definition(model)?;
    let tx = conn.transaction()?;
    let incompatible: u32 = tx.query_row("SELECT COUNT(*) FROM models m JOIN providers p ON p.id=m.provider_id WHERE m.standard_model_id=?1 AND p.type!=?2", params![model.id, model.protocol], |row| row.get(0))?;
    if incompatible > 0 {
        return Err(AppError::conflict(
            "该标准模型已被其他协议的 Provider 引用，不能修改协议",
        ));
    }
    let duplicate: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM standard_models WHERE name=?1 AND protocol=?2 AND id!=?3)",
        params![model.name.trim(), model.protocol, model.id],
        |r| r.get(0),
    )?;
    if duplicate {
        return Err(AppError::conflict("同一协议下的标准模型名称不能重复"));
    }
    tx.execute("INSERT INTO standard_models(id,name,protocol,capabilities,brand) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET name=excluded.name,protocol=excluded.protocol,capabilities=excluded.capabilities,brand=excluded.brand", params![model.id, model.name.trim(), model.protocol, serde_json::to_string(&model.capabilities)?, model.brand])?;
    tx.commit()?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    // The guarded statement also protects callers that do not enable SQLite FKs.
    conn.execute("DELETE FROM standard_models WHERE id=?1 AND NOT EXISTS(SELECT 1 FROM models WHERE standard_model_id=?1)", [id])?;
    if get(conn, id)?.is_some() {
        return Err(AppError::conflict(
            "该标准模型仍被 Provider 引用，请先解除关联",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{repo::providers, Db},
        domain::{
            capabilities::{Capabilities, Effort, Modality, Support},
            provider::{ModelMapping, ProtocolSupport, Provider, ProviderView},
        },
    };

    pub fn standard() -> StandardModel {
        StandardModel {
            id: "standard".into(),
            name: "Standard vision".into(),
            protocol: "responses".into(),
            brand: "openai".into(),
            provider_count: 0,
            capabilities: Capabilities {
                input_modalities: Some(vec![Modality::Text, Modality::Image]),
                output_modalities: Some(vec![Modality::Text]),
                context_window: Some(131072),
                max_output_tokens: Some(8192),
                effort: Effort {
                    support: Support::Supported,
                    levels: vec!["low".into(), "high".into()],
                    default: Some("high".into()),
                },
            },
        }
    }

    pub fn provider(id: &str) -> ProviderView {
        ProviderView {
            provider: Provider {
                id: id.into(),
                name: id.into(),
                short_code: id.into(),
                provider_type: "responses".into(),
                base_url: "https://example.com/v1".into(),
                api_key: String::new(),
                enabled: true,
                thinking: "medium".into(),
                extra: serde_json::json!({}),
                is_default: false,
                protocol_support: ProtocolSupport::native("responses"),
            },
            models: vec![ModelMapping {
                id: 0,
                provider_id: id.into(),
                exposed_name: format!("{id}/custom-name"),
                upstream_model: "custom-name".into(),
                standard_model_id: Some("standard".into()),
                capabilities: Capabilities::default(),
                non_standard: false,
            }],
        }
    }

    #[test]
    fn shared_capabilities_are_live_and_provider_names_remain_independent() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("test.db")).unwrap();
        let mut conn = db.conn().unwrap();
        let mut model = standard();
        save(&mut conn, &model).unwrap();
        for id in ["first", "second"] {
            providers::save(&mut conn, &provider(id)).unwrap();
        }
        assert_eq!(get(&conn, &model.id).unwrap().unwrap().provider_count, 2);
        model.name = "Renamed standard".into();
        model.capabilities.context_window = Some(262144);
        save(&mut conn, &model).unwrap();
        for id in ["first", "second"] {
            let binding = crate::db::repo::models::list_for_provider(&conn, id)
                .unwrap()
                .pop()
                .unwrap();
            assert_eq!(binding.upstream_model, "custom-name");
            assert_eq!(binding.capabilities.context_window, Some(262144));
            assert_eq!(
                binding.capabilities.input_modalities,
                model.capabilities.input_modalities
            );
        }
        assert!(delete(&conn, &model.id).is_err());
        model.protocol = "openai_chat".into();
        assert!(save(&mut conn, &model).is_err());
        assert_eq!(
            get(&conn, &model.id).unwrap().unwrap().protocol,
            "responses"
        );
        for id in ["first", "second"] {
            providers::delete(&conn, id).unwrap();
        }
        delete(&conn, &model.id).unwrap();
        assert!(list(&conn).unwrap().is_empty());
    }

    #[test]
    fn invalid_standard_and_incompatible_binding_do_not_change_saved_data() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("test.db")).unwrap();
        let mut conn = db.conn().unwrap();
        let mut model = standard();
        save(&mut conn, &model).unwrap();
        let mut p = provider("one");
        providers::save(&mut conn, &p).unwrap();
        model.capabilities.max_output_tokens = Some(262144);
        assert!(save(&mut conn, &model).is_err());
        assert_eq!(
            get(&conn, "standard")
                .unwrap()
                .unwrap()
                .capabilities
                .max_output_tokens,
            Some(8192)
        );
        p.provider.name = "Bad update".into();
        p.provider.provider_type = "anthropic".into();
        assert!(providers::save(&mut conn, &p).is_err());
        assert_eq!(providers::get(&conn, "one").unwrap().unwrap().name, "one");
        p.provider.provider_type = "responses".into();
        p.models[0].standard_model_id = Some("missing".into());
        assert!(providers::save(&mut conn, &p).is_err());
    }
}
