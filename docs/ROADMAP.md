# 路线图

## Phase 1：基础后端

- Rust workspace 和 Git 根目录。
- Axum API。
- SQLite 持久任务记录。
- 直接 URL 到 MP3/MP4 的任务链路。
- 基础 SSRF 防护。
- 本地存储。
- 单 Range HTTP 媒体服务。

## Phase 2：公网可运行

- 管理密钥和用户密钥。
- 用户密钥权限：浏览器探测、yt-dlp、外部适配器、Profile 登录。
- systemd + nginx VPS 部署。
- Docker Compose 部署。
- 一键安装脚本输出初始管理密钥。
- CI 覆盖 Rust、前端、sidecar 和 Docker build。
- 任务隐藏/恢复，数据库历史保留。

## Phase 3：媒体发现

- 候选资源模型和候选表。
- 直接 URL、外部适配器、浏览器探测统一进入候选选择。
- `yt-dlp --dump-single-json`、`you-get`、`streamlink` 适配。
- Playwright 网络、脚本、播放器状态探测。
- 站点识别、清晰度、码率、广告风险、授权需求和失败原因评分。
- 已知失败候选后端拦截。

## Phase 4：站点专项适配

- Bilibili：登录 Profile 下高质量音视频分离合并。
- Douyin/Kuaishou：服务端远程浏览器 Profile 和真实候选验证。
- YouTube/SoundCloud：公开链接与 yt-dlp 适配更新。
- AcFun/Youku/iQIYI：区分可转码、缺依赖、区域拦截和不可复放。
- MacCMS/资源站：路线、集数、播放页和 CDN 首段验证。
- Hanime1/Cloudflare 类站点：只记录真实可复现结果，不把临时绕过当稳定支持。

## Phase 5：生产级队列和存储

- 独立 worker 进程。
- 基于 lease 的任务认领和重试。
- 下载尝试表、错误分类和站点回归记录。
- 清理调度器和保留策略。
- 用户配额、速率限制和租户隔离。
- S3 兼容存储、CDN、签名 URL。

## Phase 6：播放体验

- VRChat raw URL 自检自动化。
- MP4 faststart、H.264/AAC 兼容性固定检查。
- 缩略图和元数据。
- 长视频分段、直播窗口捕获和续传。
