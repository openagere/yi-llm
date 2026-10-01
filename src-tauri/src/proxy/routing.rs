use crate::{
    db::{
        repo::{models, providers, terminal_profiles},
        Db,
    },
    domain::{
        capabilities::Capabilities,
        provider::{ModelMapping, Provider},
        terminal::Profile,
    },
    error::Result,
};
use arc_swap::ArcSwapOption;
use rusqlite::Connection;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

/// Where a request for a public model name should be sent.
#[derive(Debug, Clone)]
pub struct ResolvedRoute {
    pub provider: Provider,
    pub upstream_model: String,
    pub capabilities: Capabilities,
    pub non_standard: bool,
    /// Name recorded in usage history and returned to the client.
    pub usage_model: String,
}

/// Immutable in-memory snapshot of everything routing needs. Built with a handful of
/// queries and shared by all requests until configuration changes.
#[derive(Debug, Default)]
pub struct RouteTable {
    providers: Vec<Provider>,
    provider_index: HashMap<String, usize>,
    models: HashMap<String, Vec<ModelMapping>>,
    exposed: HashMap<String, (usize, usize)>,
    default_provider: Option<usize>,
    profiles: HashMap<String, Profile>,
}

impl RouteTable {
    pub fn load(conn: &Connection) -> Result<Self> {
        let providers = providers::list(conn)?;
        let mut models_by_provider: HashMap<String, Vec<ModelMapping>> = HashMap::new();
        for model in models::list_all(conn)? {
            models_by_provider
                .entry(model.provider_id.clone())
                .or_default()
                .push(model);
        }
        let profiles = terminal_profiles::list_lenient(conn)?
            .into_iter()
            .map(|profile| (profile.client.clone(), profile))
            .collect();
        Ok(Self::build(providers, models_by_provider, profiles))
    }

    fn build(
        providers: Vec<Provider>,
        models: HashMap<String, Vec<ModelMapping>>,
        profiles: HashMap<String, Profile>,
    ) -> Self {
        let provider_index = providers
            .iter()
            .enumerate()
            .map(|(index, provider)| (provider.id.clone(), index))
            .collect();
        let mut exposed = HashMap::new();
        for (provider_position, provider) in providers.iter().enumerate() {
            for (model_position, model) in
                models.get(&provider.id).into_iter().flatten().enumerate()
            {
                exposed.insert(
                    model.exposed_name.clone(),
                    (provider_position, model_position),
                );
            }
        }
        let default_provider = providers.iter().position(|provider| provider.is_default);
        Self {
            providers,
            provider_index,
            models,
            exposed,
            default_provider,
            profiles,
        }
    }

    pub fn provider(&self, id: &str) -> Option<&Provider> {
        self.provider_index
            .get(id)
            .map(|index| &self.providers[*index])
    }

    pub fn profile(&self, client: &str) -> Option<&Profile> {
        self.profiles.get(client)
    }

    /// A provider's mapping by exposed name, falling back to its upstream model name.
    pub fn find_model(&self, provider_id: &str, name: &str) -> Option<&ModelMapping> {
        let models = self.models.get(provider_id)?;
        models
            .iter()
            .find(|model| model.exposed_name == name)
            .or_else(|| models.iter().find(|model| model.upstream_model == name))
    }

    /// Resolves a public model name for a client protocol. A model mapping wins;
    /// otherwise the default provider receives the name unchanged.
    pub fn resolve_public(&self, requested: &str, protocol: &str) -> Option<ResolvedRoute> {
        if let Some((provider_position, model_position)) = self.exposed.get(requested) {
            let provider = &self.providers[*provider_position];
            if !provider.protocol_support.supports(protocol) {
                return None;
            }
            let model = &self.models[&provider.id][*model_position];
            return Some(ResolvedRoute {
                provider: provider.clone(),
                upstream_model: model.upstream_model.clone(),
                capabilities: model.capabilities.clone(),
                non_standard: model.non_standard,
                usage_model: requested.to_owned(),
            });
        }
        let provider = self
            .default_provider
            .map(|position| &self.providers[position])
            .filter(|provider| provider.protocol_support.supports(protocol))?;
        let model = self.find_model(&provider.id, requested);
        let capabilities = model
            .map(|model| model.capabilities.clone())
            .unwrap_or_default();
        Some(ResolvedRoute {
            provider: provider.clone(),
            upstream_model: requested.to_owned(),
            capabilities,
            non_standard: model.is_some_and(|model| model.non_standard),
            usage_model: requested.to_owned(),
        })
    }

