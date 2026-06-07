## ADDED Requirements

### Requirement: 多阶段镜像构建
系统 SHALL 提供多阶段 `Dockerfile`：构建阶段分别产出前端 `dist` 与 Rust release 二进制，运行阶段为精简镜像（如 debian-slim）仅包含运行所需文件。

#### Scenario: 构建产出可运行镜像
- **WHEN** 执行镜像构建
- **THEN** 得到一个包含前端静态资源与后端二进制、可直接启动的运行镜像

### Requirement: 单容器托管 SPA 与 API
运行镜像 SHALL 由单个后端进程同时托管 SPA 静态资源与 `/api`，监听容器内固定端口。

#### Scenario: 容器启动后对外服务
- **WHEN** 容器启动
- **THEN** 该进程在配置端口同时提供前端页面与受保护的 `/api` 接口

### Requirement: Compose 编排与反向代理
系统 SHALL 提供 `docker-compose` 编排 `app` 与 `nginx` 两个服务；nginx SHALL 作为反向代理，将外部流量转发到 `app`，并正确透传 SSE（关闭对 `/api/events` 的缓冲）。

#### Scenario: 经 nginx 访问应用
- **WHEN** 外部请求到达 nginx
- **THEN** 请求被代理到 `app`，页面与接口均可正常访问

#### Scenario: SSE 经代理不被缓冲
- **WHEN** 客户端经 nginx 连接 `/api/events`
- **THEN** 进度事件实时透传，不因代理缓冲而延迟或合并

### Requirement: HTTPS 与域名
部署 SHALL 通过 nginx 在 `a.kirineko.tech` 上启用 HTTPS（证书由运维手动申请并挂载）；HTTP SHALL 重定向到 HTTPS，以保证会话 Cookie 的 `Secure` 生效。

#### Scenario: HTTPS 访问
- **WHEN** 用户通过 `https://a.kirineko.tech/` 访问
- **THEN** 使用已挂载证书建立 TLS，应用正常工作且会话 Cookie 带 `Secure`

#### Scenario: HTTP 跳转
- **WHEN** 用户通过明文 HTTP 访问
- **THEN** 被重定向到 HTTPS

### Requirement: 运行时配置
系统 SHALL 通过环境变量提供运行所需配置（至少：登录密码哈希、会话密钥、默认邮编、监听端口），并 SHALL 提供 `.env.example` 说明；缺失关键密钥时 SHALL 启动失败并给出明确错误。

#### Scenario: 缺失关键密钥
- **WHEN** 启动时未提供密码哈希或会话密钥
- **THEN** 服务拒绝启动并打印明确的缺失项错误

#### Scenario: 通过环境变量配置
- **WHEN** 提供完整环境变量
- **THEN** 服务按配置（端口/邮编/密钥）正常启动

### Requirement: 服务器环境下的抓取链路
部署在印尼服务器（与本机网络一致）时，系统 SHALL 沿用"设日本邮编 + `/dp/{ASIN}?th=1`"的已验证链路；抓取速率 MUST NOT 超过 3 条/秒。

#### Scenario: 服务器环境取价
- **WHEN** 在部署环境初始化会话（邮编 `150-0001`）并抓取
- **THEN** 取到日本地区价格，行为与本机一致，速率受限于 ≤3 条/秒
