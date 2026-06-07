## Context

桌面应用为 Tauri v2 + React/antd 前端 + Rust 抓取内核。**抓取请求走后端 `reqwest`（不是 webview）**，因此代理必须作用在后端 `Client` 上，而非 Tauri 窗口的 `proxy_url`。

当前状态：
- **代理**：`region.rs::AmazonSession::new` 用 `Client::builder()....build()` 构建客户端，**完全没有 `.proxy()` 配置**。`Cargo.toml` 第 20 行 `reqwest = { version = "0.12", default-features = false, features = [...] }` 关掉了 reqwest 默认特性。实测 reqwest 0.12.28：

  | feature | 定义 | 现状 |
  |---|---|---|
  | `default` | `["default-tls", "charset", "http2", "system-proxy"]` | 被 `default-features=false` 关闭 |
  | `system-proxy` | `["hyper-util/client-proxy-system"]`（自动探测系统/环境代理） | **未加回 → 不读系统代理** |
  | `macos-system-configuration` | `["system-proxy"]`（macOS 读系统偏好代理） | 未启用 |
  | `socks` | `[]`（SOCKS5 支持） | **未启用 → 忽略 socks 代理** |

  结论：程序对客户的系统代理 / SOCKS5 一概不用，只会直连 → 海外/受限网络下超时或被 503/WAF 拦截。

- **核价**：`App.tsx` 结果表格的「Amazon 链接」列为 `<a href={amazonUrl} target="_blank">`，`amazonUrl` 由 `models::build_amazon_url → config::product_url`（`/dp/{asin}?th=1&psc=1`）生成。无「已查看」标记，无条目间导航。
- **取价**：`region.rs::fetch_price` 已经搜索页优先（`/s?k={asin}`）、商品页兜底，价格来源已是搜索页。

约束：
- 单用户桌面应用；reqwest 0.12.28。
- **reqwest `Client` 一旦构建，代理不可变更** → 改代理必须重建 `Client`（即重建 `AmazonSession`）。
- `AppState.session` 为 `Mutex<Option<AmazonSession>>`，已支持「清空后重建」。

## Goals / Non-Goals

**Goals:**
- 默认**自动**套用客户主机的系统/环境代理（含 SOCKS5），无需用户配置即可在多数环境工作。
- 用户可在 UI **手动**设置代理（`http://` / `socks5://`，支持认证）并**测试连通性**；配置**持久化**、重启生效；变更后**立即生效**。
- 核价：**抽屉**逐条查看 + **上一条/下一条**；**已查看**标记；**清空全部 / 删除单条**；**重新爬取自动清空**。
- Amazon 链接列展示**搜索页**（与取价同源），经系统浏览器打开。

**Non-Goals:**
- 不改取价/解析逻辑、SKU 规则、批量限速/冷却（搜索优先取价已实现）。
- 不代理 Tauri webview 自身的网络（只代理后端抓取）。
- 不做 PAC 自动配置脚本、不做按域名分流（`noProxy` 仅作可选基础项）。
- 本 change 不引入 `sysproxy` crate（reqwest 系统探测 + 手动配置已足够；将其列为未来 Linux/socks 探测的 fallback）。
- 代理密码本次以明文存本地（单机），系统钥匙串加密列为后续。

## Decisions

### 决策 1：修复 Cargo features —— 恢复系统代理探测 + 启用 socks（自动代理根因）
`src-tauri/Cargo.toml` reqwest 依赖改为：
```toml
reqwest = { version = "0.12", default-features = false, features = [
  "cookies", "gzip", "brotli", "json", "native-tls",
  "system-proxy",               # 跨平台系统/环境代理探测（hyper-util）
  "macos-system-configuration", # macOS 读系统偏好里的代理
  "socks",                      # SOCKS5 支持（http(s) 与系统探测之外）
  "charset", "http2",           # 恢复被 default-features=false 关掉的默认项
] }
```
- **为什么**：`system-proxy` 是 reqwest 默认特性，被 `default-features=false` 关掉是「自动代理失效」的直接根因；`socks` 让 `socks5://` 可用；补回 `charset`/`http2` 还原默认行为（非 UTF-8 解码兼容、HTTP/2）。
- **备选**：直接 `default-features = true`——但当前显式 `native-tls` 与 `default-tls` 路径需保持可控，显式列特性更清晰、避免意外引入 `default-tls` 与现有配置重复。

