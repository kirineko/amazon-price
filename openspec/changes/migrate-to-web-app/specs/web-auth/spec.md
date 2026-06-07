## ADDED Requirements

### Requirement: 单密码登录
系统 SHALL 提供 `POST /api/login`，校验请求中的密码与服务端环境变量内的 argon2 密码哈希；校验通过 SHALL 建立登录会话，失败 SHALL 返回 401 且不泄露密码是否存在等细节。

#### Scenario: 密码正确
- **WHEN** 客户端 `POST /api/login` 提交正确密码
- **THEN** 服务端校验通过，返回 200 并下发会话凭据

#### Scenario: 密码错误
- **WHEN** 客户端提交错误密码
- **THEN** 服务端返回 401，响应体为统一的"认证失败"信息，不下发会话凭据

### Requirement: 会话 Cookie
登录成功后系统 SHALL 通过 **httpOnly + SameSite** 的 Cookie 维持登录态；当部署在 HTTPS 下时 MUST 同时设置 `Secure` 标志；会话 MUST 具备过期时间。

#### Scenario: 登录后下发安全 Cookie
- **WHEN** 登录成功
- **THEN** 响应 `Set-Cookie` 含会话标识，并带 `HttpOnly`、`SameSite`（HTTPS 下含 `Secure`）属性

#### Scenario: 会话过期
- **WHEN** 会话超过配置的有效期后再次访问受保护接口
- **THEN** 服务端视为未认证，返回 401

### Requirement: 接口鉴权中间件
系统 SHALL 以中间件保护除 `POST /api/login` 与静态资源外的**所有** `/api/*` 端点；未携带有效会话的请求 MUST 返回 401，MUST NOT 执行任何抓取或数据操作。

#### Scenario: 未认证访问受保护接口
- **WHEN** 无有效会话的请求访问 `/api/scrape`、`/api/skus/parse` 等
- **THEN** 返回 401，且不触发任何 Amazon 请求或状态变更

#### Scenario: 已认证访问受保护接口
- **WHEN** 携带有效会话 Cookie 的请求访问受保护接口
- **THEN** 请求被放行并正常处理

#### Scenario: 登录端点豁免
- **WHEN** 未认证请求访问 `POST /api/login`
- **THEN** 不被中间件拦截，可正常进行登录校验

### Requirement: 登录限流防爆破
系统 SHALL 对登录失败进行限流（如按来源在时间窗内限制尝试次数），超过阈值 SHALL 暂时拒绝继续尝试。

#### Scenario: 连续失败触发限流
- **WHEN** 同一来源在短时间内多次登录失败并超过阈值
- **THEN** 后续登录请求被暂时拒绝（如 429），并在冷却期后恢复

### Requirement: 登出
系统 SHALL 提供登出能力（如 `POST /api/logout`），使当前会话失效；其后使用旧会话访问受保护接口 MUST 返回 401。

#### Scenario: 登出后会话失效
- **WHEN** 客户端调用登出后再用原 Cookie 访问受保护接口
- **THEN** 返回 401
