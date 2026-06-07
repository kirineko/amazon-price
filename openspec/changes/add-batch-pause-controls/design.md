## Context

抓取内核与现状（关键代码）：

- `scraper.rs::scrape_rows(session, rows, request_interval_ms, concurrency, cancel_flag, on_progress, enable_batching)`：当前把 `rows` 按 `BATCH_SIZE=100` 分批，批间 `cooldown_between_batches`（`BATCH_COOLDOWN_SECS=30` 倒计时，可被 `cancel_flag` 打断），每批内 `scrape_batch` 用 `Semaphore(concurrency=3)` 并发 + `RateLimiter`（1.5s/token, burst 1）。被取消或未完成的行经 `mark_one_cancelled` 标为 `RowStatus::Failed`「已取消」。
- `service.rs`：`start_scrape` 一次性收全部 `rows`，跑完写 `state.last_rows` 并返回；`refresh_all` 从 `last_rows` 重抓；`cancel_scrape` 设 `cancel_flag=true`。
- 前端 `App.tsx`：`handleStart` `await startScrape(rows, options)` 一次性等全部返回；进度监听里有 `Cooling` 分支（设 `cooldownMessage`）与 `setLogs`；底部「实时日志」卡片；控制区有「并发数」`InputNumber` 与「取消」按钮。
- `models.rs`：`ScrapeOptions { zip_code, request_interval_ms, concurrency }`；`ScrapeProgress { done, total, row, phase, batch_index, batch_total, cooldown_secs }`；`ScrapePhase { Scraping, Cooling }`。

用户已确认方向：**B（前端编排、后端每次只抓一片≤50）+ 串行（并发=1）+ 手动暂停按边界停 + 完全替换分批冷却 + 全部刷新同样分片**。

## Goals / Non-Goals

**Goals:**
- 抓取以「每片 ≤50 条」推进，**每片完成即停**，等用户点「继续」才抓下一片。
- 串行（并发 1）、维持 1.5s/条节奏，使「正好停在第 50 条」精确。
- 用户可在片内「手动暂停」，当前正在抓的**这一条**完成后停止当前片；未抓行保持「待处理」可续抓。
- 「全部刷新」复用同一分片暂停编排。
- 移除实时日志；抓取反馈用进度条 + 统计 + 状态条。

**Non-Goals:**
- 不引入后端长驻任务/事件驱动暂停（不走方向 A）。
- 不做片内「立即丢弃式急停」（停止以「当前条完成」为界）。
- 不改取价逻辑、代理、SKU 解析、核价抽屉/已查看。
- 分片大小本次写死 50，不做成可配置。

## Decisions

### 决策 1：前端编排分片，后端每次只抓一片（方向 B）
- 前端常量 `CHUNK_SIZE = 50`。编排函数 `scrapeNextChunk()`：从 `rows` 取「可抓取且 `status === "Pending"`」的前 50 条，调用 `startScrape(chunk, options)`，await 返回后把结果合并回 `rows`（按 `asin` 覆盖）。
- 后端 `start_scrape` 不再关心分批：收到多少就串行抓多少。前端保证每次 ≤50。
- **为什么**：与「每次只抓 50」语义同构；复用现有阻塞式命令，几乎不动引擎；状态留在前端，简单可控。
- **备选**：方向 A（后端 pause/resume 事件驱动）——命令语义需从「返回结果」改为「fire-and-forget + 事件」，错误处理/最终落库全要重审，改动大、风险高，已排除。

### 决策 2：串行（并发固定 1）
- 移除 `ScrapeOptions.concurrency` 与 UI 并发输入；`scrape_rows` 内 `Semaphore` 固定为 1（或改为顺序 `for` 循环），`RateLimiter` 维持 1.5s/token。
- **为什么**：串行才能保证「正好抓满 50 就停」与「手动暂停停在当前条」精确；并发 >1 时停点会有 1~2 条在途的歧义。
- **影响**：50 条一片约 75 秒；这是预期的人在环路节奏。

### 决策 3：每片完成即「自动暂停」
- `scrapeNextChunk()` 返回后，只要仍有「待处理」行，状态机进入 `paused`，**不自动**取下一片。
- 用户点「继续抓取」再调用一次 `scrapeNextChunk()`。
- 无剩余可抓行时进入 `done`。

### 决策 4：手动暂停 = 软停止当前片（边界=当前条）
- 运行中点「暂停」→ 前端置 `pauseRequested=true` 并调用 `cancelScrape()`（复用 `cancel_flag`）。
- 后端 `scrape_rows` 在**每条抓取之间**检查 `cancel_flag`：为真则停止处理本片剩余行并返回；**已抓的行带结果，未抓的行原样保持 `Pending`**。
- 前端合并结果后进入 `paused`；未抓行仍 `Pending`，「继续」时被下一片重新纳入。
- **为什么**：用户选了「边界停」；在串行下「当前条完成即停」≈即时，且让「手动暂停」相对「片末自动暂停」具有真实增量价值（不必等满 50）。
- **关键语义变更**：`cancel_flag` 从「取消并把未抓行标 `Failed`/已取消」改为「**软停止，未抓行保持 Pending**」。删除/改写 `mark_one_cancelled` 的标失败行为；`scrape_batch` 内「取消时把行标 Failed」的分支同样改为「保持 Pending 不写失败」。

