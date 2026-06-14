# 运维手册

本文件记录 Reflection King 在本地、VPS 和 Docker 环境里的日常运维方式。
一键部署和 Docker 启动命令见 [DEPLOYMENT.md](DEPLOYMENT.md)。

## 环境变量

常用配置见 `.env.example`、`.env.docker.example` 和 `/etc/reflection-king/reflection.env`。

关键变量：

```text
RK_API_KEY                         管理密钥。公网部署必须设置。
RK_PUBLIC_BASE_URL                 外部访问地址，用于生成 /media raw URL。
RK_STORAGE_DIR                     任务数据库、产物和临时文件目录。
RK_MAX_DOWNLOAD_MB                 单任务下载上限。
RK_MAX_CONCURRENT_JOBS             并发任务数。
RK_BROWSER_PROBE_URL               浏览器 sidecar 地址，默认 http://127.0.0.1:8791。
RK_BROWSER_PROFILE_ROOT            Playwright Profile 和 Cookie 存储目录。
RK_YTDLP_PATH                      yt-dlp 可执行文件。
RK_YOU_GET_PATH                    you-get 可执行文件。
RK_STREAMLINK_PATH                 streamlink 可执行文件。
```

## 运行时文件

默认路径：

```text
storage/tmp                 临时下载输入
storage/public              对外提供的媒体产物
storage/reflection.db       SQLite 任务、候选和恢复记录
storage/browser-profiles    Playwright 持久 Profile
```

Docker 部署时这些内容位于 `/data` volume。不要把 `storage/`、`/data`、
Cookie JSON、浏览器 Profile、SQLite 数据库或任何 `.env` 文件提交到 GitHub。

## 健康检查

```bash
curl http://127.0.0.1:8787/api/health
```

公网反代部署后：

```bash
curl http://127.0.0.1:8780/api/health
```

该接口会返回服务名、版本、ffmpeg 路径、公开地址、存储路径和数据库路径。

## systemd 服务

VPS 一键安装会创建：

```text
reflection-api.service       Rust API 和任务调度
reflection-browser.service   Playwright 浏览器探测 sidecar
nginx.service                公网反向代理
```

查看状态：

```bash
sudo systemctl status nginx reflection-browser reflection-api
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
sudo git pull --ff-only origin master
sudo RK_PUBLIC_BASE_URL=http://你的服务器IP:8780 \
  APP_DIR=/opt/reflection-king \
  bash scripts/deploy/linux-install-services.sh
```

安装脚本会等待 `http://127.0.0.1:8787/api/health` 变为可用后再打印完成信息。

## Docker 运维

启动：

```bash
docker compose --env-file .env.docker up -d --build
```

查看日志：

```bash
docker compose logs -f reflection-king
```

查看健康状态：

```bash
curl http://127.0.0.1:8780/api/health
```

查看容器内保存的初始管理密钥：

```bash
docker compose exec reflection-king cat /data/admin-key.txt
```

更新：

```bash
git pull --ff-only
docker compose --env-file .env.docker up -d --build
```

## 浏览器 Profile 和 Cookie

推荐路径是管理页里的服务端远程浏览器：

1. 打开 `管理 -> 浏览器账号配置`。
2. 设置 `Profile ID`，例如 `admin_default`。
3. 输入目标站点地址并启动服务端浏览器会话。
4. 在网页截图控制器里点击、输入或扫码登录。
5. 关闭会话。Cookie 会保存在服务器 Profile 目录，用于后续浏览器探测和 Header 回放。

同一张管理卡片也支持 Cookie JSON 导入。把浏览器插件导出的 Cookie JSON 数组粘贴到
`Cookie JSON`，再导入到目标 Profile ID。

如果 Windows 本机已经登录 Edge、Chrome 或 Firefox，可以用本地 Python 导入器只抽取指定站点
Cookie 并上传到服务器 Profile：

```powershell
python -m pip install --user -U yt-dlp browser-cookie3
python scripts/cookies/import_browser_cookies.py `
  --base-url <public-base-url> `
  --api-key "<admin-key>" `
  --browser edge `
  --platform bilibili `
  --profile-id admin_default
```

建议先加 `--dry-run` 确认域名和数量。脚本不会打印 Cookie 值。

不再支持本机协议处理器和 PowerShell 桌面助手。它们在远程 VPS 场景下不稳定，也不利于权限边界。
不要把 CDP、VNC 或浏览器调试端口直接暴露到公网。

## 外部适配器

自动解析会聚合多个候选来源：

```text
RK_YTDLP_PATH=yt-dlp
RK_YOU_GET_PATH=you-get
RK_LUX_PATH=lux
RK_STREAMLINK_PATH=streamlink
RK_EXTERNAL_PROBE_TIMEOUT_SECONDS=45
```

`yt-dlp` 仍是最宽覆盖的第一适配器；`you-get` 对部分中文站点有帮助；
`streamlink` 适合直播和 manifest 场景；`lux` 可在手动安装 Go 二进制后启用。

解析器会去重候选 URL，并记录保护类型、适配路线、置信度、广告风险和验证提示，前端用这些字段区分
“可直接转码”“需要 Cookie/Profile”“区域限制”“DRM/不可复放”“待适配”。

## 备份

VPS：

```bash
sudo systemctl stop reflection-api reflection-browser
sudo tar -czf reflection-king-storage-$(date +%F).tar.gz -C /opt/reflection-king storage
sudo systemctl start reflection-browser reflection-api
```

Docker：

```bash
docker compose stop
docker run --rm -v reflection_king_reflection_data:/data -v "$PWD:/backup" alpine \
  tar -czf /backup/reflection-king-data-$(date +%F).tar.gz -C /data .
docker compose start
```

## VRChat raw URL 自检

对单个产物：

```powershell
python scripts\smoke\vrchat_raw_url_check.py `
  --url "<public-base-url>/media/<job-id>/<artifact>.mp4"
```

对整个任务：

```powershell
$env:RK_API_KEY = "<admin-or-user-key>"
python scripts\smoke\vrchat_raw_url_check.py `
  --base-url "<public-base-url>" `
  --job-id "<job-id>"
```

自检会验证 `HEAD`、`Range: bytes=0-511`、`Accept-Ranges`、MIME、`Content-Length`、
MP4 faststart、H.264/AAC 或 MP3 音频流等关键项。
