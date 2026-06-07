## 1. 代理特性修复（Cargo，自动代理根因）

- [x] 1.1 `src-tauri/Cargo.toml`：reqwest features 加回 `system-proxy`、`macos-system-configuration`，新增 `socks`，并补回 `charset`、`http2`；`cargo build` 验证编译通过
- [x] 1.2 在已配置系统代理的环境冒烟验证「自动」模式直接生效（无需 UI 配置即可访问 amazon.co.jp）

## 2. 代理数据模型与状态（Rust）

- [x] 2.1 `models.rs`：新增 `ProxyMode { Auto, Manual, Off }` 与 `ProxyConfig { mode, url, username, password }`（`serde(rename_all="camelCase")`，`Default` 为 `Auto`）
- [x] 2.2 `state.rs`：`AppState` 持有当前代理（`Mutex<ProxyConfig>`）
- [x] 2.3 持久化：实现 `proxy.json` 在应用配置目录（`app_config_dir`）的读/写（`load_proxy` / `save_proxy`）

## 3. 会话按代理构建与重建（Rust）

- [x] 3.1 `region.rs`：`AmazonSession::new` 扩展签名接收 `&ProxyConfig`；`Client::builder()` 按模式注入——Manual → `.proxy(Proxy::all(url)? [.basic_auth])`，Off → `.no_proxy()`，Auto → 不显式配置（交给 reqwest 系统探测）
- [x] 3.2 手动代理 URL 解析失败时返回可读错误（不 panic）
- [x] 3.3 同步所有 `AmazonSession::new` 调用点（`service.rs`）传入当前 `ProxyConfig`

## 4. 代理命令与会话生效（Rust）

- [x] 4.1 `service.rs` + `commands.rs`：`get_proxy` 返回当前/持久化的 `ProxyConfig`
- [x] 4.2 `set_proxy`：写入 `state` + 持久化 + 清空 `state.session`（置 `None`）以触发按新代理重建
- [x] 4.3 `test_proxy`：用待测 `ProxyConfig` 构建临时 `AmazonSession` 跑 `self_check`，返回 `{ok, priceText, currency, message}`，且不写入 `state.session`
- [x] 4.4 `init_session`：启动时加载持久化代理作为默认配置
- [x] 4.5 `lib.rs`：注册 `get_proxy` / `set_proxy` / `test_proxy` 命令

## 5. 链接同源为搜索页（Rust）

- [x] 5.1 `models::build_amazon_url` 改用 `config::search_url`，使 `RowResult.amazonUrl` 与取价来源、CSV 导出同源（确认 `sku.rs::empty_row` 路径一致）

## 6. 前端类型与 API

- [x] 6.1 `types.ts`：新增 `ProxyMode` 与 `ProxyConfig`
- [x] 6.2 `api.ts`：新增 `getProxy` / `setProxy` / `testProxy`

## 7. 前端：代理设置面板

- [x] 7.1 `App.tsx`：代理设置区（模式单选：自动/手动/关闭；地址输入；用户名/密码；仅手动模式可编辑地址与认证）
- [x] 7.2 「测试代理」按钮 → 调 `testProxy`，展示成功样例价/币种或失败原因
- [x] 7.3 「保存」→ 调 `setProxy`，提示已重建会话生效
- [x] 7.4 启动调 `getProxy` 回填面板

## 8. 前端：核价抽屉 + 上一条/下一条

- [x] 8.1 新增 `reviewIndex` 状态；表格行「查看」动作（或点击行）打开 Drawer 并设定 `reviewIndex`
- [x] 8.2 Drawer 展示当前行 SKU/ASIN/价格/状态/错误
- [x] 8.3 「上一条」/「下一条」按钮移动 `reviewIndex`，首/末条对应按钮禁用

## 9. 前端：已查看标记与记录管理

- [x] 9.1 新增 `viewed: Set<string>`（键为 asin）；打开链接即 `add(asin)`
- [x] 9.2 列表对已查看行做视觉区分（置灰 + `✓ 已看` 标识）
- [x] 9.3 「清空已查看」按钮 → `viewed.clear()`
- [x] 9.4 单条「删除记录」（列表行与抽屉内）→ `viewed.delete(asin)`
- [x] 9.5 `handleStart` / `handleRefreshAll` 开始前 `viewed.clear()`（重爬清空）

## 10. 前端：链接搜索页 + 系统打开

- [x] 10.1 链接列与抽屉的打开改用 `tauri-plugin-opener` 的 `open(searchUrl)`，onClick 同时标记 `viewed`
- [x] 10.2 移除原 `<a target="_blank">`，避免 Tauri 内不可靠的新窗口行为

## 11. 验证

- [x] 11.1 `cargo test` + `cargo build` 通过
- [x] 11.2 `npm run build`（tsc 类型检查）通过
- [x] 11.3 手动验收：手动代理测试能取到 `￥`；自动模式在配置系统代理的机器直接通；抽屉上/下条可翻；打开即标已看；清空/删除单条可用；重爬后已看清零；链接打开的是搜索页
