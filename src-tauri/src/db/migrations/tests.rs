use super::*;
use crate::{
    db::{
        repo::{models, providers, terminal_profiles, usage},
        Db,
    },
    domain::{
        capabilities::Capabilities,
        provider::{ModelMapping, ProtocolSupport, Provider, ProviderView},
        provider_names::model_name,
        terminal::{Profile, Selection},
        usage::NewUsage,
    },
};

fn sample_usage(model: &str) -> NewUsage {
    NewUsage {
        provider_name: "Work account".into(),
        protocol: "responses".into(),
        model: model.into(),
        input_tokens: 2,
        output_tokens: 3,
        cached_tokens: 0,
        reasoning_tokens: 0,
        requested_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
    }
}

fn provider(id: &str, code: &str) -> ProviderView {
    ProviderView {
        provider: Provider {
            id: id.into(),
            short_code: code.into(),
            name: "Work account".into(),
            provider_type: "responses".into(),
            base_url: "https://example.com/v1".into(),
            api_key: String::new(),
            enabled: true,
            thinking: "medium".into(),
            extra: serde_json::json!({}),
            is_default: false,
            protocol_support: ProtocolSupport {
                responses: true,
                anthropic: true,
                openai_chat: true,
            },
        },
        models: vec![ModelMapping {
            id: 0,
            provider_id: id.into(),
            upstream_model: "gpt-6-luna".into(),
            exposed_name: model_name("gpt-6-luna", code),
            standard_model_id: None,
            capabilities: Capabilities::default(),
            non_standard: false,
        }],
    }
}

#[test]
fn fresh_database_is_stamped_with_the_latest_version_and_reopens_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fresh.db");
    let db = Db::open(&path).unwrap();
    assert_eq!(
        current_version(&db.conn().unwrap()).unwrap(),
        latest_version()
    );
    drop(db);
    let db = Db::open(&path).unwrap();
    assert_eq!(
        current_version(&db.conn().unwrap()).unwrap(),
        latest_version()
    );
}

#[test]
fn version_one_database_gains_model_flag_without_losing_mappings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("version-one.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(super::baseline::SCHEMA).unwrap();
    conn.execute("INSERT INTO providers(id,type,name,short_code,base_url) VALUES('one','responses','One','one','https://example.com')", []).unwrap();
    conn.execute("INSERT INTO models(provider_id,exposed_name,upstream_model) VALUES('one','model(one)','model')", []).unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    drop(conn);
    let db = Db::open(&path).unwrap();
    let conn = db.conn().unwrap();
    assert_eq!(current_version(&conn).unwrap(), latest_version());
    let mappings = models::list_for_provider(&conn, "one").unwrap();
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].upstream_model, "model");
    assert!(!mappings[0].non_standard);
}

#[test]
fn unversioned_settings_table_gains_log_level_without_losing_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("existing.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE settings (id INTEGER PRIMARY KEY CHECK (id = 1), host TEXT NOT NULL, port INTEGER NOT NULL); INSERT INTO settings VALUES (1, '10.0.0.5', 9000);").unwrap();
    assert_eq!(current_version(&conn).unwrap(), 0);
    drop(conn);
    let db = Db::open(&path).unwrap();
    let conn = db.conn().unwrap();
    let settings = crate::db::repo::settings::get(&conn).unwrap();
    assert_eq!(
        (
            settings.host.as_str(),
            settings.port,
            settings.log_level.as_str()
        ),
        ("10.0.0.5", 9000, "info")
    );
    assert_eq!(current_version(&conn).unwrap(), latest_version());
}

#[test]
fn pre_catalog_data_migrates_once_to_standard_references_and_canonical_routes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE providers(id TEXT PRIMARY KEY,type TEXT NOT NULL,name TEXT NOT NULL,base_url TEXT NOT NULL,api_key TEXT NOT NULL DEFAULT '',enabled INTEGER NOT NULL DEFAULT 1,thinking TEXT NOT NULL DEFAULT 'medium',extra TEXT NOT NULL DEFAULT '{}',is_default INTEGER NOT NULL DEFAULT 0,sort_order INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE models(id INTEGER PRIMARY KEY,provider_id TEXT NOT NULL,exposed_name TEXT NOT NULL,upstream_model TEXT NOT NULL);
    CREATE TABLE terminal_profiles(client TEXT PRIMARY KEY,profile TEXT NOT NULL);
    INSERT INTO providers(id,type,name,base_url) VALUES('one','responses','One','https://example.com'),('two','responses','Two','https://example.com');
    INSERT INTO models VALUES(1,'one','old-alias','actual-model'),(2,'two','other-alias','actual-model');
    INSERT INTO terminal_profiles VALUES('codex','{\"client\":\"codex\",\"models\":[{\"provider_id\":\"one\",\"model\":\"old-alias\"}],\"default_model\":\"old-alias\"}');").unwrap();
    drop(conn);
    for _ in 0..2 {
        let db = Db::open(&path).unwrap();
        let conn = db.conn().unwrap();
        let standard = crate::db::repo::catalog::list(&conn).unwrap();
        assert_eq!(standard.len(), 1);
        assert_eq!(standard[0].provider_count, 2);
        assert_eq!(standard[0].capabilities, Capabilities::default());
        assert_eq!(
            models::list_for_provider(&conn, "one").unwrap()[0].exposed_name,
            "actual-model(one)"
        );
        let stale: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM models WHERE exposed_name='old-alias'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stale, 0);
        let profile = terminal_profiles::get(&conn, "codex").unwrap().unwrap();
        assert_eq!(profile.default_model, "actual-model(one)");
        assert_eq!(profile.models[0].model, "actual-model(one)");
    }
}

#[test]
fn providers_without_codes_get_distinct_codes_and_references_follow() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let db = Db::open(&path).unwrap();
    {
        let mut conn = db.conn().unwrap();
        for id in ["one", "two"] {
            providers::save(&mut conn, &provider(id, id)).unwrap();
        }
        terminal_profiles::upsert(
            &conn,
            &Profile {
                client: "codex".into(),
                models: vec![Selection {
                    provider_id: "one".into(),
                    model: "gpt-6-luna(one)".into(),
                }],
                default_model: "gpt-6-luna(one)".into(),
            },
        )
        .unwrap();
        usage::record(&conn, &sample_usage("gpt-6-luna(one)")).unwrap();
        conn.execute("DROP INDEX idx_providers_short_code", [])
            .unwrap();
        conn.execute("ALTER TABLE providers DROP COLUMN short_code", [])
            .unwrap();
        conn.pragma_update(None, "user_version", 0).unwrap();
    }
    drop(db);
    for _ in 0..2 {
        let db = Db::open(&path).unwrap();
        let conn = db.conn().unwrap();
        assert_eq!(
            providers::get(&conn, "one").unwrap().unwrap().short_code,
            "work-account"
        );
        assert_eq!(
            providers::get(&conn, "two").unwrap().unwrap().short_code,
            "work-account-2"
        );
        assert_eq!(
            terminal_profiles::get(&conn, "codex")
                .unwrap()
                .unwrap()
                .default_model,
            "gpt-6-luna(work-account)"
        );
        assert_eq!(
            usage::snapshot_for_days(&conn, 1, None).unwrap().models[0].model,
            "gpt-6-luna(work-account)"
        );
    }
}
