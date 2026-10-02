import type { Page } from "@playwright/test";

/** Browser-only IPC fixture. Production code and real local configuration stay untouched. */
export async function installBackend(page: Page, theme: "light" | "dark", empty = false, failure = false, options: { native?: boolean; fullPage?: boolean; manyModels?: boolean } = { native: false, fullPage: false }) {
  await page.addInitScript(({ theme, empty, failure, options }) => {
    if (!localStorage.getItem("yi-llm:settings")) localStorage.setItem("yi-llm:settings", JSON.stringify({ version: 1, settings: { theme, font: "system", language: "zh-CN" } }));
    const capabilities = { input_modalities: ["text", "image"], output_modalities: ["text"], context_window: 200000, max_output_tokens: 32000, effort: { support: "supported", levels: ["low", "medium", "high"], default: "medium" } };
    let models = empty ? [] : [
      { id: "gpt", name: "GPT-5", protocol: "responses", brand: "openai", capabilities, provider_count: 2 },
      { id: "claude", name: "Claude Sonnet", protocol: "anthropic", brand: "anthropic", capabilities, provider_count: 1 },
      { id: "deepseek", name: "DeepSeek V3", protocol: "openai_chat", brand: "deepseek", capabilities: { ...capabilities, input_modalities: ["text"], effort: { support: "unsupported", levels: [], default: null } }, provider_count: 1 },
      { id: "gemini", name: "Gemini Pro", protocol: "openai_chat", brand: "gemini", capabilities, provider_count: 0 },
      { id: "kimi", name: "Kimi K2", protocol: "openai_chat", brand: "kimi", capabilities, provider_count: 0 },
    ];
    let providers = empty ? [] : [
      ["openai", "OpenAI", "oa", "responses", "https://api.openai.com/v1", "gpt"],
      ["anthropic", "Anthropic", "ac", "anthropic", "https://api.anthropic.com", "claude"],
      ["deepseek", "DeepSeek", "ds", "openai_chat", "https://api.deepseek.com/v1", "deepseek"],
      ["gateway", "Work Gateway", "work", "responses", "https://gateway.example.com/v1", "gpt"],
    ].map(([id, name, short_code, provider_type, base_url, modelId], index) => ({ id, name, short_code, provider_type, base_url, api_key: "test-key", enabled: index !== 3, thinking: "medium", extra: {}, is_default: index === 0, protocol_support: { responses: index !== 1, openai_chat: true, anthropic: index === 1 }, models: [{ id: index + 1, provider_id: id, route_id: `${short_code}/${modelId}`, name: modelId, standard_model_id: modelId, capabilities }] }));
    if (options.manyModels && !empty) {
      providers[0].models.push(...["gpt-mini", "gpt-large", "gpt-reasoning", "gpt-fast"].map((name, index) => ({
        ...structuredClone(providers[0].models[0]), id: 100 + index, route_id: `oa/${name}`, name, standard_model_id: name,
      })));
    }
    if (options.fullPage && !empty) {
      providers.push(...[4, 5].map(index => ({ ...structuredClone(providers[0]), id: `provider-${index}`, name: `Gateway ${index}`, short_code: `gw${index}`, is_default: false })));
      models.push({ ...structuredClone(models[0]), id: "extra", name: "Extra Model", provider_count: 0 });
    }
    let maximized = false;
    type IpcArgs = { action?: string; host?: string; port?: number; logLevel?: string; id?: string; provider?: { id: string }; models?: unknown[]; model?: { id: string }; profile?: { client?: string; models?: unknown[]; provider_id?: string; model_source?: string; model?: string | null } };
    const nativeCommands: { command: string; args: IpcArgs }[] = [];
    const ipcError = (code: string, message: string) => ({ code, message });
    Object.defineProperty(window, "isTauri", { value: options.native, configurable: true });
    Object.defineProperty(window, "__nativeCommands", { value: nativeCommands, configurable: true });
    let phase = "running";
    let settings = { host: "127.0.0.1", port: 11435, log_level: "info" };
    const now = Date.now();
    const day = new Date().toLocaleDateString("en-CA");
    const usageModels = models.slice(0, 3).map((model, index) => ({ model: model.name, requests: 25 + index * 10, input_tokens: 230000 + index * 70000, output_tokens: 30000 + index * 15000, cached_tokens: 80000, reasoning_tokens: 4000 }));
    const totals = usageModels.reduce((total, item) => ({ total_requests: total.total_requests + item.requests, input_tokens: total.input_tokens + item.input_tokens, output_tokens: total.output_tokens + item.output_tokens, cached_tokens: total.cached_tokens + item.cached_tokens, reasoning_tokens: total.reasoning_tokens + item.reasoning_tokens }), { total_requests: 0, input_tokens: 0, output_tokens: 0, cached_tokens: 0, reasoning_tokens: 0 });
    const usage = { ...totals, days: [{ day, ...totals }], daily_models: usageModels.map(item => ({ day, model: item.model, tokens: item.input_tokens + item.output_tokens })), models: usageModels, recent: usageModels.flatMap((model, index) => Array.from({ length: 4 }, (_, i) => ({ id: index * 4 + i, requested_at: now - i * 90000, provider_name: providers[index]?.name, protocol: providers[index]?.provider_type, model: model.model, input_tokens: 12000, output_tokens: 900, cached_tokens: 2000, reasoning_tokens: 300 }))) };
    const totalsMonitor = { connections: 3, accepted_connections: 142, peak_connections: 8, active_requests: 1, peak_requests: 4, requests: 105, completed: 104, errors: 1, received_bytes: 1590000, sent_bytes: 832000, duration_ms: 239000 };
    const profiles = ["codex", "claude-code", "opencode"].map(client => ({ client, config_path: `C:/Users/user/.${client}/config.json`, exists: true, active: true, active_mode: "proxy", config_error: null, direct_profile: null, profile: { client, models: [{ provider_id: providers[0]?.id, model: providers[0]?.models[0].route_id }], default_model: providers[0]?.models[0].route_id ?? "" } }));
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {
      metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
      invoke: async (command: string, args: IpcArgs = {}) => {
        if (failure && ["list_providers", "list_standard_models", "get_model_catalog_info", "get_proxy_runtime", "get_usage", "get_terminal_profiles"].includes(command)) throw ipcError("internal", "Fixture: backend unavailable");
        if (command.startsWith("plugin:window|")) nativeCommands.push({ command, args });
        switch (command) {
          case "plugin:window|is_maximized": return maximized;
          case "plugin:window|toggle_maximize": maximized = !maximized; return;
          case "plugin:window|minimize": return;
          case "plugin:window|start_dragging": return;
          case "plugin:window|close": return;
          case "plugin:window|set_theme": return;
          case "plugin:window|set_title": return;
          case "list_providers": return structuredClone(providers);
          case "list_standard_models": return structuredClone(models);
          case "get_model_catalog_info": return { path: "C:/Users/user/.yi-llm/models.json", version: 1 };
          case "get_proxy_runtime": return { phase, desired_running: phase === "running", started_at: now - 3600000, address: "127.0.0.1:11435", last_error: null, active_requests: 1, active_connections: 3, settings };
          case "control_proxy": phase = args.action === "stop" ? "stopped" : "running"; return;
          case "get_proxy_monitor": return { since: now - 3600000, sampled_at: Date.now(), totals: totalsMonitor, rate_seconds: 5, received_per_second: 3200, sent_per_second: 1700, requests_per_second: .8, samples: Array.from({ length: 30 }, (_, i) => ({ timestamp: now - (30 - i) * 5000, connections: 2 + i % 4, active_requests: i % 3, requests: 3 + i % 2, errors: 0, received_bytes: 12000 + i * 100, sent_bytes: 5000 + i * 30 })), routes: [{ client: "codex", protocol: "responses", ...totalsMonitor }] };
          case "get_settings": return settings;
          case "save_settings": settings = { host: args.host, port: args.port, log_level: args.logLevel }; return;
          case "get_usage": return structuredClone(usage);
          case "get_logs": return "2026-09-29 18:00:00 INFO Proxy listening on 127.0.0.1:11435\n2026-09-29 18:01:00 INFO POST /v1/responses 200";
          case "get_terminal_profiles": return profiles;
          case "preview_terminal_config": return { config_path: profiles[0].config_path, endpoint: "http://127.0.0.1:11435", files: [{ path: "config.toml", content: 'model = "oa/gpt"\nmodel_provider = "yi"' }] };
          case "apply_terminal_config": { const target = profiles.find(item => item.client === args.profile?.client); if (target && args.profile) Object.assign(target, { active: true, active_mode: "proxy", profile: structuredClone(args.profile) }); return { config_path: profiles[0].config_path, endpoint: "http://127.0.0.1:11435", backup_paths: [], model_count: args.profile?.models?.length ?? 0 }; }
          case "preview_terminal_direct_config": { const model = args.profile?.model_source === "provider" ? (args.profile.model ?? providers.find((provider) => provider.id === args.profile?.provider_id)?.models[0]?.name ?? null) : null; return { config_path: profiles[0].config_path, endpoint: "https://api.openai.com/v1", files: [{ path: "config.toml", content: `model_provider = "yi_direct_openai"\n${model ? `model = "${model}"\n` : ""}[model_providers.yi_direct_openai]\nbase_url = "https://api.openai.com/v1"\nexperimental_bearer_token = "••••••••"` }] }; }
          case "apply_terminal_direct_config": { const target = profiles.find(item => item.client === args.profile?.client); if (target && args.profile) Object.assign(target, { active: true, active_mode: "direct", direct_profile: structuredClone(args.profile) }); return { config_path: profiles[0].config_path, endpoint: "https://api.openai.com/v1", backup_paths: [], model_count: 1 }; }
          case "save_provider": {
            const provider = args.provider as (typeof providers)[number];
            const existing = providers.findIndex(p => p.id === provider.id);
            const next = { ...provider, models: args.models as (typeof providers)[number]["models"] };
            if (existing >= 0) providers[existing] = next; else providers.push(next);
            return { terminal_config_warnings: [] };
          }
          case "delete_provider": providers = providers.filter(p => p.id !== args.id); return;
          case "save_standard_model": { const model = args.model as (typeof models)[number]; models = [...models.filter(m => m.id !== model.id), model]; } return;
          case "delete_standard_model": models = models.filter(m => m.id !== args.id); return;
          case "test_provider": return "OK";
          default: throw ipcError("not_found", `Unmocked IPC command: ${command}`);
        }
      },
    }, configurable: true });
  }, { theme, empty, failure, options });
}
