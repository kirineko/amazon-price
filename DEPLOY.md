# Web 部署指南（a.kirineko.tech）

架构：**宿主机 nginx（TLS 终止）→ 127.0.0.1:9080 → docker compose app**

compose 不再内置 nginx 容器，也不使用 8080/8000 端口。

## 1. 生成密码哈希

```bash
cd src-tauri
cargo run --example hash_password -- '你的强密码'
```

将输出写入 `.env` 的 `APP_PASSWORD_HASH`。

## 2. 配置 `.env`

```bash
cp .env.example .env
```

| 变量 | 说明 |
|---|---|
| `PORT` | 应用端口，默认 **9080**（映射到 `127.0.0.1:9080`，可改为其他未占用端口） |
| `APP_PASSWORD_HASH` | Argon2 哈希（必填） |
| `STATIC_DIR` | 容器内固定 `/app/dist`，勿改 |
| `SECURE_COOKIES` | 经 HTTPS 访问设为 `true` |
| `DEFAULT_ZIP` | Amazon 日本邮编，默认 150-0001 |

若 9080 也被占用，改 `.env` 中 `PORT=9091` 等，并同步修改宿主机 nginx upstream 端口。

## 3. 构建并启动 Docker

在项目目录：

```bash
docker compose build
docker compose up -d
docker compose ps
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:9080/
```

应返回 `200`（静态页）或 `401`（若直接打 API）。

查看日志：

```bash
docker compose logs -f app
```

## 4. 配置宿主机 nginx

参考 [`deploy/nginx.host.conf.example`](deploy/nginx.host.conf.example)：

```bash
sudo cp deploy/nginx.host.conf.example /etc/nginx/sites-available/amazon-price
# 确认 upstream 端口与 .env 中 PORT 一致（默认 9080）
# 确认 ssl_certificate 路径正确
sudo ln -sf /etc/nginx/sites-available/amazon-price /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx
```

证书若尚未申请（certbot 示例）：

```bash
sudo certbot certonly --nginx -d a.kirineko.tech
# 或 webroot / standalone，按你服务器现有流程
```

## 5. 验收

```bash
# 应 401
curl -s -o /dev/null -w "%{http_code}\n" https://a.kirineko.tech/api/session -X POST -H 'Content-Type: application/json' -d '{}'

# 登录
curl -s -c /tmp/cookies.txt -X POST https://a.kirineko.tech/api/login \
  -H 'Content-Type: application/json' -d '{"password":"你的密码"}'
```

浏览器访问 `https://a.kirineko.tech/`，登录后测试 SKU 解析与抓取。

## 6. 更新部署

```bash
git pull
docker compose build
docker compose up -d
```

## 7. 本地开发

**终端 1（后端，可用 8080）：**

```bash
export APP_PASSWORD_HASH='...'
export STATIC_DIR=dist
export SECURE_COOKIES=false
export PORT=8080
npm run build
npm run start:web
```

**终端 2（前端热更新）：**

```bash
npm run dev:web   # http://localhost:1420，/api 代理到 8080
```

本地开发与生产端口互不影响；生产 compose 默认用 **9080**。

## 8. 桌面版（可选）

```bash
cd src-tauri
cargo build --bin amazon-price-scraper --features desktop
```
