use crate::{
    db::repo::catalog as catalog_repo,
    domain::{
        capabilities::Capabilities,
        standard_model::{validate_definition, StandardModel},
    },
    error::{AppError, Result},
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::Write,
    path::{Path, PathBuf},
    time::SystemTime,
};

const VERSION: u32 = 1;
const EMBEDDED_CATALOG: &str = include_str!("../../../models/catalog.json");
const EMBEDDED_SCHEMA: &str = include_str!("../../../models/catalog.schema.json");

fn schema_path() -> String {
    "./catalog.schema.json".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub protocol: String,
    #[serde(default)]
    pub brand: String,
    #[serde(default)]
    pub capabilities: Capabilities,
}

impl From<&StandardModel> for CatalogEntry {
    fn from(model: &StandardModel) -> Self {
        Self {
            id: model.id.clone(),
            name: model.name.trim().into(),
            protocol: model.protocol.clone(),
            brand: model.brand.clone(),
            capabilities: model.capabilities.clone(),
        }
    }
}

impl CatalogEntry {
    fn standard(&self) -> StandardModel {
        StandardModel {
            id: self.id.clone(),
            name: self.name.clone(),
            protocol: self.protocol.clone(),
            brand: self.brand.clone(),
            capabilities: self.capabilities.clone(),
            provider_count: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogDocument {
    #[serde(rename = "$schema", default = "schema_path")]
    pub schema: String,
    pub version: u32,
    pub models: Vec<CatalogEntry>,
}

impl CatalogDocument {
    fn parse(bytes: &[u8]) -> Result<Self> {
        let mut document: Self = serde_json::from_slice(bytes)
            .map_err(|error| AppError::validation(format!("模型目录 JSON 无效：{error}")))?;
        document.normalize()?;
        Ok(document)
    }

    fn normalize(&mut self) -> Result<()> {
        if self.version != VERSION {
            return Err(AppError::validation(format!(
                "不支持模型目录版本 {}，当前支持版本 {VERSION}",
                self.version
            )));
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for model in &mut self.models {
            model.name = model.name.trim().into();
            validate_definition(&model.standard())
                .map_err(|error| AppError::validation(format!("模型 {}：{error}", model.name)))?;
            if !ids.insert(model.id.clone()) {
                return Err(AppError::validation(format!(
                    "标准模型 ID 重复：{}",
                    model.id
                )));
            }
            if !names.insert((model.name.clone(), model.protocol.clone())) {
                return Err(AppError::validation(format!(
                    "同一协议下的标准模型名称重复：{}",
                    model.name
                )));
            }
        }
        self.models.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.protocol.cmp(&b.protocol))
                .then(a.id.cmp(&b.id))
        });
        Ok(())
    }

    fn bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

#[derive(Serialize)]
pub struct CatalogInfo {
    pub path: String,
    pub version: u32,
}

/// Cheap change detector so request handlers avoid re-reading an unchanged file.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

fn stamp_of(path: &Path) -> Option<Stamp> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(Stamp {
        modified: metadata.modified().ok(),
        len: metadata.len(),
    })
}

pub struct FileCatalog {
    path: PathBuf,
    loaded: Option<Vec<u8>>,
    stamp: Option<Stamp>,
}

pub fn default_path(data_dir: &Path) -> Result<PathBuf> {
    // 兼容旧的 LLM_MAN_MODEL_CATALOG，优先使用新的 YI_LLM_MODEL_CATALOG。
    let configured = std::env::var_os("YI_LLM_MODEL_CATALOG")
        .or_else(|| std::env::var_os("LLM_MAN_MODEL_CATALOG"));
    if let Some(path) = configured {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(AppError::validation(
                "YI_LLM_MODEL_CATALOG 必须是绝对文件路径",
            ));
        }
        return Ok(path);
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| AppError::internal("无法确定模型目录所在的仓库路径"))?
        .join("models/catalog.json");
    if cfg!(debug_assertions) && workspace.parent().is_some_and(Path::is_dir) {
        Ok(workspace)
    } else {
        Ok(data_dir.join("models/catalog.json"))
    }
}

