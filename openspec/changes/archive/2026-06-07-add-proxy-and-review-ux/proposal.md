## Why

当前桌面应用在客户主机上**无法走客户已配置的代理**访问 `amazon.co.jp`，直连导致超时 / 503 / WAF 拦截。根因已定位到 `src-tauri/Cargo.toml`：reqwest 被设为 `default-features = false`，而 reqwest 0.12.28 的 `default` 特性里包含 `system-proxy`（`= ["hyper-util/client-proxy-system"]`，自动探测系统/环境代理），被一并关掉且未加回；同时未启用 `socks`，`Client::builder()` 也没有任何 `.proxy()` 配置——因此程序对客户的系统代理 / SOCKS5 代理一概不用，只会直连。

与此同时，人工核价体验有两处短板：（1）核价需逐个打开 Amazon 链接比对，但没有「已查看」标记、也无法在条目间「上一条 / 下一条」导航，几十上百条时容易看漏、看重；（2）链接列打开的是商品页 `/dp/{asin}`，而价格实际来自搜索页 `/s?k={asin}`，两者不同源，核价时常对不上。

## What Changes

**代理（proxy-settings）**
- 恢复 reqwest 系统代理探测（加回 `system-proxy` + `macos-system-configuration`）并启用 `socks`，使程序默认自动套用客户的系统/环境代理（含 SOCKS5）。
- 新增 UI 代理设置：模式（自动探测 / 手动 / 关闭直连）+ 手动地址（`http://` 或 `socks5://`，支持 `user:pass@` 认证）。
- 新增「测试代理」：用自检 ASIN 实际跑一次，验证能否取到日元价。
- 代理配置持久化到本地、启动自动加载；变更代理后重建抓取会话使其立即生效。

**核价交互（price-review-ux）**
- 新增核价抽屉（Drawer）：逐条查看商品信息与价格，内置「上一条 / 下一条」切换。
- 「已查看」标记：打开某条链接即记入已查看，列表对已看条目做视觉区分。
- 已查看记录管理：支持「清空全部」与「删除单条」；每次重新爬取（开始抓取 / 全部刷新）自动清空全部已查看记录。
- Amazon 链接列改为展示**搜索页** `/s?k={asin}`，与取价来源同源，并通过系统方式（`tauri-plugin-opener`）打开。

> 价格「优先从搜索页取」已在现有抓取内核实现（`region.rs::fetch_price` 搜索优先、商品页兜底），本 change **不改取价逻辑**，仅让**展示链接**与取价来源对齐。

## Capabilities

### New Capabilities
- `proxy-settings`: 代理配置与生效能力——默认自动探测系统/环境代理（含 SOCKS5）、UI 手动配置（http/socks + 认证）、连通性测试、配置持久化与启动加载、代理变更后会话重建、关闭代理直连。
- `price-review-ux`: 人工核价交互能力——核价抽屉逐条查看与上一条/下一条导航、已查看标记与展示、清空全部/删除单条已查看记录、重新爬取自动清空记录、Amazon 链接以搜索页同源展示并经系统打开。

### Modified Capabilities
<!-- openspec/specs/ 基线仅含 sku-parsing；本 change 不改 SKU 规则，也不改取价/解析逻辑（搜索优先已实现），故无 MODIFIED delta。 -->

## Impact

- **改动文件（Rust）**：
  - `Cargo.toml`：reqwest features 加回 `system-proxy`、`macos-system-configuration`，新增 `socks`（并补回默认的 `charset`、`http2`）。
  - `region.rs`：`AmazonSession::new` 接收代理配置，`Client::builder()` 据此 `.proxy(Proxy::all(url))`（含 `basic_auth`）或 `.no_proxy()`；自动模式则不显式配置、交由 reqwest 系统探测。
  - `models.rs`：新增 `ProxyConfig`（模式 / url / 认证）与 `ProxyMode` 枚举；`SessionStatus`、自检结果按需带回当前代理摘要。
  - `state.rs`：持有当前 `ProxyConfig`；负责持久化读写。
  - `commands.rs` / `service.rs`：新增 `set_proxy`、`test_proxy`、`get_proxy` 命令；代理变更后清空并重建 `session`。
  - 持久化：写入 Tauri 应用数据目录下的 JSON 配置文件（或 `tauri-plugin-store`）。
- **改动文件（前端）**：`src/api.ts` / `src/types.ts`（代理命令与 `ProxyConfig` 类型、已查看相关）；`src/App.tsx`（代理设置面板、测试按钮、核价抽屉、已查看状态与管理、链接改 search 并经 opener 打开）。
- **依赖**：reqwest 特性调整（无新增直接 crate）；复用已安装的 `tauri-plugin-opener`；如选用 store 持久化则新增 `tauri-plugin-store`（可选，亦可手写 JSON）。
- **不改**：`sku.rs`（SKU 解析规则）；取价/解析逻辑（搜索优先 + 商品页兜底已实现）；批量限速/冷却。
- **风险/前置**：`system-proxy` 在 Linux 仅依赖环境变量，GUI 双击启动可能不继承 shell 导出的变量——此时需用 UI 手动模式兜底（本 change 已覆盖）。
