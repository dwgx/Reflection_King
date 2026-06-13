# 部署指南

本文档面向公开仓库部署。不要把真实管理密钥、Cookie、浏览器
Profile、SQLite 数据库或 `storage/` 提交到 GitHub。

## 方式一：VPS 一键安装

适合长期运行的单机 VPS。脚本会安装系统依赖、Rust、Node、ffmpeg、
Playwright Chromium、yt-dlp、you-get、streamlink，构建前端和后端，
写入 systemd 服务，并配置 nginx 反代到公网端口。

最短命令：

```bash
curl -fsSL https://raw.githubusercontent.com/dwgx/Reflection_King/master/install.sh | sudo bash
```

指定公网地址：

```bash
curl -fsSL https://raw.githubusercontent.com/dwgx/Reflection_King/master/install.sh | sudo bash -s -- \
  --public-base-url http://你的服务器IP:8780
```

指定域名：

```bash
curl -fsSL https://raw.githubusercontent.com/dwgx/Reflection_King/master/install.sh | sudo bash -s -- \
  --public-base-url https://rk.example.com
```

安装完成后控制台会打印：

```text
Reflection King installed.
Dashboard: http://你的服务器IP:8780
Admin key: <初始管理密钥>
Admin key file: /root/reflection-king-admin-key.txt
```

管理密钥也会写在：

```text
/etc/reflection-king/reflection.env
/root/reflection-king-admin-key.txt
```

这两个文件只留在服务器上，不要提交到 GitHub。

## VPS 运维命令

查看服务：

```bash
sudo systemctl status nginx reflection-browser reflection-api
curl http://127.0.0.1:8787/api/health
```

查看日志：

```bash
sudo journalctl -u reflection-api -f
sudo journalctl -u reflection-browser -f
sudo journalctl -u nginx -f
```

重启：

```bash
sudo systemctl restart reflection-browser reflection-api nginx
```

更新到 GitHub 最新版本：

```bash
cd /opt/reflection-king
sudo git fetch origin master
sudo git reset --hard origin/master
sudo RK_PUBLIC_BASE_URL=http://你的服务器IP:8780 \
  APP_DIR=/opt/reflection-king \
  bash scripts/deploy/linux-install-services.sh
```

## 方式二：Docker Compose

适合快速试跑或容器化部署。镜像内包含：

- Rust API
- React Dashboard 静态资源
- Playwright browser sidecar
- Chromium 运行依赖
- ffmpeg
- yt-dlp、you-get、streamlink
- SQLite 存储目录 `/data`

启动：

```bash
git clone https://github.com/dwgx/Reflection_King.git
cd Reflection_King
cp .env.docker.example .env.docker
docker compose --env-file .env.docker up -d --build
docker compose logs -f reflection-king
```

如果只是本机快速试跑，也可以不复制 `.env.docker`，直接运行：

```bash
docker compose up -d --build
```

第一次启动时容器日志会打印管理密钥：

```text
Reflection King starting.
Dashboard: http://localhost:8780
Admin key: <初始管理密钥>
Admin key file: /data/admin-key.txt
```

如果想预先指定密钥，编辑 `.env.docker`：

```env
RK_API_KEY=换成你自己的长随机字符串
RK_PUBLIC_BASE_URL=http://你的服务器IP:8780
```

查看健康状态：

```bash
curl http://127.0.0.1:8780/api/health
```

更新：

```bash
git pull --ff-only
docker compose --env-file .env.docker up -d --build
docker compose logs -f reflection-king
```

查看容器内保存的管理密钥：

```bash
docker compose exec reflection-king cat /data/admin-key.txt
```

## HTTPS

公网输入管理密钥时建议使用 HTTPS。VPS systemd 部署可以用 nginx +
certbot：

```bash
sudo snap install --classic certbot
sudo ln -s /snap/bin/certbot /usr/local/bin/certbot
sudo certbot --nginx
sudo certbot renew --dry-run
```

配置 HTTPS 后，把 `/etc/reflection-king/reflection.env` 里的
`RK_PUBLIC_BASE_URL` 改成 HTTPS 域名，再重启服务：

```bash
sudo systemctl restart reflection-browser reflection-api nginx
```

## 备份

重要数据在 `storage/` 或 Docker volume `/data`：

- `reflection.db`
- `public/` 生成媒体
- `browser-profiles/` 登录 Profile
- `tmp/` 临时文件

VPS 备份：

```bash
sudo systemctl stop reflection-api reflection-browser
sudo tar -czf reflection-king-storage-$(date +%F).tar.gz -C /opt/reflection-king storage
sudo systemctl start reflection-browser reflection-api
```

Docker 备份：

```bash
docker compose stop
docker run --rm -v reflection_king_reflection_data:/data -v "$PWD:/backup" alpine \
  tar -czf /backup/reflection-king-data-$(date +%F).tar.gz -C /data .
docker compose start
```

## 安全边界

- 不要无密钥公网部署。默认安装会启用 `RK_API_KEY`。
- 不要公开 Playwright sidecar 端口、CDP、VNC 或调试端口。
- 不要把 `.env`、`.env.docker`、`reflection.env`、`storage/`、Cookie
  JSON、浏览器 Profile、SQLite 数据库提交到 GitHub。
- `RK_PUBLIC_BASE_URL` 必须是外部客户端能访问的地址，否则生成的
  `/media/...` raw URL 只能在服务器本机用。
- 本项目不会绕过 DRM、付费墙、验证码、登录墙或访问控制。
