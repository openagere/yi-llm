# 架构说明

yi-llm 由 Tauri（Rust 后端）和 React（前端）组成。后端提供本地多协议代理与配置存储，前端提供管理界面。两端通过 Tauri command 通信，错误统一为 `{ code, message }`。

## 后端 `src-tauri/src`

按层拆分，依赖方向自上而下：`commands → service → db / catalog / terminal / proxy / backup → domain`。

| 目录 | 职责 |
| --- | --- |
| `error.rs` | `AppError`（`validation` / `not_found` / `conflict` / `database` / `io` / `upstream` / `internal`），序列化为 `{ code, message }`。 |
| `domain/` | 纯数据结构与校验（Provider、标准模型、能力、终端、用量、设置），不依赖 Tauri、SQLite 或 HTTP。 |
| `db/` | `r2d2` 连接池（WAL、busy timeout）、版本化迁移（`migrations/`）、按聚合划分的 `repo/`，以及 `UsageWriter`（用量异步批量写入）。 |
| `catalog/` | 标准模型目录 JSON 的读写与校验（原子写入、保留上一份有效缓存）。 |
| `backup/` | Provider 配置的加密导出 / 导入：PBKDF2-HMAC-SHA256 派生密钥、AES-256-GCM 认证加密，导出文件为「明文参数外壳 + 密文载荷」的 JSON；只依赖 `domain` 与 `error`。 |
| `service/` | 业务用例：编排 repo、catalog、终端配置和代理生命周期。修改数据库后调用 `AppState::invalidate_routes()` 使路由缓存失效。 |
| `commands/` | 薄适配层：解析参数、调用 service、返回 `Result<T, AppError>`。 |
| `proxy/` | axum 路由、请求管线、监控、日志、生命周期状态机；路由表通过 `arc-swap` 缓存，每次请求无锁读取。 |
| `protocol/` | 协议模型与转换（Responses / Chat Completions / Anthropic Messages），`UpstreamClient` trait 隔离真实 HTTP。 |
| `terminal/` | 终端客户端（Codex、Claude Code、OpenCode、Pi、DeepSeek Harness，OpenCode / Pi / DeepSeek Harness 可先选接入协议）配置生成与应用，`ClientConfigurator` trait，应用失败时回滚。 |
| `app/` | 启动装配、日志、旧版数据迁移、`AppState`。 |

集成测试位于 `src-tauri/tests/`（`common/mod.rs` 提供公共夹具，`proxy_*.rs` 按主题拆分，使用 wiremock 模拟上游）。

### SQLite 兼容性

重构不改变 schema 语义：迁移按 `user_version` 顺序执行，`baseline` 与历史迁移保留，已有用户数据可直接升级。

### 内存分配器评估

评估过 `mimalloc` 作为全局分配器：桌面代理的并发与分配量较小，收益不足以抵消额外的原生依赖和跨平台构建成本，因此**不引入**该依赖。若日后压测发现分配器成为瓶颈，再单独评估。

## 前端 `src`

按功能切片（feature-sliced）组织：

```
src/
  app/            应用壳：App、providers（QueryClient）、routes（懒加载页面出口）、layout（Sidebar、WorkspaceToolbar）
  features/
    providers/    Provider 列表与编辑
    models/       标准模型与能力
    proxy/        代理运行、监控、日志、协议地址
    usage/        模型用量
    terminal/     终端接入
    settings/     外观、字体、语言
  shared/
    api/          `invoke` 封装、`AppError` 解析、`describeError`
    i18n/         语义键国际化（zh-CN 为基准，en-US 键必须一致）
    navigation/   页面栈、未保存修改守卫（zustand）
    notify/       全局提示（zustand）
    settings/     本地偏好存储与持久化（zustand）
    theme/        主题与字体应用
    ui/           无业务的通用组件
    lib/          协议定义、地址工具
  styles/         设计令牌与各页面样式
```

约定：

- 每个 feature 的 `index.ts` 只暴露供其他 feature 使用的 hook、类型与工具，**不**导出页面组件；页面通过 `pages.ts` 导出，由 `app/routes.tsx` 懒加载。
- 依赖方向：`providers → models`；`proxy`、`usage`、`terminal` 依赖 `providers` 与 `models`；`models` 不依赖 `providers`。跨 feature 只能经对方 `index.ts` 引用。
- 命令类型由各 feature 的 `api.ts` 通过模块扩展注册到 `CommandMap`，`invoke("command", args)` 的参数与返回值均有类型检查。
- 服务端数据用 TanStack Query：变更后 `invalidateQueries`，轮询用 `refetchInterval`（页面隐藏或不可见时停止）；导航、未保存守卫和通知用 zustand。
- 页面按路由懒加载，Vite `manualChunks` 拆出 react、query、radix、charts。用量、日志和连接列表保持整表渲染：当前行数很小，且 Playwright 与表格样式依赖每一行都在 DOM 中。`@tanstack/react-virtual` 已安装，列表明显变长时再接入。未启用 React Compiler，现有组件以 Query 选择器与局部 memo 为主。
- 文案使用语义键：`t("provider.list.addButton")`。新增文案先写入 `shared/i18n/locales/zh-CN/<feature>.ts`，再补 `en-US/<feature>.ts`，缺键会导致类型检查失败。非 React 代码使用 `translate()`。
- 后端错误消息目前为中文；英文界面对无法翻译的错误显示通用提示（`describeError`）。

## 迁移说明

- Tauri command 的错误从 `String` 变为 `{ code, message }`；前端通过 `toAppError` / `describeError` 处理，测试夹具（`tests/mock-backend.ts`）同步使用该结构。
- 前端 `src/lib`、`src/components`、`src/core` 已移除，对应代码迁入 `src/features` 与 `src/shared`；`src/index.css` 移至 `src/styles/base.css`。
- `localStorage` 键 `yi-llm:settings`（及旧键 `llm-man:settings`）保持不变，用户偏好无需迁移。

## 验证

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run lint
npx tsc --noEmit
npm run test:ui
npm run check:theme
```

Playwright 需要 Chromium：`npx playwright install chromium`；已安装 Edge 时可设置环境变量 `PLAYWRIGHT_CHANNEL=msedge` 直接使用系统浏览器。
