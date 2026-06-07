# sku-parsing Specification

## Purpose
TBD - created by archiving change harden-sku-parsing. Update Purpose after archive.
## Requirements
### Requirement: 行预处理
系统 SHALL 对每一行原始 SKU 文本先做去除首尾空白处理，并 SHALL 跳过空行（不产生任何结果行）。

#### Scenario: 去除首尾空白
- **WHEN** 输入行为 `"  gx-b0dfxqwpps149  "`
- **THEN** 解析前先 trim 为 `"gx-b0dfxqwpps149"`

#### Scenario: 跳过空行
- **WHEN** 输入文本中存在空行或仅含空白的行
- **THEN** 该行被跳过，不计入结果行，也不计入去重/非法统计

### Requirement: gx- 前缀剥离
系统 SHALL 在派生 ASIN 前，剥离开头的 `gx-` 前缀，且 MUST 大小写不敏感。

#### Scenario: 含小写前缀
- **WHEN** 输入为 `"gx-b0dfxqwpps149"`
- **THEN** 剥离前缀后用于后续处理的主体为 `"b0dfxqwpps149"`

#### Scenario: 含大写前缀
- **WHEN** 输入为 `"GX-B018AOIO1Y150"`
- **THEN** 剥离前缀后主体为 `"B018AOIO1Y150"`

#### Scenario: 无前缀
- **WHEN** 输入为 `"b0dfxqwpps149"`
- **THEN** 主体保持为 `"b0dfxqwpps149"`，不做任何剥离

### Requirement: 长度感知的后缀剥离与 ASIN 派生
剥离 `gx-` 前缀后，系统 SHALL 按长度感知规则派生 10 位 ASIN：当主体为 13 位且末 3 位均为 ASCII 数字时，MUST 仅剥离末 3 位；当主体恰为 10 位时，MUST 原样保留（即使其以数字结尾）；其余长度 MUST 不剥离并交由 ASIN 校验判定。最终 ASIN MUST 为主体的大写形式。

#### Scenario: 13 位带 3 位数字后缀
- **WHEN** 去前缀后主体为 `"b0dfxqwpps149"`（13 位，末 3 位为数字）
- **THEN** 剥离末 3 位得 `"b0dfxqwpps"`，派生 ASIN 为 `"B0DFXQWPPS"`

#### Scenario: 恰好 10 位的裸 ASIN（字母结尾）
- **WHEN** 去前缀后主体为 `"b0dfxqwpps"`（10 位）
- **THEN** 原样保留，派生 ASIN 为 `"B0DFXQWPPS"`，不剥离任何字符

#### Scenario: 恰好 10 位且结尾为数字的裸 ASIN
- **WHEN** 去前缀后主体为 `"4873115655"`（10 位，结尾 3 位为数字）
- **THEN** 原样保留，派生 ASIN 为 `"4873115655"`，MUST NOT 剥离末 3 位

#### Scenario: 13 位但末 3 位非数字
- **WHEN** 去前缀后主体为 `"b0dfxqwppsabc"`（13 位，末 3 位非数字）
- **THEN** MUST NOT 剥离，主体维持 13 位，进入 ASIN 校验（将判定为格式错误）

### Requirement: ASIN 校验与非法标记
系统 SHALL 仅当派生结果为 10 位且全部为字母或数字（`^[A-Z0-9]{10}$`）时视为有效 ASIN；否则 SHALL 将该行标记为 `FormatError` 状态，并保留原始 SKU 文本与可读错误信息。

#### Scenario: 有效 ASIN
- **WHEN** 派生 ASIN 为 `"B0DFXQWPPS"`
- **THEN** 该行有效，状态为 `Pending`，并据 ASIN 生成 Amazon 商品链接

#### Scenario: 长度非法
- **WHEN** 去前缀后主体为 `"abc"`（3 位）或 `"b0dfxqwpps1"`（11 位）
- **THEN** 该行标记为 `FormatError`，错误信息说明"ASIN 需为 10 位字母数字"

#### Scenario: 含非字母数字字符
- **WHEN** 派生主体长度为 10 但包含非字母数字字符（如 `"b0df-xqwpp"`）
- **THEN** 该行标记为 `FormatError`

### Requirement: 按 ASIN 去重
系统 SHALL 以派生出的大写 ASIN 为键去重，重复行 MUST 不再次产生结果行，并 SHALL 累加重复计数返回给调用方。

#### Scenario: 重复 SKU 去重
- **WHEN** 输入文本包含两行解析出相同 ASIN（如 `"gx-b0dfxqwpps149"` 与 `"b0dfxqwpps"`）
- **THEN** 仅保留第一条结果行，重复计数加 1

#### Scenario: 格式错误行不参与去重
- **WHEN** 输入文本包含多条会判定为 `FormatError` 的行
- **THEN** 每条 `FormatError` 行均保留，不因彼此重复而被去除

