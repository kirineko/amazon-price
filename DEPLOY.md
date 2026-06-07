# Web 部署指南（a.kirineko.tech）

架构：**宿主机 nginx（TLS 由 certbot 配置）→ 127.0.0.1:9080 → docker compose app**

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

| 变量 | 首次部署（HTTP） | certbot 完成后 |
|---|---|---|
| `PORT` | `9080` | 不变 |
| `APP_PASSWORD_HASH` | 必填 | 不变 |
| `STATIC_DIR` | `/app/dist` | 不变 |
| `SECURE_COOKIES` | **`false`** | **`true`** |
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

## 3. 配置 nginx（HTTP）

```bash
sudo cp deploy/nginx.host.conf.example /etc/nginx/sites-available/amazon-price
sudo ln -sf /etc/nginx/sites-available/amazon-price /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx
```

确认 upstream 端口与 `.env` 中 `PORT` 一致（默认 `9080`）。

```bash
curl -I http://a.kirineko.tech/
```

---

## 4. certbot 申请证书并启用 HTTPS

使用 nginx 插件，**证书路径由 certbot 自动写入 nginx 配置**，无需手动指定：

```bash
sudo certbot --nginx -d a.kirineko.tech
```

按提示选择是否将 HTTP 重定向到 HTTPS（建议选 **Redirect**）。

certbot 会直接修改 `/etc/nginx/sites-available/amazon-price`，添加 `listen 443 ssl` 及默认 Let's Encrypt 证书引用。

完成后：

```bash
# .env 设置 SECURE_COOKIES=true
docker compose up -d

curl -I https://a.kirineko.tech/
```

续期测试：

```bash
sudo certbot renew --dry-run
```

> **注意**：服务器上的 nginx 配置已被 certbot 改过，后续 `git pull` 后不要用仓库里的 example 覆盖它。若需调整反代规则，请直接编辑服务器上的文件，或改 example 后手动 merge 到现有配置。

---

## 5. 验收

```bash
curl -s -o /dev/null -w "%{http_code}\n" \
  https://a.kirineko.tech/api/session \
  -X POST -H 'Content-Type: application/json' -d '{}'
# 应 401

curl -s -c /tmp/cookies.txt -X POST https://a.kirineko.tech/api/login \
  -H 'Content-Type: application/json' -d '{"password":"你的密码"}'
```

浏览器访问 `https://a.kirineko.tech/` 登录并测试抓取。

---

## 6. 更新部署

```bash
git pull
docker compose build
docker compose up -d
```

nginx 配置若已被 certbot 管理，一般无需重新 copy example。

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

## 配置文件

| 文件 | 用途 |
|---|---|
| `deploy/nginx.host.conf.example` | 初始 HTTP 反代；certbot `--nginx` 会在此基础上自动加 HTTPS |
