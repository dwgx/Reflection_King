# 下一个 Agent 交接文档

更新时间：2026-06-16<br>
仓库：`https://github.com/dwgx/Reflection_King`  
当前主分支：`master`  
当前公网服务：按部署环境设置 `RK_BASE_URL` 或 `--base-url`，不要依赖历史 IP。

本文档用于让下一个 Agent 不依赖聊天窗口也能理解 Reflection King 当前状态、真实目标、
已验证能力、未完成工作和必须遵守的 workflow。

## 资料来源

本交接基于以下证据整理：

- 当前仓库代码和文档。
- 最近提交历史。
- GitHub Actions 当前状态。
- VPS 当前 `/api/health`。
- `docs/evidence/platform-smoke-2026-06-12.md` 里的真实 smoke 记录。
- 子代理只读审查结果。
- 2026-06-16 网页归档、后台解压浏览、缓存清理和 review 修复记录：
  `docs/evidence/page-archive-cache-2026-06-16.md`。

用户曾提到一个长聊天记录文件：

```text
D:\Project\Reflection_King\2026-06-11-110052-dprojectreflectionking-codex-m.txt
```

本轮在仓库根目录、`D:\Project` 和 `C:\Users\dwgx1` 检索时没有找到该文件。
因此不要声称已经读过它；如后续找到，应再补充本交接。

## 最初目标

Reflection King 的核心目标是：

把公开可访问、用户有权访问的网页或媒体 URL，通过爬虫式发现、候选资源选择、
下载、合并、转码或 remux，生成由我们自己的服务器托管的媒体文件，并提供可被外部播放器直接访问的
raw URL，例如 VRChat 视频播放器。

用户一开始明确要做的是：

- Bilibili 解析。
- YouTube 解析。
- SoundCloud 解析。
- 后续扩展到 Douyin、Kuaishou、AcFun、Youku、iQIYI、TikTok、Hanime1、
  MacCMS/动漫资源站、通用未知网页。
- 不是只保存原始 URL，而是把资源拉到自己的服务器，转成稳定可播放的文件。
- 输出 URL 要能给外部播放器直接播放，尤其要考虑 VRChat。
- 要有数据库，不要只靠内存队列。
- 要有前端控制台，让用户能看任务、候选资源、产物、系统状态、密钥和 Profile。
- 要能部署到 VPS，并能通过公开 GitHub 仓库匿名 HTTPS clone/pull。
- 要有 Docker 部署和一键安装命令。
- 文档要中文，GitHub 仓库要让别人能直接理解和部署。

用户对工作方式的要求：

- 没有结论不停止工作。
- 不许猜测不存在的问题或能力。
- review 时必须基于文件、日志、测试、API 合约或真实证据。
- 编码必须保持高质量，能验证就验证。
- 平台支持必须用真实候选、真实转码产物、Range/VRChat 检查证明。
- 失败、待适配、实验性平台要明确标注，不要装作成功。

## 当前已经做到什么

### 2026-06-16 新增：网页归档、后台浏览和缓存维护

本轮完成的核心工作：

- `outputs: ["page_html"]` 不再只是单 HTML 预览，而是网页前端包：
  `index.html`、`index.inline.html`、`page.html`、`page.txt`、
  `screenshot.png`、`resources.json`、`archive.zip`，并可选保存
  `archive.mhtml`、`archive.har`、`archive.warc`。
- Playwright sidecar 增加 CDP 网络捕获，记录 request/response、initiator、
  frame URL、redirect chain、cache/service worker 标记，并在字节预算内缓存
  CSS/JS/image/font/manifest/wasm 响应体。
- `resources.json` 记录归档资源溯源：原始 URL、最终 URL、文档/资源 origin、
  same-origin、method/status/content type、initiator、frame、redirect chain、
  capture source、本地路径、跳过原因等。
- 新增 `GET /api/jobs/{id}/archive/tree` 和
  `GET /api/jobs/{id}/archive/file?path=<relative-path>`，用于后台查看解压后的
  `page/` 目录。文件读取只允许规范化相对路径。
- Dashboard 的归档资源“打开”使用带 `x-api-key` 的 authenticated fetch +
  blob URL，不再用裸 `<a href>` 直接访问受鉴权 API。
- 高级设置新增网页归档开关和预算：CDP 捕获、MHTML/HAR/WARC 保存、CDP 单响应
  MB、CDP 总响应 MB、缓存清理最小小时。
- 高级设置新增缓存面板：查看 public artifacts、temporary jobs、browser profiles
  占用；支持 cleanup preview 和 confirm cleanup。
