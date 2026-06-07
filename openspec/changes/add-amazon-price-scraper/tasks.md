## 1. 项目脚手架与依赖

- [x] 1.1 用 `create-tauri-app` 初始化 Tauri 2 + React + TypeScript 项目
- [x] 1.2 添加 Rust 依赖：`reqwest`(cookies/gzip/brotli/json)、`scraper`、`tokio`、`governor`、`serde`/`serde_json`、`anyhow`/`thiserror`
- [x] 1.3 添加前端依赖：UI 组件库（Ant Design 或 shadcn/ui）、CSV 导出库、轻量状态管理
- [x] 1.4 配置 `tauri.conf.json`（窗口尺寸、应用标识、Windows 便携版/portable 产物）

## 2. SKU 输入解析（sku-input）

- [x] 2.1 实现 dp code/ASIN 解析：去前缀 `gx-`（忽略大小写）、去末尾 3 位、大写得 ASIN
- [x] 2.2 实现按行解析：trim、跳过空行
- [x] 2.3 实现去重与 ASIN 格式校验（10 位字母数字），非法项标记"格式错误"
- [x] 2.4 实现 `.txt` 文件读取，与文本输入统一解析路径
- [x] 2.5 单元测试：用 `ids.txt` 覆盖正常/前缀容错/非法格式/重复

## 3. 日本地区会话（region-session）

- [x] 3.1 构建 `reqwest::Client`：启用 `cookie_store`、gzip/brotli 解压、超时、默认桌面 Chrome header
- [x] 3.2 预置 `lc-acbjp=ja_JP`、`i18n-prefs=JPY`，GET 首页获取 `session-id` 等 cookie
- [x] 3.3 从首页 HTML 解析 `glowValidationToken`（= `anti-csrftoken-a2z`）
- [x] 3.4 `POST /gp/delivery/ajax/address-change.html`（form 编码 + token header）设置邮编，校验响应 `"successful":1`
- [x] 3.5 邮编可配置：默认 `150-0001`，校验格式 `\d{3}-\d{4}`
- [x] 3.6 实现会话刷新（每批次前）与地区回退检测（配送地非目标或大面积无价 → 重设）
- [x] 3.7 集成测试：设置后 GET 首页确认 `glow-ingress-line2` 为目标地区

## 4. 抓取引擎（price-scraping）

- [x] 4.1 构造 `https://www.amazon.co.jp/dp/{ASIN}?th=1` 请求，复用地区会话 cookie
- [x] 4.2 `governor` 令牌桶限速（≤3/s，可配）+ `tokio` 并发池（默认 3）+ 50–150ms 随机抖动
- [x] 4.3 失败/无价指数退避重试（0.8s/1.6s/3.2s，最多 3 次）+ 高失败率自动降速
- [x] 4.4 价格解析：取第一个非空 `span.a-offscreen`（含 `￥`）→ 去 `￥`/逗号转整数日元，保留原始字符串
- [x] 4.5 兜底选择器：`#corePriceDisplay_desktop_feature_div`/`#corePrice_feature_div` → `a-price-whole`+`a-price-fraction`
- [x] 4.6 ASIN 校验（页面 `data-asin`/canonical）与状态判定：成功/不可售/无价/失败/疑似不匹配
- [x] 4.7 集成测试：`ids.txt` 6 条期望 6/6 取到价（设东京地区）

## 5. IPC 命令与数据模型

- [x] 5.1 定义 `RowResult { sku, dpCode, asin, amazonUrl, priceText, priceValue?, currency, status, error?, fetchedAt }`（serde）
- [x] 5.2 实现 Tauri command：`init_session(zip)`、`parse_skus(text|path)`、`start_scrape(rows,opts)`、`refresh_one(asin)`、`refresh_all()`、`export_csv(rows)`
- [x] 5.3 抓取进度事件 `app.emit("scrape-progress", {done,total,row})`
- [x] 5.4 前端 `invoke` 封装与事件订阅

## 6. 前端 UI（desktop-shell + results-management）

- [x] 6.1 输入区：多行文本框 + 文件拖拽上传 + "识别到 N 条有效 SKU"提示
- [x] 6.2 控制区：开始/暂停/取消、并发与速率可调、邮编设置入口
- [x] 6.3 进度区：进度条 + 成功/失败计数 + 实时日志
- [x] 6.4 结果表：SKU/dp code/ASIN/价格(JPY)/Amazon 链接/状态/时间，支持排序、筛选、状态着色
- [x] 6.5 动态刷新：单条"刷新"按钮 + 顶部"全部刷新"
- [x] 6.6 CSV 导出（UTF-8 BOM，含全部字段）
- [x] 6.7 响应式布局与窗口缩放自适应

## 7. 健壮性与可维护性

- [x] 7.1 将选择器与接口参数集中到配置/常量模块（便于 Amazon 结构变更时集中维护）
- [x] 7.2 启动自检：抓取一个已知 ASIN 验证整条链路可取价
- [x] 7.3 错误日志与用户可读错误提示（区分网络/反爬/不可售）
- [x] 7.4 （可选）代理配置入口，默认关闭

## 8. 打包与发布

- [x] 8.1 Windows 构建：NSIS 安装包 + 便携版（portable）
- [x] 8.2 macOS 构建：dmg（含签名/公证说明，未签名时提供右键打开指引）
- [x] 8.3 GitHub Actions 矩阵构建（windows-latest / macos-latest）产出双平台产物
- [x] 8.4 编写 README：使用说明、邮编配置、频率与合规（≤3/s、自用、遵守条款）说明

## 9. 验收

- [x] 9.1 用 `ids.txt` 端到端验收：设东京 `150-0001`，期望 6/6 取到正确日元价
- [x] 9.2 验证 CSV 导出内容完整、Excel 可正常打开
- [x] 9.3 验证动态刷新（单条/全部）正确更新价格与状态
- [x] 9.4 验证抓取速率不超过 3 条/秒
