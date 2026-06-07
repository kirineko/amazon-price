## Context

现状是 Tauri 2 桌面应用：前端 React19+Antd6+Vite7，后端 Rust。抓取内核 `region.rs`/`sku.rs`/`config.rs`/`models.rs` 为纯 Rust、零 Tauri 依赖；仅 `commands.rs`（命令层）、`scraper.rs`（`app.emit` 推进度）、`lib.rs`（启动）与 Tauri 强耦合。

目标运行环境：印尼服务器，网络与本机一致——上一轮已验证"设日本邮编 `150-0001` + `/dp/{ASIN}?th=1` + 取第一个非空 `a-offscreen`"链路在该网络下 6/6 稳定。部署域名 `a.kirineko.tech`，HTTPS 由运维用 nginx + 手动证书处理。实际仅一个用户。

## Goals / Non-Goals

**Goals:**
- 复用抓取内核，几乎"换壳不重写"地提供 Web 访问。
- 所有数据/抓取接口需登录后才能访问。
- 一条命令可在服务器用 Docker 起服务（app + nginx）。
- 前端改动最小化，保留现有 UI 与数据形状。

**Non-Goals:**
- 不做多用户/注册/角色/权限体系（仅单密码）。
- 不做抓取结果的持久化存储/历史库（沿用内存态，进程重启清空）。
- 不做任务队列/分布式（单实例、单用户全局状态足够）。
- 不在本 change 内删除桌面壳（可后续单独处理）。

## Decisions

### 决策 1：Web 框架选 Axum
- **理由**：现有 `Cargo.toml` 已有 `tokio` 与 `reqwest`（hyper 系），Axum 同源，新增概念最少；`tower-http` 直接提供 `ServeDir`/fallback/压缩/CORS。
- **备选**：actix-web（另一套运行时心智）、warp（filter 风格较绕）。均无明显收益，弃。

### 决策 2：二进制结构 —— 复用 lib，新增 web 入口
- 把抓取内核继续放在 `amazon_price_scraper_lib`；新增二进制入口（如 `src-tauri/src/bin/web.rs` 或独立 crate）构建 Axum app。
- Tauri 相关代码（`commands.rs`/`lib.rs::run`）保留但不再是默认交付路径；`scraper.rs` 解耦掉对 `tauri::AppHandle`/`Emitter` 的直接依赖。

### 决策 3：进度推送 —— 框架无关回调 + tokio::broadcast + SSE
- 把 `ScrapeEngine::scrape_rows` 的 `app.emit("scrape-progress", …)` 抽象为**进度回调/通道**（如传入 `Sender<ScrapeProgress>` 或一个 `Fn(ScrapeProgress)`），桌面端与 Web 端各自适配。
- Web 端：进度写入一个进程级 `tokio::broadcast` 通道；`GET /api/events` 用 Axum SSE 订阅该通道转发给浏览器。
- **单用户简化**：全局一个广播通道即可，无需按 jobId 路由；前端与现状一致——挂载时建立 SSE，再调用 `POST /api/scrape`。
- **备选**：WebSocket（双向，但此处只需服务端→客户端，SSE 更轻）；每请求独立 SSE 通道（多用户才需要）。

### 决策 4：抓取请求形态 —— 长请求返回最终结果 + SSE 推进度
- `POST /api/scrape` 同步等待抓取完成返回 `RowResult[]`，与现有前端"await startScrape + 监听进度"逻辑 1:1 对应；取消用全局 `AtomicBool`（沿用 `AppState.cancel_flag`）。
- nginx 需放宽该路由的读超时（抓取可能数分钟）。
- **备选**：立即返回 jobId + 轮询/SSE 终态（任务模型）。对单用户属过度设计，暂不采用，但进度抽象已为将来留口子。

### 决策 5：认证 —— 单密码 + 服务端会话 Cookie
- 密码以 **argon2** 哈希存于环境变量；`POST /api/login` 比对成功后建立会话。
- 会话用 **`tower-sessions`（内存 store）** 或签名 Cookie：单实例单用户下内存 session 最简单；Cookie 设 `HttpOnly`+`SameSite=Lax`，HTTPS 下加 `Secure`。
- 鉴权中间件（`tower` layer）保护除 `/api/login` 与静态资源外的全部 `/api/*`。
- 登录失败用内存计数限流（按 IP/全局窗口）。提供 `POST /api/logout` 销毁会话。
- **备选**：JWT（无状态，但需处理吊销/过期，单实例无收益）；反代 Basic Auth（体验差，已在探索中排除）。

### 决策 6：状态管理 —— Arc<AppState> 注入
- 现 `AppState { session, cancel_flag, last_rows }` 用 `parking_lot::Mutex`，直接包成 `Arc` 用 Axum `State` 注入，语义不变（单用户全局态可接受）。

### 决策 7：静态托管 + SPA 回退
- `tower-http::ServeDir` 托管前端 `dist`，未命中 `/api/*` 的路径 fallback 到 `index.html`，支持前端路由刷新。

### 决策 8：部署拓扑
```
浏览器 ──HTTPS──► nginx(443, 手动证书) ──proxy──► app:8080 (Axum 单二进制: SPA + /api)
                    └─ /api/events 关闭 proxy_buffering（SSE 实时）
docker-compose: [nginx] + [app]；env: APP_PASSWORD_HASH / SESSION_SECRET / DEFAULT_ZIP / PORT
```

## Risks / Trade-offs

- [明文 HTTP 上认证形同虚设] → 必须经 nginx 启用 HTTPS 并把 HTTP 跳转到 HTTPS；会话 Cookie 设 `Secure`。运维手动申请证书。
- [SSE 被 nginx 缓冲导致进度卡顿] → `/api/events` 配置 `proxy_buffering off`、`proxy_read_timeout` 拉长，后端响应头加 `X-Accel-Buffering: no`。
- [长抓取请求被代理超时切断] → 调大 `proxy_read_timeout`/`proxy_send_timeout`；进度已走 SSE，终态请求超时也可由前端据 SSE 兜底（后续可演进为任务模型）。
- [数据中心 IP 反爬比住宅更严] → 服务器与本机网络一致、已验证 6/6，先不引代理；保留 `reqwest` 代理配置入口，必要时再开住宅/日本代理。
- [内存态随重启丢失] → 单用户可接受；如需历史再以新 change 增加持久化。
- [单密码泄露即全开放] → 强密码 + argon2 + 登录限流 + HTTPS；必要时叠加 nginx 层 IP allowlist。

## Migration Plan

1. 重构 `scraper.rs`：进度从 `app.emit` 改为注入式回调/通道（桌面与 Web 共用内核）。
2. 新增 Web 入口与路由、`AppState` 注入、SSE、CSV/解析/抓取等端点（逐个对应原命令）。
3. 加认证（登录/中间件/会话/限流/登出）。
4. 前端：重写 `api.ts`（fetch + EventSource）、新增登录页与登录态守卫，`App.tsx` 接线。
5. 容器化：多阶段 Dockerfile、`docker-compose`、`nginx.conf`、`.env.example`。
6. 在 `a.kirineko.tech` 部署，挂载证书，自检 `GET /api/self-check` 跑通，再用 `ids.txt` 端到端验收。
- **回滚**：Web 与桌面共享内核、互不破坏；如 Web 出问题可回退到桌面构建。

## Open Questions

- 会话方案最终用 `tower-sessions`（内存）还是签名 Cookie？实现期二选一，对外行为一致（本设计默认内存 session）。
- 是否需要在 nginx 层叠加 IP allowlist 作为第二道防线？（取决于是否有固定出口 IP）