- 缓存清理只删除旧临时目录和 orphan public artifact 目录；不会删除 browser
  profiles、数据库历史、known job public artifacts 或活跃 job 的 tmp 目录。

本轮 review 已修复两个 P2：

- 归档资源裸链接不带 `x-api-key` 导致 authenticated dashboard 用户 401。
- cache cleanup 会误删长时间运行中的 `storage/tmp/<job_id>`。

本轮本地验证：

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm.cmd --prefix apps/reflection-dashboard run build
npm.cmd --prefix services/reflection-browser run check
npm.cmd --prefix services/reflection-browser run build
git diff --check
```

本轮已按用户要求把 Rust 独立安装到 `D:\Software\Rust`，没有自动修改 PATH。
本地验证使用：

- `RUSTUP_HOME=D:\Software\Rust\rustup`
- `CARGO_HOME=D:\Software\Rust\cargo`
- `D:\Software\Rust\cargo\bin\cargo.exe`
- `D:\Software\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat`

`scripts/check.ps1` 已更新为能自动发现上述本机 Rust 和 VS Community 路径，
并补齐 sidecar build、dashboard build、shell 语法检查（存在 Bash 时）和
`git diff --check`。

直接 sidecar smoke 使用临时 Profile 抓取 `https://example.com/`，确认 HTML、
MHTML、HAR 均返回且 warnings 为空。临时 Profile 已清理。

未完成验证：

- 当前 Windows 本机仍没有 Docker、WSL，因此未能本地运行 Docker build 或
  Compose health。
- 下一位 Agent 若要复验本轮本地代码，可直接运行：

```powershell
.\scripts\check.ps1
```

### 后端

已实现：

- Rust workspace：`reflection-core`、`reflection-api`、`reflection-worker`。
- Axum API。
- SQLite 任务、候选、产物、隐藏历史和 API key 存储。
- 启动恢复未完成任务。
- 直接 URL 下载、ffmpeg 转码、remux、MP4 faststart。
- `/media/{job-id}/{file}` raw URL。
- `HEAD` 和单段 HTTP Range 支持。
- SSRF、私网、重定向目标校验。
- 下载大小和外部工具超时限制。
- 管理密钥和用户密钥。
- 用户密钥权限：浏览器探测、yt-dlp、外部适配器、Profile 登录。
- 任务列表隐藏/恢复：清空 UI 不删除数据库历史。
- 已知坏候选后端防线：failed、DRM、region_blocked、expired、suspect_ad 不允许手动选择。

### 媒体发现

已实现：

- `direct`：直接媒体 URL。
- `external`：`yt-dlp`、`you-get`、`streamlink` 候选发现。
- `browser`：Playwright sidecar 浏览器探测。
- `auto`：按当前配置组合多条链路。
- Bilibili 页面候选和 DASH 音视频分离处理。
- Hanime1 mobile HTML 专用 extractor。
- MacCMS `player_aaaa` episode 页面 extractor。
- iQIYI 假候选过滤和不可复放 manifest 失败分类。
- 浏览器 Profile Cookie/Header 回放。
- yt-dlp 委托下载 fallback，用于 raw CDN URL 直接 ffmpeg 失败但 yt-dlp 可下载的场景。
- 候选评分：清晰度、输出类型、MP4 兼容性、音频伴随、授权需求、失败原因、广告风险。

### 浏览器 sidecar 和 Profile

已实现：

- Playwright sidecar。
- 服务器端持久 Profile。
- Cookie JSON 导入。
- 管理页远程浏览器会话：
  - 截图显示。
  - 鼠标左键/右键/中键点击。
  - 滚轮。
  - 输入文字。
  - Enter/Escape/Tab 等按键。
  - 放大比例。
  - Resize。
  - 导航和关闭会话。
- 浏览器 Cookie 保存在服务器 Profile，不通过公开任务或候选 API 返回 Cookie 明文。

当前注意：

- 不再支持本机协议处理器和 PowerShell 登录助手。
- 不应公开 CDP、VNC、Playwright sidecar 或调试端口。
- Profile 能提高部分站点成功率，但不是绕过验证码、付费墙、DRM 或区域限制的方法。

### 前端控制台

已实现：

- 中文控制台。
- 创建解析任务。
- 自定义来源 URL、解析方式、清晰度、站点、输出类型、授权模式。
- 任务列表分页、隐藏历史、恢复。
- 任务详情。
- 候选资源列表和禁用坏候选。
- 产物播放、复制、打开。
- 系统状态。
- 管理密钥、用户密钥、权限开关。
- 管理页远程浏览器 Profile 登录。
- 帮助页。