    /// Enabled providers' models for `/v1/models`: `(exposed name, provider name)`.
    pub fn enabled_models(&self) -> impl Iterator<Item = (&str, &str)> {
        self.providers
            .iter()
            .filter(|provider| provider.enabled)
            .flat_map(|provider| {
                self.models
                    .get(&provider.id)
                    .into_iter()
                    .flatten()
                    .map(move |model| (model.exposed_name.as_str(), provider.name.as_str()))
            })
    }
}

/// Lazily rebuilt, lock-free-read holder of the current [`RouteTable`].
#[derive(Default)]
pub struct RouteCache {
    table: ArcSwapOption<RouteTable>,
    generation: AtomicU64,
}

impl RouteCache {
    /// Drops the snapshot; the next request rebuilds it from the database.
    pub fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.table.store(None);
    }

    pub fn is_warm(&self) -> bool {
        self.table.load().is_some()
    }

    pub async fn get(&self, db: &Db) -> Result<Arc<RouteTable>> {
        if let Some(table) = self.table.load_full() {
            return Ok(table);
        }
        let generation = self.generation.load(Ordering::SeqCst);
        let table = Arc::new(db.run(|conn| RouteTable::load(conn)).await?);
        // A concurrent invalidation means this snapshot may already be stale for later
        // requests, so only publish it when nothing changed while it was being built.
        if self.generation.load(Ordering::SeqCst) == generation {
            self.table.store(Some(table.clone()));
        }
        Ok(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::provider::{ProtocolSupport, ProviderView};

    fn provider(id: &str, is_default: bool, protocol: &str) -> ProviderView {
        ProviderView {
            provider: Provider {
                id: id.into(),
                provider_type: protocol.into(),
                name: id.into(),
                short_code: id.into(),
                base_url: "https://example.com".into(),
                api_key: String::new(),
                enabled: true,
                thinking: "high".into(),
                extra: serde_json::json!({}),
                is_default,
                protocol_support: ProtocolSupport::native(protocol),
            },
            models: vec![ModelMapping {
                id: 0,
                provider_id: id.into(),
                exposed_name: format!("mapped-{id}"),
                upstream_model: format!("real-{id}"),
                standard_model_id: None,
                capabilities: Capabilities::default(),
                non_standard: false,
            }],
        }
    }

    fn table() -> (tempfile::TempDir, Db, RouteTable) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("test.db")).unwrap();
        {
            let mut conn = db.conn().unwrap();
            providers::save(&mut conn, &provider("p1", true, "anthropic")).unwrap();
            providers::save(&mut conn, &provider("p2", false, "responses")).unwrap();
        }
        let table = RouteTable::load(&db.conn().unwrap()).unwrap();
        (dir, db, table)
    }

    #[test]
    fn prefers_mapping_then_default_provider() {
        let (_dir, _db, table) = table();
        let route = table.resolve_public("mapped-p1", "anthropic").unwrap();
        assert_eq!(
            (route.provider.id.as_str(), route.upstream_model.as_str()),
            ("p1", "real-p1")
        );
        let route = table.resolve_public("anything-else", "anthropic").unwrap();
        assert_eq!(route.upstream_model, "anything-else");
        assert_eq!(route.provider.id, "p1");
    }

    #[test]
    fn mapping_for_an_unsupported_protocol_never_falls_back_to_default() {
        let (_dir, _db, table) = table();
        assert!(table.resolve_public("mapped-p2", "anthropic").is_none());
        assert!(table.resolve_public("unknown", "responses").is_none());
        assert!(table.resolve_public("mapped-p2", "responses").is_some());
    }

    #[test]
    fn default_provider_matches_upstream_names_for_capabilities() {
        let (_dir, _db, table) = table();
        assert!(table.find_model("p1", "real-p1").is_some());
        assert!(table.find_model("p1", "mapped-p1").is_some());
        assert!(table.find_model("p1", "missing").is_none());
        assert_eq!(table.enabled_models().count(), 2);
    }

    #[tokio::test]
    async fn cache_rebuilds_after_invalidation() {
        let (_dir, db, _) = table();
        let cache = RouteCache::default();
        assert!(!cache.is_warm());
        let first = cache.get(&db).await.unwrap();
        assert!(cache.is_warm());
        assert!(Arc::ptr_eq(&first, &cache.get(&db).await.unwrap()));
        db.conn()
            .unwrap()
            .execute("UPDATE providers SET enabled=0 WHERE id='p1'", [])
            .unwrap();
        assert!(
            cache
                .get(&db)
                .await
                .unwrap()
                .provider("p1")
                .unwrap()
                .enabled
        );
        cache.invalidate();
        assert!(
            !cache
                .get(&db)
                .await
                .unwrap()
                .provider("p1")
                .unwrap()
                .enabled
        );
    }
}
