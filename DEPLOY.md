# Web 部署指南（a.kirineko.tech）

架构：**宿主机 nginx（TLS 终止）→ 127.0.0.1:9080 → docker compose app**

nginx 分两阶段配置：先 HTTP 跑通并申请 certbot 证书，再切换 HTTPS。

---

## 1. 准备 `.env`

```bash
cp .env.example .env
```

生成密码哈希：

```bash
cd src-tauri
cargo run --example hash_password -- '你的强密码'
```

编辑 `.env`：

| 变量 | 首次部署 | 证书就绪后 |
|---|---|---|
| `PORT` | `9080`（或未被占用的端口） | 不变 |
| `APP_PASSWORD_HASH` | 必填 | 不变 |
| `STATIC_DIR` | `/app/dist` | 不变 |
| `SECURE_COOKIES` | **`false`**（HTTP 阶段） | **`true`** |
| `DEFAULT_ZIP` | `150-0001` | 不变 |

> HTTP 阶段若 `SECURE_COOKIES=true`，浏览器不会保存登录 Cookie。

---

## 2. 启动 Docker

```bash
docker compose build
docker compose up -d
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:9080/
```

应返回 `200`。

---

## 3. 阶段一：配置 HTTP nginx

```bash
sudo cp deploy/nginx.host.http.conf.example /etc/nginx/sites-available/amazon-price
sudo ln -sf /etc/nginx/sites-available/amazon-price /etc/nginx/sites-enabled/
sudo mkdir -p /var/www/certbot
sudo nginx -t && sudo systemctl reload nginx
```

确认 upstream 端口与 `.env` 中 `PORT` 一致（默认 `9080`）。

验证 HTTP 可访问：

```bash
curl -I http://a.kirineko.tech/
```

---

## 4. 使用 certbot 申请证书

**推荐 webroot 方式**（不自动改 nginx 配置）：

```bash
sudo certbot certonly --webroot \
  -w /var/www/certbot \
  -d a.kirineko.tech
```

按提示填写邮箱并同意条款。成功后证书位于：

```
/etc/letsencrypt/live/a.kirineko.tech/fullchain.pem
/etc/letsencrypt/live/a.kirineko.tech/privkey.pem
```

续期（certbot 定时任务通常已配置，可手动测试）：

```bash
sudo certbot renew --dry-run
```

---

## 5. 阶段二：切换 HTTPS nginx

```bash
sudo cp deploy/nginx.host.https.conf.example /etc/nginx/sites-available/amazon-price
sudo nginx -t && sudo systemctl reload nginx
```

更新 `.env` 并重启 app：

```bash
# .env 中设置 SECURE_COOKIES=true
docker compose up -d
```

验证：

```bash
curl -I http://a.kirineko.tech/          # 应 301 到 https
curl -I https://a.kirineko.tech/       # 应 200

# API 未登录应 401
curl -s -o /dev/null -w "%{http_code}\n" \
  https://a.kirineko.tech/api/session \
  -X POST -H 'Content-Type: application/json' -d '{}'
```

浏览器访问 `https://a.kirineko.tech/` 登录测试。

---

## 6. 更新部署

```bash
git pull
docker compose build
docker compose up -d
# nginx 配置无变更则无需 reload
```

---

## 7. 本地开发

**终端 1：**

```bash
export APP_PASSWORD_HASH='...'
export STATIC_DIR=dist
export SECURE_COOKIES=false
export PORT=8080
npm run build && npm run start:web
```

**终端 2：**

```bash
npm run dev:web   # http://localhost:1420
```

---

## 配置文件说明

| 文件 | 用途 |
|---|---|
| `deploy/nginx.host.http.conf.example` | 阶段 1：仅 HTTP，供 certbot 验证 |
| `deploy/nginx.host.https.conf.example` | 阶段 2：HTTP→HTTPS 跳转 + TLS 反代 |