当前注意：

- 前端功能可用，但 UI/UX 仍不是最终形态。
- 用户多次要求更现代、更清晰、更少误导，尤其是候选资源、任务详情、布局、控件和分页。
- 后续前端改动必须用真实浏览器截图或 Playwright 检查，不要只靠想象。

### 部署

已实现并验证：

- 公开 GitHub 仓库。
- VPS systemd + nginx 部署。
- 一键安装脚本：

```bash
curl -fsSL https://raw.githubusercontent.com/dwgx/Reflection_King/master/install.sh | sudo bash
```

- 指定公网地址：

```bash
curl -fsSL https://raw.githubusercontent.com/dwgx/Reflection_King/master/install.sh | sudo bash -s -- \
  --public-base-url http://你的服务器IP:8780
```

- Docker Compose 部署：

```bash
git clone https://github.com/dwgx/Reflection_King.git
cd Reflection_King
cp .env.docker.example .env.docker
docker compose --env-file .env.docker up -d --build
```

- Docker 镜像内包含 Rust API、Dashboard 静态资源、Playwright sidecar、Chromium、
  ffmpeg、yt-dlp、you-get、streamlink、SQLite 存储目录。
- VPS 安装脚本默认隐藏初始管理密钥，并写入：
  - `/root/reflection-king-admin-key.txt`
  - `/etc/reflection-king/reflection.env`
- Docker 首次启动默认隐藏初始管理密钥，并写入：
  - `/data/admin-key.txt`
- 只有显式设置 `RK_PRINT_BOOTSTRAP_KEY=1` 时，脚本才会把密钥打印到 stdout。

不要在聊天、提交、文档或日志摘录里公开真实管理密钥。

## 当前验证状态

### GitHub 和 CI

当前规则：

```text
branch: master
status requirement: latest GitHub Actions run on master must be success
local command: git rev-parse --short HEAD
remote command: git ls-remote origin refs/heads/master
vps command: cd /opt/reflection-king && git rev-parse --short HEAD
```

最近一次已确认通过的功能基线：

```text
commit: b3e4533
GitHub Actions run: 27457817046
status: success
```

当前本机最新已确认：

```text
commit: 04f4882
GitHub Actions run: 27593628977
status: success
```

本文档之后的纯文档提交不改变功能基线，但交接时仍要重新核对 `master` 最新 CI、
远端 `origin/master` 和 VPS 当前 HEAD。

CI 覆盖：

- `bash -n install.sh scripts/deploy/*.sh`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- browser sidecar TypeScript check
- dashboard build
- Docker build
- `docker compose --env-file .env.docker.example config`
- `docker compose --env-file .env.docker.example up -d --build`
- Docker 容器启动后访问 `/api/health`

### VPS

VPS 状态未在本轮重新验证。上次记录的 VPS 期望状态：

```text
URL: set RK_BASE_URL or --base-url to the active host
commit requirement: /opt/reflection-king must match origin/master
last functional baseline in this document: b3e4533
reflection-api: active
reflection-browser: active
nginx: active
/api/health: ok
```

如果当前公网端点仍是 HTTP，PC VRChat 可在允许不受信任 URL 的情况下测试；
Android/Quest 或生产使用应配置域名和 HTTPS。

## 平台支持矩阵

下表只按已有 evidence 和 smoke 记录分类。不要把 experimental 当 stable。