### 决策 2：代理配置数据模型 `ProxyConfig` + `ProxyMode`
`models.rs` 新增（`serde(rename_all = "camelCase")`）：
```rust
pub enum ProxyMode { Auto, Manual, Off }      // 自动探测 / 手动 / 关闭直连
pub struct ProxyConfig {
    pub mode: ProxyMode,
    pub url: Option<String>,        // 手动模式：http://host:port 或 socks5://host:port
    pub username: Option<String>,   // 可选认证
    pub password: Option<String>,
}
impl Default { mode: Auto, .. None }
```
- **Auto**：构建 `Client` 时**不显式配置代理**，交由 reqwest `system-proxy` 探测。
- **Manual**：`Proxy::all(url)?`；有认证时 `.basic_auth(user, pass)`（或 URL 内联 `http://user:pass@host:port`）。
- **Off**：`ClientBuilder::no_proxy()`，强制直连（即使系统有代理）。

### 决策 3：`Client` 按代理重建（代理不可变的应对）
- `AmazonSession::new(zip_code: &str, proxy: &ProxyConfig)`：在 `Client::builder()` 链上按 `ProxyMode` 分支注入。
- `set_proxy` 命令：写入 `state` 的 `ProxyConfig` + 持久化，并**清空 `state.session`**（置 `None`）；下次 `init_session` / 抓取用新代理重建。
- **为什么**：reqwest `Client` 构建后代理不可改；重建是唯一可靠路径，且 `AppState.session` 本就是 `Option`，天然支持。

### 决策 4：连通性测试 `test_proxy`
- 入参为待测 `ProxyConfig`；用它构建**临时** `AmazonSession`，跑一次 `scraper::self_check`（搜索自检 ASIN `B0DFXQWPPS`，校验全角 `￥`/JPY）。
- 返回 `{ ok, priceText, currency, message }`，**不写入** `state.session`（不污染当前会话）。
- **为什么**：用户改完代理需要「立刻知道通不通」，复用既有自检链路最省事且贴近真实抓取。

### 决策 5：配置持久化
- 写入 Tauri 应用数据目录（`app_config_dir`）下 `proxy.json`（`serde_json`）。启动 / 首次 `init_session` 时读取作为默认。
- **备选**：`tauri-plugin-store`——更省事但新增依赖；本次倾向手写 JSON（零新依赖），如团队偏好 store 可替换。
- 安全：`password` 明文存于用户目录文件，单机桌面可接受，文档注明；后续可迁系统钥匙串。

### 决策 6：核价抽屉（Drawer）+ 上一条/下一条
- 前端新增状态 `reviewIndex: number | null`（当前查看行在 `rows` 中的下标）。
- 表格行新增「查看」动作 / 点击行打开抽屉，设 `reviewIndex`。
- Drawer 内展示该行 SKU / ASIN / 价格 / 状态 / 错误，并提供：
  - 「上一条」`reviewIndex--`、「下一条」`reviewIndex++`，到边界时禁用按钮。
  - 「打开搜索页」按钮 → 打开链接并标记已查看（见决策 8）。
- **为什么用户选了抽屉模型 B**：核价是「逐条审阅」动作，抽屉能集中展示单条信息并连续翻页，比表格高亮更聚焦。

### 决策 7：已查看状态与生命周期
- 前端 `viewed: Set<string>`（键为 `asin`）。打开某条链接即 `viewed.add(asin)`。
- 列表视觉：已看行整体置灰（降低不透明度）并显示 `✓ 已看` 标签（可加一列或并入状态列）。
- 记录管理：
  - 「清空已查看」按钮：`viewed.clear()`。
  - 每行 / 抽屉内「删除记录」按钮：`viewed.delete(asin)`（把单条移出已查看）。
  - **重新爬取**（`handleStart` 开始抓取、`handleRefreshAll` 全部刷新）触发时**先 `viewed.clear()`**，使新一轮核价从零开始。
