# 项目工作流

本文件是 Reflection King 开发、审查、部署和证据记录的协作约定。

## 项目目的

Reflection King 是一个受策略约束的媒体抓取、候选选择、转码和 raw URL 输出后端。

当前能力：

- 接收直接媒体 URL，经过 SSRF 和大小校验后下载、转码并提供 Range 媒体 URL。
- 使用 Playwright sidecar 打开用户有权访问的页面，观察网络和播放器状态，生成候选资源。
- 通过 DOM、metadata、anchor、performance resource、inline script、manifest URL 等路径做通用发现。
- 用 SQLite 持久化任务、候选资源、产物、隐藏历史和 API key。
- 提供 React 控制台，用于创建任务、查看候选、选择资源、播放产物和管理密钥。
- 支持 `yt-dlp`、`you-get`、`streamlink` 等外部适配器。

短期目标：

- 让 Bilibili、Douyin、Kuaishou、AcFun、Youku、iQIYI、SoundCloud、YouTube、
  TikTok、MacCMS 资源站等平台的成功、失败和待适配状态都有明确证据。
- 对候选资源做更严格的可复放验证，避免广告、假资源、区域限制或 DRM manifest 被当成可用视频。
- 让 VPS 一键部署、Docker Compose 和 GitHub CI 都能复现最小可运行环境。

明确非目标：

- DRM 移除。
- 验证码求解。
- 付费墙、登录墙、年龄门槛、区域限制或访问控制绕过。
- 猜测私有 token、规避平台限流或隐藏真实失败原因。
- 没有证据就宣称某个平台“已支持”。

## 证据规则

- 没有构建和验证过的能力，不写成“已完成”。
- 来自代码检查的结论要给文件或函数。
- 来自真实测试的结论要记录命令、URL 类型、候选结构和产物检查。
- 推理必须标注为推理，并说明缺少什么证据。
- review 只报告有文件、函数、API 合约、测试或日志支撑的问题。
- 不臆造未来 bug。把已确认缺陷、风险和开放问题分开写。

## 编码规则

- 保持直接 URL 路径可用，不因新增浏览器探测破坏基础下载转码。
- discovery 只生成候选；下载、合并和转码由 Rust 后端统一执行。
- 每个候选 URL 必须通过和用户输入 URL 同等级别的安全策略。
- 浏览器 sidecar 不得通过公开任务或候选 API 返回 Cookie/Auth header 明文。
- `node_modules`、`target`、`storage`、浏览器 Profile、Cookie、日志和生成媒体不得提交。
- 行为变化要同步更新 README、部署文档、API 示例或证据文档。

## 提交前检查

本地优先运行：

```powershell
.\scripts\check.ps1
```

当前 CI 覆盖：

- `bash -n install.sh scripts/deploy/*.sh`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- 浏览器 sidecar TypeScript 检查
- Dashboard 构建
- Docker build
- `docker compose config`
- `docker compose up` 后访问 `/api/health`

如果某项检查无法运行，必须在交接或最终说明里写明缺失依赖和错误文本。

## Review 工作流

用户要求 review 时：

1. 先读变更代码和相关文档。
2. 按严重性列出已确认问题。
3. 尽量给出文件和行号。
4. 只有缺测试和具体风险有关时才列为发现。
5. 没有发现问题时，说明剩余风险和未跑检查。

不要把没有证据的猜测包装成确定问题。

## Git 和 GitHub

- `master` 必须保持可构建。
- 一次提交只做一个清晰主题。
- 不提交密钥、Cookie、Profile、数据库、产物、依赖目录或本地日志。
- 公开仓库可以在服务器上匿名 HTTPS clone/pull：

```bash
git clone https://github.com/dwgx/Reflection_King.git /opt/reflection-king
cd /opt/reflection-king
git pull --ff-only
```

私有仓库需要 deploy key、token 或 SSH key。不要使用密码交互，也不要提交凭据。

## 部署工作流

VPS 推荐一键安装：

```bash
curl -fsSL https://raw.githubusercontent.com/dwgx/Reflection_King/master/install.sh | sudo bash -s -- \
  --public-base-url http://你的服务器IP:8780
```

已有部署更新：

```bash
cd /opt/reflection-king
sudo git fetch origin master
sudo git pull --ff-only origin master
sudo RK_PUBLIC_BASE_URL=http://你的服务器IP:8780 \
  APP_DIR=/opt/reflection-king \
  bash scripts/deploy/linux-install-services.sh
```

Docker 部署：

```bash
git clone https://github.com/dwgx/Reflection_King.git
cd Reflection_King
cp .env.docker.example .env.docker
docker compose --env-file .env.docker up -d --build
```

如果部署期间曾共享 root 密码，部署后应切换到 SSH key 并轮换密码。

## 下一批里程碑

1. 为通用未知网页 discovery 增加自动 fixture：DOM media、metadata、preload、performance resources、inline JSON、script URL、manifest。
2. 增加 CDP 网络捕获，记录重定向链、initiator 和受限体积的 JSON/manifest 片段。
3. 增加 HLS/DASH manifest 解析器，包含子 URL SSRF 校验、variant 元数据、保护标记和分片/时长硬限制。
4. 持续完善站点专项适配器，并把失败分类写入 evidence 文档。
5. 为候选选择、Range 媒体响应、Docker 启动和 VPS 一键部署增加更强回归测试。
