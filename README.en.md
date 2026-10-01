# yi-llm

<div align="center">
  <img src="public/favicon.svg" alt="yi-llm" width="88" height="88" />
  <p><strong>A local multi-protocol LLM gateway & management desktop app</strong></p>
  <p>
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT" /></a>
    <img src="https://img.shields.io/badge/Tauri-3-24C8DB.svg?logo=tauri&logoColor=white" alt="Tauri 3" />
    <img src="https://img.shields.io/badge/React-19-61DAFB.svg?logo=react&logoColor=white" alt="React 19" />
    <img src="https://img.shields.io/badge/Rust-axum-DEA584.svg?logo=rust&logoColor=white" alt="Rust axum" />
  </p>
  <p><a href="README.md">中文</a> | English</p>
</div>

---

## Introduction

**yi-llm** is a Tauri desktop app that runs a local multi-protocol model proxy on your machine. It accepts **OpenAI Responses**, **OpenAI Chat Completions** and **Anthropic Messages** requests, then routes them to Anthropic Messages, OpenAI Chat Completions or Responses API providers. A provider uses its native protocol by default; enable either of the other client protocols in its settings to turn on automatic request/response conversion.

Provider settings, exposed model mappings and reported token usage are stored locally in SQLite, while standard model definitions live in a Git-versionable JSON catalog (`models/catalog.json`) that is easy to share and review. The app also ships a live proxy monitor, token usage statistics and one-click configuration for terminal clients such as Codex CLI, Claude Code and OpenCode. The interface supports Simplified Chinese / English and light / dark themes.

## Screenshots

### Providers

Manage providers, upstream protocols, client-protocol toggles and routing.

![Providers](docs/images/providers.png)

### Model Management

Standard models own protocol, brand icon, modalities, context and reasoning-effort policy; providers only bind real upstream model names.

![Model management](docs/images/models.png)

### Proxy Monitor

Start/stop controls, uptime, TCP connections, request counts, error rate and live traffic charts.

![Proxy monitor](docs/images/proxy.png)

### Model Usage

Token usage (input, output, cached, reasoning) aggregated by day, model and protocol, with request details and CSV export.

![Model usage](docs/images/usage.png)

### Terminal Integration

Generate and apply managed configuration for Codex CLI, Claude Code and OpenCode, with model groups, default model and configuration preview.

![Terminal integration](docs/images/terminal.png)

## Key Features

- 🌐 **Multi-protocol local proxy**: listens on `127.0.0.1:11435` by default and serves Responses, Chat Completions and Messages client endpoints, plus `GET /v1/models` and `GET /health`.
- 🔀 **Two-way cross-protocol conversion**: text conversations, image input, function tools and results, Responses namespace tools, custom text tools, reasoning text, token limits and JSON Schema structured output are translated between protocols.
- 🧩 **Standard model catalog**: model definitions are decoupled from providers; several providers can reference one standard model. The catalog is a versionable, shareable JSON file with an editor schema and atomic writes.
- 📈 **Live monitoring & usage statistics**: connection, request, error-rate and traffic charts plus runtime logs; token usage aggregated by day, model and protocol with call details and CSV export.
- 🖥️ **One-click terminal integration**: managed configuration for Codex CLI, Claude Code and OpenCode with merge-write, automatic backups, rollback on failure and native model pickers.
- 🌗 **Bilingual UI & themes**: Simplified Chinese / English interface with light / dark themes, shared design tokens and font stacks.
- 🔒 **Fully local**: settings and usage stay in local SQLite; the local proxy requires no authentication and SDK keys may use the placeholder `yi`.

## How It Works

Clients call the local proxy with any supported protocol; the proxy resolves routes through "standard model → provider binding": native-protocol traffic passes through untouched, cross-protocol traffic is converted in both directions.

| Client protocol         | Endpoint                      | Possible upstreams                                |
| ----------------------- | ----------------------------- | ------------------------------------------------- |
| OpenAI Responses        | `POST /v1/responses`        | Responses / Chat Completions / Anthropic Messages |
| OpenAI Chat Completions | `POST /v1/chat/completions` | Responses / Chat Completions / Anthropic Messages |
| Anthropic Messages      | `POST /v1/messages`         | Responses / Chat Completions / Anthropic Messages |

Provider and model-mapping changes are resolved per request, so edits take effect immediately without a proxy restart; only listen-address (host / port) changes restart the listener.

## Quick Start

Requirements: Node.js, npm, Rust stable and the native build prerequisites for Tauri on your platform.

```powershell
npm install
npm run tauri dev                                  # run in dev mode
cargo test --manifest-path src-tauri/Cargo.toml    # backend tests
npm run tauri build                                # package the app
```

Three steps to get started:

1. Create standard models in Model Management (protocol, modalities, context, reasoning effort, …).
2. Add a provider with real upstream model names and references to standards matching its native protocol; enable protocol conversion in its settings when cross-protocol clients are needed.
3. Start the listener on the Proxy page and point your clients at the local address.

UI tests run with `npm run test:ui` (Playwright); install Chromium with `npx playwright install chromium`, or set `PLAYWRIGHT_CHANNEL=msedge` to use the system Edge.

## Using the Proxy

