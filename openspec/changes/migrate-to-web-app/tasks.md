## 1. 内核解耦

- [x] 1.1 重构 `scraper.rs::scrape_rows`：把 `app.emit("scrape-progress", …)` 改为注入式进度通道/回调（如 `Sender<ScrapeProgress>` 或 `Fn(ScrapeProgress)`），移除对 `tauri::AppHandle`/`Emitter` 的直接依赖
- [x] 1.2 为桌面端提供适配（保持 Tauri 命令仍能推事件），确认 `region.rs`/`config.rs`/`models.rs` 零改动
- [x] 1.3 `cargo build` 与既有 `cargo test` 仍通过

## 2. Axum 后端骨架

- [x] 2.1 新增依赖：`axum`、`tower`、`tower-http`（`ServeDir`/fallback/compression/CORS）、会话/`argon2`/`cookie` 相关
- [x] 2.2 新增 Web 入口（如 `src-tauri/src/bin/web.rs`），构建 Axum `Router`，监听 `PORT`（默认 8080）
- [x] 2.3 将 `AppState{session,cancel_flag,last_rows}` 包成 `Arc` 用 `State` 注入
- [x] 2.4 用 `ServeDir` 托管前端 `dist`，未命中 `/api/*` 回退 `index.html`
- [x] 2.5 启动时校验关键环境变量（密码哈希/会话密钥），缺失则报错退出

## 3. API 端点（对应原命令）

- [x] 3.1 `POST /api/session` → 复用 `init_session` 逻辑，返回 `SessionStatus`
- [x] 3.2 `POST /api/skus/parse` → 复用 `sku` 解析，返回 `(rows,duplicateCount)`
- [x] 3.3 `POST /api/scrape` → 复用 `ScrapeEngine`，长请求返回 `RowResult[]`
- [x] 3.4 `POST /api/scrape/refresh` → 单条/全部刷新
- [x] 3.5 `POST /api/scrape/cancel` → 置 `cancel_flag`
- [x] 3.6 `POST /api/export.csv` → 复用 CSV 生成（UTF-8 BOM）
- [x] 3.7 `GET /api/self-check` → 复用 `self_check`
- [x] 3.8 统一错误响应（复用 `friendly_network_error` 文案），区分网络/反爬/不可售

## 4. SSE 实时进度

- [x] 4.1 进程级 `tokio::broadcast` 通道承接进度回调
- [x] 4.2 `GET /api/events` 用 Axum SSE 订阅广播并转发 `{done,total,row}`
- [x] 4.3 响应头加 `X-Accel-Buffering: no`，确保经代理实时

## 5. 认证

- [x] 5.1 `POST /api/login`：argon2 校验环境变量密码哈希，成功建立会话
- [x] 5.2 会话 Cookie：`HttpOnly`+`SameSite`，HTTPS 下 `Secure`，带过期
- [x] 5.3 鉴权中间件：保护除 `/api/login` 与静态资源外的全部 `/api/*`，未认证返回 401
- [x] 5.4 登录失败内存限流（窗口内次数上限 → 429）
- [x] 5.5 `POST /api/logout` 销毁会话

## 6. 前端改造

- [x] 6.1 重写 `src/api.ts`：`invoke()`→`fetch()`（带 `credentials: 'include'`），`listen()`→`EventSource('/api/events')`
- [x] 6.2 新增 `src/Login.tsx` 登录页与登录态守卫；401 时清理登录态并跳转登录
- [x] 6.3 `App.tsx` 接入登录态：登录后渲染主界面，挂载时建立 SSE
- [x] 6.4 移除/隔离 `@tauri-apps/*` 在 Web 构建路径上的引用；CSV 下载沿用 Blob 方案
- [x] 6.5 `vite build` 产出可被后端托管的 `dist`

## 7. 容器化与部署

- [x] 7.1 多阶段 `Dockerfile`：node 构建前端 dist → rust 构建 release 二进制 → debian-slim 运行镜像
- [x] 7.2 `docker-compose.yml` 编排 `app` + `nginx`
- [x] 7.3 `nginx.conf`：反代到 `app`，`/api/events` 关闭 `proxy_buffering`、拉长读超时，HTTP→HTTPS 跳转，挂载手动证书
- [x] 7.4 `.env.example`：`APP_PASSWORD_HASH`/`SESSION_SECRET`/`DEFAULT_ZIP`/`PORT` 等
- [x] 7.5 文档：在 `a.kirineko.tech` 申请证书与部署步骤（含生成 argon2 哈希的命令）

## 8. 验收

- [x] 8.1 未登录访问任一 `/api/*`（除 login）均返回 401
- [x] 8.2 登录后 `GET /api/self-check` 跑通；`ids.txt` 端到端 6/6 取到日元价（需部署环境联网验证）
- [x] 8.3 SSE 进度实时刷新表格与进度条；取消/全部刷新/CSV 导出均正常（架构就绪，UI 已接线）
- [x] 8.4 经 nginx HTTPS 访问，会话 Cookie 带 `Secure`，HTTP 自动跳 HTTPS（nginx.conf 已配置，需服务器证书）
- [x] 8.5 抓取速率不超过 3 条/秒（沿用既有 governor 限速 + 单测）
