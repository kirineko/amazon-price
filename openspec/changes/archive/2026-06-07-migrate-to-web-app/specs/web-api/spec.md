## ADDED Requirements

### Requirement: 会话初始化端点
系统 SHALL 提供 `POST /api/session`，复用现有 `AmazonSession` 逻辑设置日本配送地区（默认邮编 `150-0001`，可传入），返回 `SessionStatus`（含配送地与可读消息）。

#### Scenario: 初始化成功
- **WHEN** 已认证客户端 `POST /api/session`（可带邮编）
- **THEN** 服务端设置日本地区会话并返回 `initialized=true` 与配送地信息

#### Scenario: 网络/地区设置失败
- **WHEN** 连接 Amazon 或设置地区失败
- **THEN** 返回可读错误信息（区分超时/连接/反爬），不崩溃

### Requirement: SKU 解析端点
系统 SHALL 提供 `POST /api/skus/parse`，接收多行 SKU 文本，复用 `sku` 解析能力返回 `(rows, duplicateCount)`，数据形状与现有 `RowResult` 一致。

#### Scenario: 解析多行文本
- **WHEN** 已认证客户端提交多行 SKU 文本
- **THEN** 返回解析后的结果行数组与去重计数

### Requirement: 抓取端点
系统 SHALL 提供 `POST /api/scrape`，接收 `rows` 与可选 `options`（速率/并发），复用 `ScrapeEngine` 执行抓取并返回最终 `RowResult[]`；抓取过程中 SHALL 通过进度通道实时广播每条结果。

#### Scenario: 批量抓取返回结果
- **WHEN** 已认证客户端 `POST /api/scrape` 提交解析后的行
- **THEN** 服务端按限速/并发抓取，过程产生实时进度，结束返回全部结果行

#### Scenario: 速率约束
- **WHEN** 未显式指定速率/并发
- **THEN** 使用默认值（≤3 条/秒、并发 3），且抓取速率 MUST NOT 超过 3 条/秒

### Requirement: 刷新与取消端点
系统 SHALL 提供刷新（单条/全部）与取消端点：`POST /api/scrape/refresh`（按行或全部重抓）、`POST /api/scrape/cancel`（置取消标志，使进行中的抓取尽快停止）。

#### Scenario: 全部刷新
- **WHEN** 已认证客户端请求全部刷新且存在上一批结果
- **THEN** 重新抓取这些行并返回更新后的结果

#### Scenario: 取消进行中的抓取
- **WHEN** 抓取进行中收到取消请求
- **THEN** 取消标志置位，剩余未完成行尽快以"已取消"结束

### Requirement: CSV 导出端点
系统 SHALL 提供 `POST /api/export.csv`，复用现有 CSV 生成逻辑（UTF-8 BOM、含全部字段）返回 CSV 内容供前端下载。

#### Scenario: 导出 CSV
- **WHEN** 已认证客户端提交结果行请求导出
- **THEN** 返回带 BOM、含全部字段的 CSV 文本

### Requirement: 自检端点
系统 SHALL 提供 `GET /api/self-check`，复用 `self_check` 抓取一个已知 ASIN 验证整条链路可取价，返回 `SelfCheckResult`。

#### Scenario: 自检通过
- **WHEN** 已认证客户端请求自检且链路正常
- **THEN** 返回 `ok=true` 与测试商品价格

### Requirement: SSE 实时进度
系统 SHALL 提供 `GET /api/events`（Server-Sent Events），向已认证客户端推送抓取进度事件 `{done,total,row}`，语义与原 `scrape-progress` 一致。

#### Scenario: 订阅进度
- **WHEN** 已认证客户端在抓取开始前建立 SSE 连接
- **THEN** 抓取期间持续收到每条完成行的进度事件，可据此更新表格与进度条

#### Scenario: 未认证订阅被拒
- **WHEN** 无有效会话的客户端尝试连接 `/api/events`
- **THEN** 连接被拒绝（401）

### Requirement: SPA 静态托管与前端接线
系统 SHALL 由同一服务进程托管前端构建产物（SPA），对未匹配 `/api/*` 的路径回退到 `index.html`；前端 SHALL 用 `fetch` 调用 REST、用 `EventSource` 消费 SSE，并在未登录时引导至登录页、登录后访问主界面。

#### Scenario: 直接访问前端路由
- **WHEN** 浏览器请求非 `/api` 路径（含刷新子路由）
- **THEN** 服务返回 SPA 的 `index.html`，由前端路由接管

#### Scenario: 未登录访问主界面
- **WHEN** 未登录用户打开应用
- **THEN** 前端展示登录页；登录成功后进入抓取主界面

#### Scenario: 会话失效的前端处理
- **WHEN** 受保护请求返回 401
- **THEN** 前端清理本地登录态并引导用户重新登录