impl FileCatalog {
    pub fn open(path: PathBuf, conn: &Connection) -> Result<Self> {
        if !path.exists() {
            let models = catalog_repo::list(conn)?;
            let mut document = if models.is_empty() {
                CatalogDocument::parse(EMBEDDED_CATALOG.as_bytes())?
            } else {
                CatalogDocument {
                    schema: schema_path(),
                    version: VERSION,
                    models: models.iter().map(CatalogEntry::from).collect(),
                }
            };
            document.normalize()?;
            atomic_write(&path, &document.bytes()?)?;
            let schema = path.with_file_name("catalog.schema.json");
            if !schema.exists() {
                atomic_write(&schema, EMBEDDED_SCHEMA.as_bytes())?;
            }
        }
        Ok(Self {
            path,
            loaded: None,
            stamp: None,
        })
    }

    pub fn info(&self) -> CatalogInfo {
        CatalogInfo {
            path: self.path.to_string_lossy().into(),
            version: VERSION,
        }
    }

    /// True when the file differs from the last revision we inspected.
    ///
    /// Invalid revisions are remembered as well: callers keep using the last valid
    /// database cache instead of failing every read until the file changes again.
    pub fn is_stale(&self) -> bool {
        stamp_of(&self.path) != self.stamp
    }

    /// Applies the file to the database cache. Returns whether the cache content changed.
    pub fn sync(&mut self, conn: &mut Connection) -> Result<bool> {
        let stamp = stamp_of(&self.path);
        let bytes = std::fs::read(&self.path).map_err(|error| {
            AppError::Io(format!(
                "读取模型目录 {} 失败：{error}",
                self.path.display()
            ))
        })?;
        // Record the revision before validation. If it is invalid, startup can retain
        // the last valid database cache and retry only after the file changes.
        self.stamp = stamp;
        if self.loaded.as_ref() == Some(&bytes) {
            return Ok(false);
        }
        let document = CatalogDocument::parse(&bytes)?;
        let tx = conn.transaction()?;
        apply_document(&tx, &document)?;
        tx.commit()?;
        self.loaded = Some(bytes);
        self.stamp = stamp;
        Ok(true)
    }

    pub fn save(&mut self, conn: &mut Connection, model: &StandardModel) -> Result<()> {
        self.save_many(conn, std::slice::from_ref(model))
    }

    /// Adds or replaces several models with a single catalog write, so importing a
    /// configuration rewrites the shared catalog file only once.
    pub fn save_many(&mut self, conn: &mut Connection, models: &[StandardModel]) -> Result<()> {
        if models.is_empty() {
            return Ok(());
        }
        self.sync(conn)?;
        let mut document = self.loaded_document()?;
        for model in models {
            if let Some(existing) = document
                .models
                .iter_mut()
                .find(|entry| entry.id == model.id)
            {
                *existing = CatalogEntry::from(model);
            } else {
                document.models.push(CatalogEntry::from(model));
            }
        }
        self.persist(conn, document)
    }

    /// Applies catalog additions and dependent database writes in one transaction.
    /// The file is replaced only after every database operation has succeeded.
    pub fn save_many_with<F>(
        &mut self,
        conn: &mut Connection,
        models: &[StandardModel],
        write: F,
    ) -> Result<()>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<()>,
    {
        self.sync(conn)?;
        let mut document = self.loaded_document()?;
        for model in models {
            document.models.push(CatalogEntry::from(model));
        }
        self.persist_with(conn, document, write)
    }

    pub fn delete(&mut self, conn: &mut Connection, id: &str) -> Result<()> {
        self.sync(conn)?;
        let mut document = self.loaded_document()?;
        document.models.retain(|model| model.id != id);
        self.persist(conn, document)
    }

    fn loaded_document(&self) -> Result<CatalogDocument> {
        let bytes = self
            .loaded
            .as_ref()
            .ok_or_else(|| AppError::internal("模型目录尚未读取"))?;
        CatalogDocument::parse(bytes)
    }

    fn persist(&mut self, conn: &mut Connection, document: CatalogDocument) -> Result<()> {
        self.persist_with(conn, document, |_| Ok(()))
    }

