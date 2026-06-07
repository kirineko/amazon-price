## 1. 重写解析核心

- [x] 1.1 在 `src-tauri/src/sku.rs::parse_dp_code` 中用长度感知规则替换"无条件砍末 3 位"：去 `gx-` 前缀后，仅当主体为 13 位且末 3 位为 ASCII 数字时砍末 3 位；10 位主体原样保留；其余长度交由校验判定
- [x] 1.2 保持 `dpCode` = 剥离后小写主体、`asin` = 其大写形式，复用现有 `is_valid_asin`（`^[A-Z0-9]{10}$`）
- [x] 1.3 确认 `parse_skus_from_text` 的去重（按 ASIN）、空行跳过、`FormatError` 行保留逻辑无回归

## 2. 单元测试

- [x] 2.1 新增用例：裸 ASIN `b0dfxqwpps`（字母结尾）→ `B0DFXQWPPS`，不被砍
- [x] 2.2 新增用例：结尾为数字的 10 位 ASIN `4873115655` → 原样保留，不被砍
- [x] 2.3 新增用例：大写前缀 `GX-B018AOIO1Y150` → `B018AOIO1Y`
- [x] 2.4 新增用例：13 位末 3 位非数字 `b0dfxqwppsabc` → `FormatError`
- [x] 2.5 新增用例：11/12 位等非法长度 → `FormatError`
- [x] 2.6 保留并通过既有用例：`gx-b0dfxqwpps149` → `B0DFXQWPPS`、`ids.txt` 6 条仍为 6 条有效

## 3. 验收

- [x] 3.1 运行 `cargo test -p amazon-price-scraper` 全绿
- [x] 3.2 用 `ids.txt` 走一遍解析，结果与变更前一致（带后缀场景无差异）
- [x] 3.3 手测混合输入（前缀/无前缀/裸 ASIN/结尾数字/非法长度）解析结果符合 spec 场景