### 决策 5：彻底移除分批冷却
- 删除 `cooldown_between_batches`、`emit_cooling_progress`、`config::BATCH_SIZE`/`BATCH_COOLDOWN_SECS`、`scrape_rows` 的 `enable_batching` 参数与分批循环。
- `models.rs` 移除 `ScrapePhase::Cooling` 及 `ScrapeProgress` 的 `batch_index`/`batch_total`/`cooldown_secs`；`ScrapePhase` 仅剩 `Scraping`（或一并移除 `phase` 字段）。
- 前端移除 `Cooling` 分支与 `cooldownMessage`。

### 决策 6：进度统计基于 rows 计算（跨片累计）
- 分片后单次 `startScrape` 的 `payload.total` 只是「本片大小」，不能作全局总数。
- 前端进度改为基于 `rows` 派生：
  - `total = rows.filter(s => s.status !== "FormatError").length`（可抓取总数）
  - `done = rows.filter(s => s.status !== "FormatError" && s.status !== "Pending").length`（已出结果数）
- 进度事件仍用于**逐行实时更新** `rows`（每条抓完即刷新该行状态/价格）。

### 决策 7：前端抓取状态机与按钮
```
状态：idle → running → paused → running → … → done
                         ▲                      │
                         └──── 仍有 Pending ─────┘

按钮（随状态切换）：
- idle / done：  「开始抓取」（重置 viewed，把可抓行全部置 Pending，抓第 1 片）
- running：      「暂停」（pauseRequested + cancelScrape；当前条完成后停）
- paused：       「继续抓取（剩 N 条）」（抓下一片） + 「结束」(可选：清 Pending→idle)
- 全部刷新：     等价于「把全部可抓行重置 Pending → 走开始编排」
- 导出 CSV：     不变
```
- 原「取消」按钮由「暂停/继续」取代。
- `running` 期间禁用「开始/全部刷新/解析/上传」，避免并发编排。

### 决策 8：last_rows 与 refresh_all
- 前端持有完整 `rows`，分片编排完全在前端；`handleRefreshAll` 改为「把全部可抓行置 Pending → `scrapeNextChunk` 循环（受暂停控制）」，不再依赖后端 `refresh_all`。
- 后端 `refresh_all` 命令可保留（暂不删，前端不再调用），或后续清理；`start_scrape` 是否继续写 `state.last_rows` 不影响前端流程（保留无害）。

## Risks / Trade-offs

- [`cancel_flag` 语义变更影响所有调用点] → 全量检索 `cancel`/`mark_one_cancelled`，确保「软停止=未抓保持 Pending」，且无残留「标失败」路径；`refresh_one` 等单条路径不受影响。
- [串行 50 条/片约 75s，大批量需多次点击] → 属设计意图（人在环路、降低反爬）；UI 用「剩 N 条」「本片 x/50」给足反馈。
- [进度改为基于 rows 派生] → 需确保每条抓完都更新了 rows 状态（成功/失败/无价等均非 Pending），否则 done 不前进；对「Mismatch/NoPrice/Unavailable/Failed」都计入 done。
- [移除 ScrapeProgress 冷却字段是结构变更] → 前后端同步修改 `types.ts`/`models.rs`，避免序列化字段不匹配。

## Migration Plan

1. `models.rs`：去 `concurrency`、去 `Cooling` 与冷却字段；`types.ts` 同步。
2. `config.rs`：移除 `BATCH_SIZE`/`BATCH_COOLDOWN_SECS`。
3. `scraper.rs`：`scrape_rows` 改为串行抓入参一片、条间检查 `cancel_flag` 软停止、未抓保持 Pending；删分批/冷却/emit_cooling；并发=1。
4. `service.rs`：`start_scrape` 收一片即可；`cancel_scrape` 注释/语义更新为软停止。
5. 前端 `App.tsx`：移除日志卡片与 logs/cooldown；移除并发输入；加状态机 + `CHUNK_SIZE=50` 编排 + 暂停/继续按钮；进度基于 rows；`handleRefreshAll` 改分片编排。
6. 验证：`cargo test` + `cargo build` + `npm run build`；手动验收见 tasks。
- **回滚**：改动集中在 `scraper.rs`/`models.rs`/`service.rs`/`config.rs` 与 `App.tsx`/`types.ts`，可整体回退到当前「分批冷却 + 并发」版本。

## Open Questions

- 「结束」按钮是否需要？（暂停态长期不继续即可达到「停下」，本设计将「结束」设为可选：清空剩余 Pending 回到 idle。）
- 是否要把片大小 50 做成可配置？（本次写死，后续按需。）
- 后端 `refresh_all` 命令是保留还是删除？（本次保留、前端不调用，留待清理。）
