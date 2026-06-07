## Why

当前抓取是「一次性把所有 SKU 跑完」：超过 100 条才分批，批间**自动冷却 30 秒倒计时**，且并发为 3。用户希望换成更可控、更克制的节奏——**每抓 50 条就停下来等人确认，再点一次才继续**，以便边抓边核价、随时叫停，并降低被反爬的概率。同时，底部的「实时日志」对用户价值不大，反而占用版面，应移除。

## What Changes

- **移除实时日志**：删除「实时日志」卡片及其状态与写入逻辑；抓取反馈改由进度条 + 完成/成功/异常统计 + 状态条承载。
- **串行抓取**：并发固定为 1（串行），节奏维持每条间隔 1.5 秒；移除 UI 上的「并发数」设置项与 `ScrapeOptions.concurrency`。
- **50 条分片 + 暂停等待**：抓取以**每片最多 50 条**推进；**每抓完一片即自动暂停**，必须由用户点击「继续抓取」才进行下一片。
- **手动暂停（边界停）**：抓取进行中用户可「暂停」，在**当前正在抓取的这一条完成后**停止当前片；未抓取的行保持「待处理」，可随后继续。
- **替换原分批冷却**：彻底移除「>100 分批 + 30s 自动冷却」机制（含 `Cooling` 阶段与冷却倒计时提示），由「50 条分片 + 等待用户」取代；保留 1.5s/条限速。
- **软停止语义**：暂停/停止不再把未抓取的行标记为「失败/已取消」，而是保持「待处理」，使其可被后续续抓。
- **全部刷新同样分片暂停**：「全部刷新」与「开始抓取」走同一套 50 条分片 + 暂停编排。

## Capabilities

### New Capabilities
- `scrape-control-ux`: 抓取节奏与控制交互——串行 1.5s/条、每 50 条一片、片末自动暂停、继续/手动暂停（边界停）、软停止不污染未抓行、全部刷新走同一编排、进度与状态展示（不含实时日志）。

### Modified Capabilities
<!-- openspec/specs/ 基线含 sku-parsing / proxy-settings / price-review-ux；其中无「分批/冷却/并发/日志」相关需求（这些仅存在于已归档的 harden-price-scraping，未进基线），故本 change 以新能力 scrape-control-ux 承载，无 MODIFIED delta。 -->

## Impact

- **改动文件（Rust）**：
  - `config.rs`：移除 `BATCH_SIZE`、`BATCH_COOLDOWN_SECS`（或改用途为前端分片不再需要）；保留 `DEFAULT_REQUEST_INTERVAL_MS`。
  - `models.rs`：`ScrapeOptions` 去掉 `concurrency`；`ScrapeProgress` 去除 `Cooling` 相关字段（`batch_index`/`batch_total`/`cooldown_secs`）与 `ScrapePhase::Cooling`。
  - `scraper.rs`：`scrape_rows` 简化为「串行抓入参这一片、条间检查停止标志」；删除分批循环、`cooldown_between_batches`、`emit_cooling_progress`；停止时未抓行**保持 Pending**（删除/改写 `mark_one_cancelled` 的「标失败」行为）；并发固定为 1。
  - `service.rs`：`start_scrape` 接收前端切好的「一片 ≤50」；`cancel_scrape` 语义改为「软停止当前片」。
- **改动文件（前端）**：
  - `App.tsx`：移除实时日志卡片、`logs`/`setLogs`、`cooldownMessage` 与 `Cooling` 分支；移除并发数输入；新增抓取状态机（idle/running/paused/done）、分片编排（`CHUNK_SIZE=50`）、按钮「开始抓取 / 继续抓取(剩 N) / 暂停」，`handleRefreshAll` 改为分片编排；进度基于 `rows` 计算。
  - `types.ts`：`ScrapeOptions` 去 `concurrency`；`ScrapeProgress` 同步去冷却字段。
- **不改**：`sku.rs`、取价逻辑（搜索优先）、代理、核价抽屉/已查看。
- **风险**：串行 + 50 条一片，1000 条需点约 20 次「继续」，且每片约 75 秒——属预期的「人在环路」节奏；`cancel_flag` 语义变更需同步审视所有调用点。
