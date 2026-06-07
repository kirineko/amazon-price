# Amazon Price Scraper

跨平台桌面程序，用于批量解析内部 SKU、抓取 Amazon.co.jp 商品页 buybox 现价（日元），并支持结果刷新与 CSV 导出。

## 功能

- 支持多行文本输入或上传 `.txt` 文件
- SKU 解析规则：`gx-b0dfxqwpps149` → dp code `b0dfxqwpps` → ASIN `B0DFXQWPPS`
- 自动初始化日本地区会话（默认邮编 `150-0001`，可配置）
- 直达商品页 `https://www.amazon.co.jp/dp/{ASIN}?th=1` 抓取价格
- 频率控制：默认最多 3 条/秒，支持并发
- 结果字段：SKU、dp code、ASIN、价格(JPY)、Amazon 链接、状态、抓取时间
- 支持单条刷新、全部刷新、CSV 导出（UTF-8 BOM）

## 技术栈

- Tauri 2 + React + TypeScript
- Rust: `reqwest`、`scraper`、`governor`、`tokio`

## 开发

```bash
npm install
npm run tauri dev
```

## 构建

```bash
npm run tauri build
```

产物说明：

- **Windows**: NSIS 安装包 + 便携版（portable）
- **macOS**: `.dmg` / `.app`（未签名版本首次打开需右键“打开”）

## 使用说明

1. 在左侧输入框粘贴 SKU，或上传 txt 文件
2. 点击“解析 SKU”
3. 确认配送邮编（默认东京 `150-0001`）
4. 点击“开始抓取”
5. 可在结果表中单条刷新或全部刷新，并导出 CSV

## 合规与频率

- 默认限速 **≤ 3 条/秒**，请勿提高频率
- 仅用于自用价格采集，请遵守 Amazon 使用条款
- 程序以游客态抓取公开页面，不依赖登录

## 测试

```bash
cd src-tauri
cargo test
```

网络集成测试（需可访问 Amazon.co.jp）：

```bash
cd src-tauri
cargo test -- --ignored --nocapture
```

## 样例数据

项目根目录 `ids.txt` 提供 6 条测试 SKU。

## macOS 未签名说明

若使用未签名 dmg，首次运行请在“系统设置 → 隐私与安全性”中允许，或右键应用选择“打开”。