| 平台 / 类型 | 当前状态 | 证据和说明 |
| --- | --- | --- |
| 直接媒体 URL | 可用 | 后端基础路径，CI 覆盖相关单元测试。 |
| Bilibili | 可用但高质量依赖 Profile | 公共样本有 MP4 artifact 和 Range 206。未登录时可能只能拿低清，1080p/更高质量需要登录 Profile。 |
| YouTube | 可用样本 | platform smoke 样本通过 yt-dlp 得到 MP4，Range 206。平台规则可能随 yt-dlp 变化。 |
| SoundCloud | 可用样本 | platform smoke 样本输出 MP3，Range 206。browser-only 曾失败，当前主要依赖外部适配器。 |
| AcFun | 可用样本 | 用户样本 `ac48589257` 输出 MP4，VRChat raw URL 检查通过。 |
| Youku | 可用样本 | 用户样本输出 MP4，VRChat raw URL 检查通过。曾有“候选 720p 但产物 540p”的格式选择问题，后续样本达到 720p。仍需持续回归。 |
| TikTok | 可用样本但易变 | yt-dlp raw CDN 403 时用委托下载 fallback；platform smoke 有 MP4/Range 206。 |
| Douyin | experimental | 部分 public short video 通过 browser probing 成功；yt-dlp 常提示 fresh cookies；Profile 不保证稳定。 |
| Kuaishou | experimental | fresh browser candidate 可成功，但 CDN URL 短时效；yt-dlp/you-get/streamlink 不可靠。 |
| iQ.com trailer | experimental | 有 PhantomJS/inline HLS 成功样本，但依赖过时组件，签名和区域容易漂移。 |
| iQIYI 国内 `www.iqiyi.com/v_...` | 未成功 / 待适配 | 浏览器可看到 manifest，但 TS segment replay QWS 403；已标为 failed，不允许选择。需要专用 runtime/signature adapter。 |
| Hanime1 | experimental | 两个用户样本已通过 dedicated extractor 输出 MP4 并通过 VRChat raw 检查；Cloudflare/403 会漂移。 |
| MacCMS / 资源站 | 解析可用，下载取决于 CDN | `dmttang`、`83dm` 能解析 `player_aaaa` 和路线/集数；VPS 侧 CDN 404/403 被标为 `region_blocked`，不能当成功下载。 |
| StreetVoice | 未验证支持 | 代码里有相关注释和测试痕迹，但当前仓库没有真实 smoke evidence。不要宣称已支持。 |
| Pornhub/成人站、广告强站 | 未稳定支持 | 用户曾反馈广告/skip ad/假候选问题。当前不要宣称通用支持，必须逐站做证据。 |

## 已知关键问题

1. HTTPS 未完成。  
   公网仍是 HTTP，输入管理密钥和 VRChat Quest/Android 生产使用都应优先配置 HTTPS。

2. iQIYI 国内页仍缺专用适配器。  
   当前不是 Cookie/Header 简单回放能解决，证据显示 QWS 403，需要 runtime signature 或平台专用链路。

3. Douyin/Kuaishou 不稳定。  
   它们依赖 fresh cookies、challenge state、短时效 CDN URL 和浏览器 Profile。必须继续用真实 smoke 跟踪。

4. MacCMS/资源站需要更强 episode adapter。  
   现在能解析路线和集数，但要在候选层记录路线/集数/来源，并在选择前验证 manifest 和首段可复放。

5. 通用未知网页发现还需要 fixture。  
   需要覆盖 DOM media、metadata、preload、performance resources、inline JSON、script URL、manifest。

6. CDP 网络捕获未完成。  
   需要记录重定向链、initiator、小体积 JSON/manifest 片段，并严格限制字节和敏感内容。

7. 任务队列还不是独立 worker 架构。  
   当前是 API 进程内调度和 SQLite 持久记录。未来应实现 lease-based worker。

8. UI/UX 仍需继续打磨。  
   用户要求现代化、清楚、少误导，候选资源和任务详情尤其重要。

## 下一个 Agent 必须遵守的 workflow

开始前：

1. 运行：

```powershell
git status --short
git log --oneline -10
```

2. 先读：

```text
docs/NEXT_AGENT_HANDOFF.md
docs/WORKFLOW.md
docs/evidence/platform-smoke-2026-06-12.md
docs/DEPLOYMENT.md
docs/SECURITY.md
```

3. 核对当前 CI 和 VPS：

```powershell
gh run list --repo dwgx/Reflection_King --limit 5
ssh <已有本机 SSH 配置> "cd /opt/reflection-king && git rev-parse --short HEAD && systemctl is-active reflection-api reflection-browser nginx && curl -fsS http://127.0.0.1:8780/api/health"
```

不要把 SSH key、root 密码、真实管理密钥或 Cookie 值写进聊天。

开发中：

- 不要凭感觉声明平台支持。
- 每个平台变更必须有真实 URL、候选结构、转码结果和失败分类。
- 浏览器发现只生成候选；下载、合并、转码由 Rust 后端统一控制。
- 每个候选 URL 都必须通过 SSRF、私网、重定向和下载限制。
- Cookie/Auth header 不得通过公开任务或候选 API 明文返回。
- 如果站点返回 DRM、区域限制、验证码、登录墙、年龄门槛或付费墙，不要绕过，记录分类。
- 对假候选、广告、短时效 URL、不可复放 manifest，要在后端拦截，不只靠前端隐藏。

