# Reflection King

Reflection King 是一个 Rust 媒体抓取、转码和 raw URL 输出后端。目标是把
公开可访问、用户有权限访问的页面或媒体 URL 转成自己的服务器产物，供外部
播放器直接访问，例如 VRChat 视频播放器。

当前能力：

- Axum API + React 控制台。
- SQLite 持久任务队列。
- SSRF 和私网地址拦截。
- ffmpeg 转码、remux、MP4 faststart。
- `/media/{id}/{file}` raw URL，支持 HTTP Range。
- `yt-dlp`、`you-get`、`streamlink` 外部解析。
- Playwright 浏览器探测 sidecar。
- 服务器端浏览器 Profile/Cookie 导入。
- 候选资源评分、失败候选拦截、地区限制/DRM/广告风险标记。
- 部分站点专用适配：Bilibili、Hanime1、MacCMS/资源站页面等。

本项目不会绕过 DRM、付费墙、验证码、登录墙或访问控制。只用于你拥有权利
或已获授权的内容。

## 快速部署

### VPS 一键安装

公开 VPS 上最简单的安装方式：

适用前提：

- Debian 12、Ubuntu 22.04/24.04 或兼容的 apt + systemd 服务器。
- 使用 root 或 sudo 执行。
- 建议至少 2 核 CPU、2 GB 内存、15 GB 可用磁盘。
- 公网防火墙放行 `8780/tcp`，或按 `--port` 指定其他端口。
- 非 Debian/Ubuntu、无 systemd 的容器或轻量系统，建议改用 Docker Compose。

```bash
curl -fsSL https://raw.githubusercontent.com/dwgx/Reflection_King/master/install.sh | sudo bash
```

指定公网地址：

```bash
curl -fsSL https://raw.githubusercontent.com/dwgx/Reflection_King/master/install.sh | sudo bash -s -- \
  --public-base-url http://你的服务器IP:8780
```

安装完成后控制台会显示：

```text
Dashboard: http://你的服务器IP:8780
Admin key: <初始管理密钥>
Admin key file: /root/reflection-king-admin-key.txt
```

### Docker Compose

```bash
git clone https://github.com/dwgx/Reflection_King.git
cd Reflection_King
cp .env.docker.example .env.docker
docker compose --env-file .env.docker up -d --build
docker compose logs -f reflection-king
```

第一次启动会在日志里打印管理密钥，并保存到 Docker volume 的
`/data/admin-key.txt`。

更多部署细节见 [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)。

## 本地开发

Windows 本地开发：

```powershell
cd D:\Project\Reflection_King
copy .env.example .env
.\scripts\dev\bootstrap.ps1
.\scripts\dev\run-local.ps1
```

打开：

```text
http://localhost:8787
```

Linux 本地或 VPS 手动部署：

```bash
git clone https://github.com/dwgx/Reflection_King.git /opt/reflection-king
cd /opt/reflection-king
sudo bash scripts/deploy/linux-bootstrap.sh
sudo RK_PUBLIC_BASE_URL=http://你的服务器IP:8780 \
  APP_DIR=/opt/reflection-king \
  bash scripts/deploy/linux-install-services.sh
```

## 仓库结构

```text
crates/reflection-core       配置、模型、任务存储、URL 安全、下载、转码
crates/reflection-api        Axum HTTP API、队列调度、媒体服务、Dashboard 静态资源
crates/reflection-worker     未来独立 worker 入口
services/reflection-browser  Playwright 浏览器探测 sidecar
apps/reflection-dashboard    React 控制台
docs/                        架构、安全、部署、媒体管线和 smoke 证据
config/                      策略和转码配置示例
scripts/                     开发、部署、cookie 导入和 smoke 脚本
tests/                       集成测试说明和未来 fixture
```

## API 示例

创建任务：

```bash
curl -X POST http://127.0.0.1:8787/api/jobs \
  -H 'content-type: application/json' \
  -H 'x-api-key: <管理密钥>' \
  -d '{
    "url": "https://example.com/video.mp4",
    "discovery": "auto",
    "platform_hint": "auto",
    "outputs": ["video"],
    "bitrate": "auto"
  }'
```

查看候选资源：

```bash
curl -H 'x-api-key: <管理密钥>' \
  http://127.0.0.1:8787/api/jobs/<job-id>/candidates
```

提交候选：

```bash
curl -X POST http://127.0.0.1:8787/api/jobs/<job-id>/select-candidates \
  -H 'content-type: application/json' \
  -H 'x-api-key: <管理密钥>' \
  -d '{"candidate_ids":["<candidate-id>"]}'
```

## 运维命令

```bash
sudo systemctl status nginx reflection-browser reflection-api
sudo journalctl -u reflection-api -f
sudo journalctl -u reflection-browser -f
curl http://127.0.0.1:8787/api/health
```

## 安全

- 不要把 `.env`、`.env.docker`、`reflection.env`、`storage/`、Cookie
  JSON、浏览器 Profile、SQLite 数据库提交到 GitHub。
- 默认公开部署会启用管理密钥。不要无密钥公网部署。
- 不要公开 Playwright sidecar、CDP、VNC 或调试端口。
- `RK_PUBLIC_BASE_URL` 必须是外部客户端能访问的地址，否则 `/media/...`
  raw URL 只能在本机使用。
- 生产环境建议配置 HTTPS，特别是需要在控制台输入管理密钥时。

更多内容见 [docs/SECURITY.md](docs/SECURITY.md) 和
[docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)。
