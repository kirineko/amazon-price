## 1. 数据模型与配置清理（Rust）

- [x] 1.1 `models.rs`：`ScrapeOptions` 移除 `concurrency` 字段（保留 `zip_code`、`request_interval_ms`），同步 `Default`
- [x] 1.2 `models.rs`：移除 `ScrapePhase::Cooling`（`ScrapePhase` 仅剩 `Scraping`，或整体移除 `phase`）；`ScrapeProgress` 移除 `batch_index`/`batch_total`/`cooldown_secs`
- [x] 1.3 `config.rs`：移除 `BATCH_SIZE`、`BATCH_COOLDOWN_SECS`（保留 `DEFAULT_REQUEST_INTERVAL_MS`）

## 2. 抓取引擎串行化与软停止（Rust）

- [x] 2.1 `scraper.rs`：`scrape_rows` 移除 `enable_batching` 参数与分批循环，改为「串行抓入参这一片」
- [x] 2.2 `scraper.rs`：并发固定为 1（`Semaphore(1)` 或顺序 `for`），保留 1.5s/条 `RateLimiter`
- [x] 2.3 `scraper.rs`：条与条之间检查 `cancel_flag`；为真则停止处理本片剩余行并返回
- [x] 2.4 `scraper.rs`：软停止语义——停止/未抓的行**保持 `RowStatus::Pending`**，删除/改写 `mark_one_cancelled` 与 `scrape_batch` 内「标 `Failed`/已取消」的分支
- [x] 2.5 `scraper.rs`：删除 `cooldown_between_batches` 与 `emit_cooling_progress`

## 3. 服务层接线（Rust）

- [x] 3.1 `service.rs`：`start_scrape` 适配「接收一片 ≤50 行」（移除 `enable_batching`/并发入参），返回该片结果
- [x] 3.2 `service.rs`：`cancel_scrape` 语义更新为「软停止当前片」（注释与行为对齐）
- [x] 3.3 检索全部 `cancel`/`mark_one_cancelled` 调用点，确认无残留「标失败」路径；`refresh_one` 单条路径不受影响

## 4. 前端类型与 API

- [x] 4.1 `types.ts`：`ScrapeOptions` 去 `concurrency`；`ScrapeProgress` 去 `batchIndex`/`batchTotal`/`cooldownSecs`，`ScrapePhase` 去 `Cooling`
- [x] 4.2 `api.ts`：确认 `startScrape(chunk, options)` 可传任意长度（前端切片≤50），`cancelScrape` 复用为暂停

## 5. 前端抓取状态机与分片编排

- [x] 5.1 `App.tsx`：新增 `CHUNK_SIZE = 50` 与状态机 `scrapeState: idle|running|paused|done` + `pauseRequested`
- [x] 5.2 `App.tsx`：`scrapeNextChunk()`——取 `rows` 中 `status==="Pending"` 的前 50 条 → `startScrape` → 合并结果 → 若仍有 Pending 则进入 `paused`，否则 `done`
- [x] 5.3 `App.tsx`：`handleStart` 改为「重置 viewed + 把可抓行全部置 Pending + 抓第 1 片」
- [x] 5.4 `App.tsx`：「继续抓取（剩 N 条）」按钮 → `scrapeNextChunk()`
- [x] 5.5 `App.tsx`：「暂停」按钮（running 时可见）→ `pauseRequested=true` + `cancelScrape()`
- [x] 5.6 `App.tsx`：`handleRefreshAll` 改为「全部可抓行置 Pending → 走同一编排」
- [x] 5.7 `App.tsx`：运行中禁用 开始/全部刷新/解析/上传

## 6. 前端进度与日志移除

- [x] 6.1 `App.tsx`：进度改为基于 `rows` 派生（total=非 FormatError；done=非 FormatError 且非 Pending）
- [x] 6.2 `App.tsx`：移除进度监听中的 `Cooling` 分支与 `cooldownMessage`；保留逐行更新 `rows`
- [x] 6.3 `App.tsx`：删除「实时日志」卡片、`logs`/`setLogs` 状态与全部写入点
- [x] 6.4 `App.tsx`：移除「并发数」`InputNumber`；状态条/按钮区展示「抓取中 / 已暂停（剩 N）/ 已完成」

## 7. 验证

- [x] 7.1 `cargo test` + `cargo build` 通过
- [x] 7.2 `npm run build`（tsc）通过
- [x] 7.3 手动验收：抓 50 条自动暂停；点「继续」抓下一片；运行中「暂停」当前条后停且未抓行保持待处理、可续抓；全部刷新走同一分片暂停；界面无实时日志、无并发设置、无 30s 冷却；进度跨片累计不归零
