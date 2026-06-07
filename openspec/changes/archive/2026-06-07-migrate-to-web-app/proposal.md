## Why

当前应用是 Tauri 桌面程序，只能在本机运行。实际只有一个使用者，但需要随时随地通过浏览器访问，并部署到服务器 `http://a.kirineko.tech/`。由于抓取内核（`region.rs`/`sku.rs`/`config.rs`/`models.rs`）是纯 Rust、零 Tauri 依赖，可几乎原样复用，因此用 **Axum**（与现有 `tokio`/`reqwest` 同源）做 Web 后端即可"换壳不重写"，同时补齐认证与容器化部署。

## What Changes

- **新增 Axum HTTP 后端**，复用现有抓取内核；把 9 个 Tauri command 映射为受保护的 REST 端点（`/api/session`、`/api/skus/parse`、`/api/scrape`、`/api/scrape/refresh`、`/api/scrape/cancel`、`/api/export.csv`、`/api/self-check`）。
- **进度推送由 Tauri 事件改为 SSE**：抓取进度经 `tokio::broadcast` 通道经 `GET /api/events`（Server-Sent Events）推给浏览器（单用户，全局广播即可）。
- **新增单密码认证**：`POST /api/login` 校验环境变量中的 argon2 密码哈希，签发 **httpOnly + SameSite 会话 Cookie**；中间件保护除 `/api/login` 外的所有 `/api/*`；登录失败做内存限流防爆破。
- **前端最小改造**：`src/api.ts` 由 `invoke()/listen()` 改为 `fetch()/EventSource`；新增登录页与登录态守卫；保留 Antd 表格、结果管理等 UI。
- **容器化部署**：多阶段 Dockerfile（前端 `dist` + Rust release 单二进制，同时托管 SPA 与 `/api`）；`docker-compose` 编排 `app` + `nginx`；nginx 反代到 `a.kirineko.tech`，TLS 证书由运维手动申请挂载。
- **BREAKING（交付形态）**：默认交付物从 Tauri 桌面应用改为 Web 服务；桌面打包不再是主交付路径（抓取内核保持共享，桌面壳可后续单独保留）。

## Capabilities

### New Capabilities
- `web-auth`: 单密码登录、会话 Cookie 签发与校验、`/api/*` 鉴权中间件、登录限流与登出。
- `web-api`: 以 HTTP 暴露抓取能力（会话初始化/解析/抓取/刷新/取消/导出/自检）、SSE 实时进度、SPA 静态资源托管与前端接线。
- `deployment`: Docker 多阶段构建、`docker-compose` 编排、nginx 反代与 HTTPS、运行所需环境变量与运维流程。

### Modified Capabilities
<!-- openspec/specs/ 基线为空；SKU 解析的加固在独立 change `harden-sku-parsing` 中处理，本 change 不重复。 -->

## Impact

- **新增依赖（Rust）**：`axum`、`tower`/`tower-http`（`ServeDir`/`fallback`/`CORS`/`compression`）、会话/JWT（`tower-sessions` 或 `jsonwebtoken`）、`argon2`、`cookie`；复用 `tokio`/`reqwest`/`serde`。
- **新增依赖（前端）**：无强依赖（用原生 `fetch`/`EventSource`）；可选轻量路由用于登录页。
- **新增文件**：Web 服务入口（如 `src-tauri/src/bin/web.rs` 或新 crate）、`auth` 模块；`Dockerfile`、`docker-compose.yml`、`nginx.conf`、`.env.example`。
- **改动文件**：`scraper.rs`（`app.emit` 解耦为框架无关的进度回调/通道）、`src/api.ts`（重写）、`src/App.tsx`（接入登录态）、新增 `src/Login.tsx`。
- **不改动**：`region.rs`、`config.rs`、`models.rs`；`sku.rs` 由 `harden-sku-parsing` 负责。
- **运维/安全**：必须在 nginx 层启用 HTTPS（明文 HTTP 上认证无意义）；服务器位于印尼、与本机网络一致，沿用已验证的"设日本邮编 + `/dp/{ASIN}?th=1`"链路，抓取成功率风险可控、暂不引入代理。
