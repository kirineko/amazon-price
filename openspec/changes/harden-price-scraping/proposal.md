## Why

应用部署到香港服务器后，价格抓取暴露三个问题：（1）Amazon.co.jp 按访客 IP 本地化货币，香港出口 IP 拿到的是换算后的外币而非日元——实测搜索页对 `B08CKGRHLF` 返回 `USD 36.20`（≈¥5,287），但当前解析逻辑会兜底拼成一个错误的小额「日元」值；（2）商品页 URL 缺 `psc=1`，多变体商品 buybox 显示区间而非定价，且实测商品页价格（￥5,665）与真实 buybox（¥5,287）不符；（3）前端复制价格时正则把小数点一起吃掉。此外大批量抓取缺少宏观限速，数据中心 IP 容易触发反爬。

实测还发现：用搜索页 `/s?k={asin}` 取价比商品页更准（命中真正的 buybox 特价、返回单一价而非区间），但**搜索页同样按 IP 本地化货币**——因此「锁定日元」与「换用搜索页取价」是两件必须同时做的正交工作。

## What Changes

- **货币锁定为日元**：用持久化 cookie jar 预置 `i18n-prefs=JPY` + `lc-acbjp=ja_JP`，会话初始化后重新写回以抵御 Amazon 的 `Set-Cookie` 覆盖，并在商品/搜索请求上再次显式断言，配合现有日本配送地址（`150-0001`）。
- **取价主路径改为搜索页 + 商品页兜底**：优先 `GET /s?k={asin}`，精确匹配 `data-asin` 的结果卡片（自动过滤 Sponsored 广告），读取其价格；搜索 0 命中时回退到商品页 `GET /dp/{asin}?th=1&psc=1`（新增 `psc=1`）。
- **币种校验（非日元快速失败）**：解析时读取页面**真实**货币符号（而非系统自己拼接的字符串），仅当为日元（全角 `￥`）才计为成功；检测到外币则**不重试**、直接标记失败并在错误信息中写明真实币种；`RowResult.currency` 字段真正填入检测结果（此前恒为 `"JPY"`）。
- **大批量分批限速**：实际抓取条数 > 100 时按每批 100 条切块，块间冷却 30 秒（可被取消立即打断）；抓取进度暴露「批次/冷却」阶段，供前端展示。
- **自检暴露币种**：`self_check` 返回检测到的币种与样例价，会话初始化的状态条据此即时显示「日元锁是否生效」，便于远程部署后判断是否需要日本代理。
- **复制/导出保留小数**：前端复制数值时保留小数点（防御性，纵使锁定日元后通常为整数）。

## Capabilities

### New Capabilities
- `price-scraping`: 价格抓取的取价正确性与稳健性契约——日元货币锁定、取价来源（搜索优先 + 商品页兜底）、币种校验与失败语义、大批量分批限速与冷却、抓取进度/自检对币种与批次状态的可观测性、价格数值复制/导出的小数保真。

### Modified Capabilities
<!-- openspec/specs/ 基线仅含 sku-parsing；web-api/web-auth/deployment 仍在未归档的 migrate-to-web-app change 中、尚未进入基线，故本 change 不对其做 MODIFIED delta，相关取价行为统一收敛到新能力 price-scraping。 -->

## Impact

- **改动文件（Rust）**：
  - `region.rs`：`cookie_store(true)` → `cookie_provider(Arc<Jar>)` 并预置/重置 JPY 偏好；新增搜索页抓取与解析；`extract_price` 返回真实币种；商品页 URL 兜底带 `psc=1`。
  - `config.rs`：`product_url` 加 `psc=1`；新增搜索 URL 构造；新增批量大小/冷却时长常量（写死 100 / 30s）。
  - `models.rs`：`RowResult.currency` 真正赋值；`ScrapeProgress` 增加批次/冷却阶段字段。
  - `scraper.rs`：`scrape_rows` 增加分批循环 + 可取消的 30s 冷却；非日元快速失败（跳过重试）；`self_check` 改为币种感知。
  - `service.rs`：接线（进度阶段、币种透传）。
- **改动文件（前端）**：`src/api.ts`/`src/types.ts`（进度阶段字段、币种字段）；`src/App.tsx`（复制保留小数、冷却期 UI 提示、币种展示）。
- **依赖**：复用 `reqwest`（cookie jar）、`scraper`、`tokio`；预计无新增 crate。
- **风险/前置**：海外 IP 下 cookie 不一定 100% 压住货币本地化；本 change 先走「cookie 锁 + 币种校验 + 自检暴露」，若部署后自检显示仍非日元，再以独立 change 引入日本节点代理（保留 `reqwest` 代理入口）。
- **不改动**：`sku.rs`（解析能力由 `harden-sku-parsing` 负责）；认证/部署相关代码。