| Purpose                        | Address                                                        |
| ------------------------------ | -------------------------------------------------------------- |
| OpenAI SDKs (Responses / Chat) | `http://127.0.0.1:11435/v1`                                  |
| Anthropic SDKs (Messages)      | `http://127.0.0.1:11435` (no protocol path)                  |
| Direct HTTP                    | complete endpoint, e.g.`http://127.0.0.1:11435/v1/responses` |
| Codex CLI                      | `http://127.0.0.1:11435/clients/codex/v1`                    |
| Claude Code                    | `http://127.0.0.1:11435/clients/claude-code`                 |
| OpenCode                       | `http://127.0.0.1:11435/clients/opencode/v1`                 |

The Proxy → protocol-address tab shows each protocol's Base URL and complete request URL with copy controls and compatible provider counts. The local proxy does not require authentication; use the placeholder `yi` as the SDK API key.

Terminal-specific endpoints are restricted to the selected model collection: unknown models, unsupported protocols, disabled providers and moved models are rejected, with no fallback to the global default provider. Reapply terminal configuration after changing the listen address or a terminal's model collection.

## Model Management

Standard models own their protocol, brand icon, input/output modalities, context size, maximum output tokens and reasoning-effort policy. A provider binding only stores the real upstream model name and the standard-model reference, and its name is independent of the standard's display name. Changing a standard affects all associated routes on the next request.

All standard definitions are maintained in [models/catalog.json](models/catalog.json): development builds read and write the repository file, packaged builds use `<app-data>/models/catalog.json`, and setting `YI_LLM_MODEL_CATALOG` to an absolute path switches to a Git checkout. Model Management displays and copies the active path. External changes are validated before model reads and requests; invalid revisions return an error while retaining the last valid cache. See the [catalog guide](models/README.md).

## Usage Statistics

The Usage page shows readable `Provider / model name` labels with brand icons: total, input / output / cached / reasoning tokens, request count, active models and per-request averages; the daily trend chart splits input / output / cached tokens and can be grouped by model. Request records keep protocol, duration and token details, with CSV export including raw route IDs.

## Terminal Integration

On the Terminal page, pick a client, select enabled providers and the model collection, choose a default model, then generate and apply the managed configuration.

| Client      | Protocol                | Native model selection | Managed files                                                       |
| ----------- | ----------------------- | ---------------------- | ------------------------------------------------------------------- |
| Codex CLI   | OpenAI Responses        | `/model`             | `~/.codex/config.toml`, `yi-models.json`                        |
| Claude Code | Anthropic Messages      | `/model`             | `~/.claude/settings.json`                                         |
| OpenCode    | OpenAI Chat Completions | `/models`            | `~/.config/opencode/opencode.json` or existing `opencode.jsonc` |

These paths respect `CODEX_HOME`, `CLAUDE_CONFIG_DIR`, `XDG_CONFIG_HOME` and `OPENCODE_CONFIG`. Applying configuration merges the managed settings with the existing file, creates backups of overwritten files and uses atomic replacement; if writing files or saving the collection fails, the previous files are restored. The preview displays managed settings only and excludes unrelated credentials and hooks.

Codex uses the native `model_catalog_json` catalog so `/model` lists the selected models and their capabilities; Claude Code uses native `modelPicker.options` and requires **2.1.242 or newer**; OpenCode uses the official OpenAI-compatible provider configuration. Native model discovery was verified with Codex **0.158.0**, Claude Code **2.1.283** and OpenCode **1.14.50**. Restart terminal sessions after applying configuration, and keep the proxy running while terminals send requests.

## Protocol Capabilities & Limits

- Cross-protocol conversion covers text conversations, image input (Anthropic upstreams require a base64 data URL), function tools and results, Responses namespace tools (flattened upstream, restored in Responses output), custom text tools, reasoning text, token limits and JSON Schema structured output (`text.format` → Anthropic `output_config.format` / Chat `response_format`).
- Thinking signatures / reasoning payloads are preserved across turns: Anthropic `signature_delta` and `redacted_thinking` surface as reasoning-item `encrypted_content` and are echoed back on the next request; Chat Completions `reasoning_content` is replayed on assistant messages and mapped to reasoning output.
- Prompt-cache usage is reported through `input_tokens_details.cached_tokens`.
- Stateful Responses requests (`store=true` or `previous_response_id`) and file input are rejected across translated protocols.
- Responses server-side tools such as `web_search` are omitted when the selected upstream cannot execute them; other built-in tools require a native Responses upstream. Native connections retain their provider-specific request fields.

## Development

```powershell
npm run dev           # frontend-only Vite dev server
npm run typecheck     # TypeScript type checking
npm run lint          # ESLint
npm run format        # Prettier formatting
npm run test:ui       # Playwright UI tests
npm run check:theme   # theme token check
```

The backend is layered Rust code (`commands → service → db / catalog / terminal / proxy → domain`) with integration tests in `src-tauri/tests/` (wiremock-mocked upstreams); the frontend is feature-sliced with shared i18n, navigation, notify, theme and UI modules. Code layout, layering rules and migration notes are described in [docs/architecture.md](docs/architecture.md).

## Project Structure

```
src/          React frontend (feature-sliced)
src-tauri/    Rust backend: proxy, protocol conversion, SQLite, terminal config
models/       standard model catalog JSON + schema
docs/         architecture docs and screenshots
tests/        Playwright UI tests
scripts/      helper scripts (theme token check, etc.)
```

## License

This project is released under the [MIT License](LICENSE). © 2026 peng.yi

The brand icons in `src/assets/providers/` are adapted from [cc-switch](https://github.com/farion1231/cc-switch) (MIT); attribution and its license text are included in that directory. Protocol marks use their company's logo: OpenAI for Responses and Chat Completions, Anthropic for Messages.
