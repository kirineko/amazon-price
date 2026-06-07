## 1. 货币锁定（region.rs）

- [x] 1.1 将 `AmazonSession::new` 的 `Client::builder().cookie_store(true)` 改为 `.cookie_provider(Arc<reqwest::cookie::Jar>)`，并在 `AmazonSession` 中持有该 `Arc<Jar>` 句柄
- [x] 1.2 新增 `seed_currency_cookies()`：向 jar 写入 `i18n-prefs=JPY`、`lc-acbjp=ja_JP`（域 `.amazon.co.jp`、路径 `/`）；`new()` 中预置一次
- [x] 1.3 在 `init()` 完成（含 `set_delivery_zip`）后再次调用 `seed_currency_cookies()`，抵御首页 `Set-Cookie` 覆盖
- [x] 1.4 移除 `fetch_glow_token` 里手写的一次性 `Cookie: lc-acbjp=ja_JP; i18n-prefs=JPY` header（改由 jar 统一携带），确认地区设置流程仍正常

## 2. 取价来源：搜索优先 + 商品页兜底（region.rs + config.rs）

- [x] 2.1 `config.rs`：`product_url` 改为 `/dp/{asin}?th=1&psc=1`；新增 `search_url(asin)` → `/s?k={asin}`
- [x] 2.2 `region.rs`：新增 `fetch_search_html(asin)`（带默认头 + 已锁定币种 cookie）
- [x] 2.3 新增 `parse_search_page(html, asin) -> Option<(text, value, currency)>`：遍历 `div[data-component-type="s-search-result"]`，仅取 `data-asin` 忽略大小写等于目标 ASIN 的卡片，读其 `.a-price .a-offscreen` 原文与货币符号
- [x] 2.4 在 `fetch_and_parse`（或新取价入口）中实现「先搜索、0 命中再 `/dp` 兜底」的两段式取价
- [x] 2.5 商品页解析复用现有 `parse_product_page`，但同样返回检测到的币种（见任务 3）

## 3. 币种校验与失败语义（region.rs + models.rs + scraper.rs）

- [x] 3.1 定义币种判定：从页面真实符号映射（全角 `￥`→JPY；`$`/`HK$`/`€`/半角 `¥` 等→对应非日元标识）；MUST 基于页面原文，禁止用 `format!("￥…")` 拼接串判定
- [x] 3.2 将 `extract_price`/相关解析函数的返回从 `(Option<String>, Option<u64>)` 扩展为携带 `currency`，贯穿到 `ParsedProduct`
- [x] 3.3 `models.rs`：`RowResult.currency` 由解析结果真正赋值（不再恒为 `"JPY"`）
- [x] 3.4 `scraper.rs::scrape_one_with_retry`：取到价格但币种非日元时，置 `RowStatus::Failed` + 错误信息含真实币种，并**直接返回不重试**
- [x] 3.5 仅币种为 JPY 时计 `RowStatus::Success`

## 4. 大批量分批限速与冷却（config.rs + scraper.rs + models.rs）

- [x] 4.1 `config.rs`：新增常量 `BATCH_SIZE = 100`、`BATCH_COOLDOWN_SECS = 30`
- [x] 4.2 `models.rs`：`ScrapeProgress` 增加阶段字段（如 `phase`：抓取中/冷却中，可带批次序号与剩余冷却秒数）
- [x] 4.3 `scraper.rs::scrape_rows`：当有效抓取条数 > `BATCH_SIZE` 时按批切块执行，块间 `tokio::time::sleep(BATCH_COOLDOWN)`
- [x] 4.4 冷却用 `tokio::select!` 同时监听 `cancel_flag`，取消时立即结束冷却
- [x] 4.5 进度 `done/total` 始终基于全局总量；进入冷却时发出携带冷却阶段的进度事件
- [x] 4.6 确认 `refresh_all` 走同一分批路径；单条刷新不分批

## 5. 自检币种感知（scraper.rs）

- [x] 5.1 `self_check` 返回检测到的币种与样例价（沿用 `SELF_CHECK_ASIN`）
- [x] 5.2 非日元时自检判为「货币未锁定」并在消息中写明真实币种

## 6. 服务层与前端接线（service.rs + api.ts + types.ts + App.tsx）

- [x] 6.1 `service.rs`：透传币种与进度阶段，确保 SSE/返回结构包含新字段
- [x] 6.2 `src/types.ts`：`RowResult` 增加/启用 `currency`；`ScrapeProgress` 增加阶段字段
- [x] 6.3 `src/api.ts`：按需适配新字段（无破坏性变更）
- [x] 6.4 `App.tsx`：复制数值改为保留小数（`replace(/[^\d.]/g,"")`，含小数时优先用文本，否则用 `priceValue`）
- [x] 6.5 `App.tsx`：冷却阶段在进度区/日志显示等待提示（如「批次 x/y 完成，冷却 30s」）
- [x] 6.6 `App.tsx`：自检状态条展示币种（如「自检通过 ￥1,980 (JPY)」/「检测到 USD，货币未锁定」）

## 7. 测试与验收

- [x] 7.1 单测：币种判定（全角 `￥`→JPY 成功；`USD`/`HK$`→失败且不重试）
- [x] 7.2 单测：搜索页解析按 `data-asin` 精确匹配、过滤 Sponsored 卡片；0 命中触发商品页兜底
- [x] 7.3 单测：分批切块（100/100/50）与「≤100 不冷却」；冷却可被取消打断
- [x] 7.4 `cargo build` + `cargo test` 通过；`vite build` 通过
- [ ] 7.5 部署到香港服务器，自检状态条确认币种为 JPY（若仍非日元 → 记录，转后续代理 change）
- [ ] 7.6 端到端：对 `B08CKGRHLF` 取到 ¥5,287（日元、非区间）；复制数值含小数场景验证