- **持久化决策**：`viewed` 仅存前端内存、不写盘。理由：重爬即清空，且结果集 `rows` 本身不跨应用重启持久化，已查看跟随结果集生命周期，写盘无意义。
- **为什么 asin 作键**：`rows` 以 `asin` 为 `rowKey`，去重也按 asin，键唯一且稳定。

### 决策 8：链接展示搜索页 + 系统打开
- 「Amazon 链接」列与抽屉的打开目标改为 `config::search_url(asin)`（`/s?k={asin}`），与取价来源同源。
- 后端 `models::build_amazon_url` 改用 `search_url`，使 `RowResult.amazonUrl` 与 CSV 导出同源（统一）。
- 打开方式从 `<a target="_blank">` 改为调用 `tauri-plugin-opener` 的 `open()`（已安装），用系统默认浏览器打开；onClick 同时 `viewed.add(asin)`。
- **为什么**：Tauri 内 `<a target="_blank">` 行为不可靠；显式 `open()` 既稳定又能在同一处「打开 + 标记已看」。

## Risks / Trade-offs

- [GUI 双击启动可能不继承 shell 导出的 `HTTP_PROXY` 等环境变量，Linux 自动模式可能读不到代理] → 提供 UI 手动模式兜底（本 change 已含）；文档提示 Linux 用户优先手动配置。
- [reqwest 对「系统代理是 SOCKS5」的自动探测历史上不稳定] → 手动填 `socks5://host:port` 是可靠保底；已启用 `socks` 特性。
- [代理密码明文存本地文件] → 限用户目录、文档注明风险；后续可迁系统钥匙串（Open Question）。
- [reqwest `Client` 代理不可变] → 用「清空 + 重建 session」解决，已纳入决策 3。
- [`amazonUrl` 由商品页改为搜索页，影响导出列与既有习惯] → 搜索页与取价同源，核价更准；如仍需商品页可后续加独立列（本次不做）。
- [手动代理 URL 格式错误] → `Proxy::all` 解析失败时 `test_proxy`/`set_proxy` 返回可读错误，UI 提示「代理地址无效」。

## Migration Plan

1. **Cargo.toml**：改 reqwest features → `cargo build` 验证编译；在配了系统代理的机器上验证自动模式直接生效。
2. **models.rs / state.rs**：新增 `ProxyConfig`/`ProxyMode`；`AppState` 持有当前代理 + 持久化读写。
3. **region.rs**：`AmazonSession::new` 扩展签名，`Client::builder()` 按代理分支注入；其余取价逻辑不动。
4. **commands.rs / service.rs**：新增 `set_proxy` / `test_proxy` / `get_proxy`；`set_proxy` 后清空并重建 session；`init_session` 启动加载持久化代理。
5. **前端**：`types.ts`/`api.ts` 加 `ProxyConfig` 与三个命令；`App.tsx` 加代理设置面板（模式/地址/认证/测试按钮）、核价抽屉（上/下条）、已查看状态与管理、链接改 `search_url` 并用 opener 打开、重爬清空 viewed。
6. **验收**：手动填代理→测试取到 `￥`；自动模式在配置了系统代理的机器直接通；核价抽屉上/下条可翻、打开即标已看、清空/删除单条可用、重爬后已看清零；链接打开的是搜索页。
- **回滚**：代理改动集中在 `Cargo.toml`/`region.rs`/`state.rs`/`commands.rs`/`service.rs`；UI 改动集中在 `App.tsx`。可分别回退。

## Open Questions

- 代理密码是否需要系统钥匙串加密存储？（本次明文，后续可选增强。）
- 是否需要 `noProxy` / PAC / 分流？（本次不做，按需再起 change。）
- 「已查看」是否需要跨应用重启持久化？（当前结论：不需要——重爬即清空、结果集本身不持久化；若将来结果集持久化再议。）
- 自动模式在 Linux 读不到系统代理时，是否引入 `sysproxy` crate 主动读取？（列为后续 fallback。）