    fn persist_with<F>(
        &mut self,
        conn: &mut Connection,
        mut document: CatalogDocument,
        write: F,
    ) -> Result<()>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<()>,
    {
        document.normalize()?;
        let old = self
            .loaded
            .as_ref()
            .ok_or_else(|| AppError::internal("模型目录尚未读取"))?;
        let bytes = document.bytes()?;
        let tx = conn.transaction()?;
        apply_document(&tx, &document)?;
        write(&tx)?;
        // Detect external edits before replacing the shared source file.
        if std::fs::read(&self.path)? != *old {
            return Err(AppError::conflict(
                "模型目录已被其他程序修改，请刷新后重新保存",
            ));
        }
        atomic_write(&self.path, &bytes)?;
        if let Err(error) = tx.commit() {
            atomic_write(&self.path, old).map_err(|restore| {
                AppError::Io(format!("缓存提交失败：{error}；目录恢复失败：{restore}"))
            })?;
            return Err(AppError::Database(format!(
                "缓存提交失败，模型目录已恢复：{error}"
            )));
        }
        self.stamp = stamp_of(&self.path);
        self.loaded = Some(bytes);
        Ok(())
    }
}

fn apply_document(conn: &Connection, document: &CatalogDocument) -> Result<()> {
    let references = {
        let mut stmt = conn.prepare("SELECT m.standard_model_id,p.type,p.name FROM models m JOIN providers p ON p.id=m.provider_id WHERE m.standard_model_id IS NOT NULL")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (id, protocol, provider) in references {
        let model = document
            .models
            .iter()
            .find(|model| model.id == id)
            .ok_or_else(|| {
                AppError::conflict(format!(
                    "标准模型 {id} 仍被 Provider「{provider}」引用，不能从目录删除"
                ))
            })?;
        if model.protocol != protocol {
            return Err(AppError::conflict(format!(
                "标准模型「{}」与 Provider「{provider}」的 {protocol} 协议不匹配",
                model.name
            )));
        }
    }
    let existing = catalog_repo::list(conn)?;
    // Temporary unique names allow a file revision to swap model display names.
    conn.execute("UPDATE standard_models SET name=CHAR(0)||id", [])?;
    for model in &document.models {
        let capabilities = serde_json::to_string(&model.capabilities)?;
        conn.execute("INSERT INTO standard_models(id,name,protocol,brand,capabilities) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET name=excluded.name,protocol=excluded.protocol,brand=excluded.brand,capabilities=excluded.capabilities", params![model.id, model.name, model.protocol, model.brand, capabilities])?;
    }
    for model in existing {
        if !document.models.iter().any(|entry| entry.id == model.id) {
            conn.execute("DELETE FROM standard_models WHERE id=?1", [model.id])?;
        }
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::internal("模型目录路径没有父目录"))?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)
        .map_err(|error| AppError::Io(format!("保存模型目录 {} 失败：{error}", path.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{
            repo::{models, providers},
            Conn, Db,
        },
        domain::provider::{ModelMapping, ProtocolSupport, Provider, ProviderView},
    };

    fn standard() -> StandardModel {
        StandardModel {
            id: "shared-model".into(),
            name: "Standard model".into(),
            protocol: "responses".into(),
            brand: "openai".into(),
            provider_count: 0,
            capabilities: Capabilities {
                context_window: Some(262144),
                ..Default::default()
            },
        }
    }

    fn setup() -> (tempfile::TempDir, Conn, FileCatalog) {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = Db::open(&dir.path().join("cache.db"))
            .unwrap()
            .conn()
            .unwrap();
        catalog_repo::save(&mut conn, &standard()).unwrap();
        let mut store = FileCatalog::open(dir.path().join("models/catalog.json"), &conn).unwrap();
        store.sync(&mut conn).unwrap();
        (dir, conn, store)
    }

    fn attach_provider(conn: &mut Connection) {
        providers::save(
            conn,
            &ProviderView {
                provider: Provider {
                    id: "account".into(),
                    name: "Work account".into(),
                    short_code: "work".into(),
                    provider_type: "responses".into(),
                    base_url: "https://private.example/v1".into(),
                    api_key: "test-secret".into(),
                    enabled: true,
                    thinking: "medium".into(),
                    extra: serde_json::json!({}),
                    is_default: false,
                    protocol_support: ProtocolSupport::native("responses"),
                },
                models: vec![ModelMapping {
                    id: 0,
                    provider_id: "account".into(),
                    exposed_name: "account/custom-name".into(),
                    upstream_model: "custom-name".into(),
                    standard_model_id: Some("shared-model".into()),
                    capabilities: Default::default(),
                    non_standard: false,
                }],
            },
        )
        .unwrap();
    }

    fn document(store: &FileCatalog) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(&store.path).unwrap()).unwrap()
    }

    #[test]
    fn existing_models_are_exported_without_local_account_data() {
        let (dir, mut conn, _) = setup();
        attach_provider(&mut conn);
        let path = dir.path().join("export/catalog.json");
        FileCatalog::open(path.clone(), &conn).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        for private in [
            "test-secret",
            "private.example",
            "account/custom-name",
            "provider_count",
        ] {
            assert!(!raw.contains(private));
        }
        let parsed = CatalogDocument::parse(raw.as_bytes()).unwrap();
        assert_eq!(parsed.models.len(), 1);
        assert_eq!(parsed.models[0].id, "shared-model");
        assert!(path.with_file_name("catalog.schema.json").exists());
    }

    #[test]
    fn fresh_install_uses_the_embedded_shared_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = Db::open(&dir.path().join("cache.db"))
            .unwrap()
            .conn()
            .unwrap();
        let mut store = FileCatalog::open(dir.path().join("catalog.json"), &conn).unwrap();
        store.sync(&mut conn).unwrap();
        assert_eq!(
            catalog_repo::list(&conn).unwrap().len(),
            CatalogDocument::parse(EMBEDDED_CATALOG.as_bytes())
                .unwrap()
                .models
                .len()
        );
    }

    #[test]
    fn unchanged_file_is_not_stale_and_edits_are_detected() {
        let (_dir, mut conn, mut store) = setup();
        assert!(!store.is_stale());
        assert!(!store.sync(&mut conn).unwrap());
        let mut updated = document(&store);
        updated["models"][0]["capabilities"]["context_window"] = 1048576.into();
        std::fs::write(&store.path, serde_json::to_vec(&updated).unwrap()).unwrap();
        assert!(store.is_stale());
        assert!(store.sync(&mut conn).unwrap());
        assert!(!store.is_stale());
    }

    #[test]
    fn external_file_edits_update_referenced_capabilities_and_allow_name_swaps() {
        let (_dir, mut conn, mut store) = setup();
        attach_provider(&mut conn);
        let mut second = standard();
        second.id = "second".into();
        second.name = "Second model".into();
        store.save(&mut conn, &second).unwrap();
        let mut updated = document(&store);
        for entry in updated["models"].as_array_mut().unwrap() {
            if entry["id"] == "shared-model" {
                entry["name"] = "Second model".into();
                entry["capabilities"]["context_window"] = 1048576.into();
            } else {
                entry["name"] = "Standard model".into();
            }
        }
        std::fs::write(&store.path, serde_json::to_vec(&updated).unwrap()).unwrap();
        store.sync(&mut conn).unwrap();
        let mapping = models::list_for_provider(&conn, "account")
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(mapping.upstream_model, "custom-name");
        assert_eq!(mapping.capabilities.context_window, Some(1048576));
        assert_eq!(
            catalog_repo::get(&conn, "shared-model")
                .unwrap()
                .unwrap()
                .name,
            "Second model"
        );
    }

    #[test]
    fn invalid_file_revisions_preserve_cache_and_recover_after_correction() {
        let (_dir, mut conn, mut store) = setup();
        let original = std::fs::read(&store.path).unwrap();
        let valid = document(&store);
        let mut bad_version = valid.clone();
        bad_version["version"] = 2.into();
        let mut duplicate = valid.clone();
        duplicate["models"]
            .as_array_mut()
            .unwrap()
            .push(valid["models"][0].clone());
        let mut duplicate_name = valid.clone();
        let mut other = valid["models"][0].clone();
        other["id"] = "other".into();
        duplicate_name["models"].as_array_mut().unwrap().push(other);
        let mut typo = valid.clone();
        typo["models"][0]["capabilities"]["context_size"] = 100.into();
        let mut invalid_tokens = valid.clone();
        invalid_tokens["models"][0]["capabilities"]["context_window"] = 0.into();
        let mut invalid_effort = valid;
        invalid_effort["models"][0]["capabilities"]["effort"] =
            serde_json::json!({"support":"supported","levels":["minimal"]});
        for invalid in [
            b"{invalid".to_vec(),
            serde_json::to_vec(&bad_version).unwrap(),
            serde_json::to_vec(&duplicate).unwrap(),
            serde_json::to_vec(&duplicate_name).unwrap(),
            serde_json::to_vec(&typo).unwrap(),
            serde_json::to_vec(&invalid_tokens).unwrap(),
            serde_json::to_vec(&invalid_effort).unwrap(),
        ] {
            std::fs::write(&store.path, &invalid).unwrap();
            assert!(store.sync(&mut conn).is_err());
            assert_eq!(
                catalog_repo::get(&conn, "shared-model").unwrap().unwrap(),
                standard()
            );
        }
        std::fs::write(&store.path, original).unwrap();
        store.sync(&mut conn).unwrap();
        store.save(&mut conn, &standard()).unwrap();
    }

    #[test]
    fn crud_persists_source_and_cache_and_rejects_reference_conflicts() {
        let (_dir, mut conn, mut store) = setup();
        attach_provider(&mut conn);
        let before = std::fs::read(&store.path).unwrap();
        assert!(store.delete(&mut conn, "shared-model").is_err());
        let mut incompatible = standard();
        incompatible.protocol = "anthropic".into();
        assert!(store.save(&mut conn, &incompatible).is_err());
        assert_eq!(std::fs::read(&store.path).unwrap(), before);
        let mut external = document(&store);
        external["models"] = serde_json::json!([]);
        std::fs::write(&store.path, serde_json::to_vec(&external).unwrap()).unwrap();
        assert!(store.sync(&mut conn).is_err());
        assert!(!store.is_stale());
        assert_eq!(
            catalog_repo::get(&conn, "shared-model")
                .unwrap()
                .unwrap()
                .provider_count,
            1
        );
        std::fs::write(&store.path, before).unwrap();
        let mut changed = standard();
        changed.name = "Renamed model".into();
        store.save(&mut conn, &changed).unwrap();
        assert_eq!(document(&store)["models"][0]["name"], "Renamed model");
        let mut reopened = FileCatalog::open(store.path.clone(), &conn).unwrap();
        reopened.sync(&mut conn).unwrap();
        assert_eq!(
            catalog_repo::get(&conn, "shared-model")
                .unwrap()
                .unwrap()
                .name,
            "Renamed model"
        );
        providers::delete(&conn, "account").unwrap();
        reopened.delete(&mut conn, "shared-model").unwrap();
        assert!(catalog_repo::list(&conn).unwrap().is_empty());
        assert!(document(&reopened)["models"].as_array().unwrap().is_empty());
    }

    #[test]
    fn concurrent_file_revision_is_not_overwritten() {
        let (_dir, mut conn, mut store) = setup();
        let mut pending = store.loaded_document().unwrap();
        pending.models[0].name = "Pending rename".into();
        let mut external = document(&store);
        external["models"][0]["name"] = "External rename".into();
        let bytes = serde_json::to_vec(&external).unwrap();
        std::fs::write(&store.path, &bytes).unwrap();
        assert!(store
            .persist(&mut conn, pending)
            .unwrap_err()
            .to_string()
            .contains("其他程序修改"));
        assert_eq!(std::fs::read(&store.path).unwrap(), bytes);
        assert_eq!(
            catalog_repo::get(&conn, "shared-model")
                .unwrap()
                .unwrap()
                .name,
            "Standard model"
        );
        store.sync(&mut conn).unwrap();
        assert_eq!(
            catalog_repo::get(&conn, "shared-model")
                .unwrap()
                .unwrap()
                .name,
            "External rename"
        );
    }

    #[test]
    fn cache_commit_failure_restores_the_catalog_file() {
        let (_dir, mut conn, mut store) = setup();
        let before = std::fs::read(&store.path).unwrap();
        conn.execute_batch("CREATE TABLE test_parent(id INTEGER PRIMARY KEY);
            CREATE TABLE test_child(id INTEGER REFERENCES test_parent(id) DEFERRABLE INITIALLY DEFERRED);
            CREATE TRIGGER fail_catalog_commit AFTER INSERT ON standard_models BEGIN INSERT INTO test_child VALUES(1); END;").unwrap();
        let mut added = standard();
        added.id = "new-model".into();
        added.name = "New model".into();
        assert!(store
            .save(&mut conn, &added)
            .unwrap_err()
            .to_string()
            .contains("目录已恢复"));
        assert_eq!(std::fs::read(&store.path).unwrap(), before);
        assert!(catalog_repo::get(&conn, "new-model").unwrap().is_none());
        assert_eq!(
            catalog_repo::get(&conn, "shared-model").unwrap().unwrap(),
            standard()
        );
    }
}