提交前：

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm run check  # services/reflection-browser
npm run build  # apps/reflection-dashboard
```

如果改了部署：

```powershell
& 'C:\Program Files\Git\bin\bash.exe' -n install.sh scripts/deploy/*.sh
```

如果改了平台解析：

```powershell
python scripts\smoke\live_smoke.py --base-url $env:RK_BASE_URL --case <case>
python scripts\smoke\vrchat_raw_url_check.py --url "<artifact-url>"
```

提交后：

- 推送 GitHub。
- 等 GitHub CI 变绿。
- 如果涉及 VPS 运行行为，部署到 VPS 并验证 `/api/health`。
- 更新 evidence 或 handoff 文档。

## 禁区

不要做：

- DRM 移除。
- 验证码求解。
- 付费墙、登录墙、年龄门槛、区域限制或访问控制绕过。
- 猜测私有 token 或规避平台限流。
- 把临时可见 URL 写成稳定支持。
- 把浏览器 UI 音效、广告、预热视频、tracking pixel 当成候选媒体。
- 公开真实管理密钥、Cookie、Profile、SQLite 数据库、`.env`、`storage/`。
- 暴露 Playwright sidecar、CDP、VNC 或浏览器调试端口到公网。
- 使用 `git reset --hard`、`git checkout --` 等方式回滚用户未授权改动。

## 推荐下一步

优先级从高到低：

1. 配置 HTTPS。  
   这是公网管理密钥和 VRChat Quest/Android 生产可用性的前置条件。

2. 做“真实平台 smoke 控制面板”。  
   让用户在前端看到每个平台最近一次成功/失败、失败分类、需要 Profile 与否、最近 artifact 检查结果。

3. iQIYI 国内页专用 adapter。  
   目标不是绕过 DRM，而是判断能否合法复放。无法复放就保持 failed 并给出清楚原因。

4. Douyin/Kuaishou fresh Profile 和短时效 URL 流程。  
   探索“发现后立即捕获”的队列路径，减少候选过期。

5. MacCMS episode adapter。  
   记录路线、集数、候选来源、首段验证和地区阻断。不要把 403/404 CDN manifest 交给用户点。

6. 通用网页 discovery fixture 和 CDP 捕获。  
   CDP 捕获已有第一版，下一步要用可控 fixture 和 Rust/Playwright 测试覆盖，
   不要只靠真实站点临时结果。

7. UI 继续重做候选资源体验。  
   候选要让用户看懂：可用、需要授权、地区阻断、DRM、广告、过期、待适配，不能混在一起。

8. 独立 worker / lease 队列。  
   当前 SQLite 已有基础，但多 worker 和崩溃恢复还不是最终生产形态。

9. 网页归档回归测试。
   给 archive tree/file API、authenticated blob 打开、WARC/HAR/MHTML 开关、
   active tmp cleanup skip 增加 CI 覆盖。当前已有部分 Rust 单元测试草案，但本机未跑。

## 给下一个 Agent 的复制提示词

```text
你接手 D:\Project\Reflection_King。先阅读 docs/NEXT_AGENT_HANDOFF.md、
docs/WORKFLOW.md、docs/evidence/page-archive-cache-2026-06-16.md、
docs/evidence/platform-smoke-2026-06-12.md、docs/DEPLOYMENT.md、docs/SECURITY.md。
不要猜测平台支持；只有真实 URL、候选结构、转码产物、
Range/VRChat 检查或 CI/VPS 证据支持时才能写“已完成”。

当前目标是继续把 Reflection King 做成高质量的 Rust 媒体抓取、候选选择、
转码和 raw URL 输出后端。公网服务地址由当前部署环境提供，GitHub 仓库是
https://github.com/dwgx/Reflection_King。不要输出真实管理密钥、Cookie、Profile、
SQLite、storage 或 SSH 私密信息。

开始前运行 git status、查看最近提交、确认 GitHub CI 和 VPS health。
如果改代码，跑 cargo fmt/test/clippy、browser npm check、dashboard npm build。
如果改部署，跑 shell 语法和部署验证。若改平台解析，必须跑 live_smoke 和
vrchat_raw_url_check，并更新 evidence。
如果继续本轮网页归档/缓存工作，先运行 `.\scripts\check.ps1`；当前本机 Rust 在
`D:\Software\Rust`，但 Docker 验证仍需要有 Docker 的环境。

禁区：不绕过 DRM、验证码、付费墙、登录墙、年龄门槛、区域限制或访问控制；
不把浏览器看到的 URL 当成可播放成功；不让 failed/DRM/region_blocked/ad-risk 候选进入转码队列。
```
