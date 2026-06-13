# 架构说明

Reflection King 当前是一个可单机部署的 Rust 媒体抓取与转码后端。它把 API、持久队列、
候选资源发现、下载转码和静态媒体服务放在同一个部署单元里，后续可以拆出独立 worker。

## Crate 划分

- `reflection-core`：运行配置、API 模型、SQLite 任务存储、URL 安全策略、下载、转码、存储路径和共享错误。
- `reflection-api`：Axum HTTP 服务、持久任务注册、进程内调度、worker loop、健康检查和 `/media` Range 服务。
- `reflection-worker`：未来独立任务消费者入口，目前是占位 crate。

## 当前流程

```text
POST /api/jobs
  -> 校验 API key
  -> 规范化解析方式、站点、输出类型和清晰度
  -> 写入 SQLite 任务
  -> 进入本地调度队列
  -> 直接 URL / 外部适配器 / 浏览器探测返回候选资源
  -> 用户或自动策略选择候选
  -> 下载、remux 或 ffmpeg 转码
  -> 发布 /media/<job-id>/<artifact> raw URL，并支持 HTTP Range
```

API 默认把任务记录保存在 `storage/reflection.db`。启动时会恢复未完成任务，并保留隐藏任务历史。
当前 dispatcher 仍在 API 进程内运行；如果要横向扩展，下一步应做基于 lease 的队列表和独立 worker。

## 媒体抓取方向

抓取逻辑分成两层：

- 发现层：识别页面、平台、manifest、直接文件、图片、音频、视频、广告风险、区域限制和 DRM 风险，返回候选资源。
- 捕获层：只对通过安全策略和可复放检查的候选执行下载、合并或转码。

所有候选 URL 都是不可信输入，必须和用户直接提交的 URL 一样经过 SSRF、重定向、大小和内容类型校验。

详见：

- [媒体抓取设计](crawler/media-acquisition-design.md)
- [爬虫后端调研](research/crawler-backend-survey-2026-06-09.md)

## 主要组件

- `source_resolver`：判断输入是直接文件、HLS、DASH、平台页面、直播还是普通网页。
- `external adapters`：调用 `yt-dlp`、`you-get`、`streamlink` 等工具生成候选。
- `browser sidecar`：用 Playwright 打开页面，监听网络、脚本数据和播放器状态。
- `candidate scorer`：按类型、清晰度、码率、可复放状态、广告风险、授权需求给候选排序。
- `capture`：下载单文件、HLS/DASH 分片，或合并音视频分离资源。
- `processing`：ffmpeg 输出 MP3、AAC、MP4 faststart、缩略图等格式。
- `delivery`：提供无需 API key 的 raw URL、Range、HEAD 和 CORS。
- `policy`：域名策略、授权边界、配额、保留时间和站点限制。

## 部署拓扑

VPS systemd 模式：

```text
Internet
  -> nginx :8780
  -> reflection-api :8787
  -> reflection-browser :8791
  -> storage/reflection.db + storage/public
```

Docker 模式：

```text
reflection-king container
  -> Rust API :8780
  -> Playwright sidecar :8791 (container localhost)
  -> /data SQLite + public media + browser profiles
```

浏览器 sidecar、CDP、VNC 和调试端口不应该直接暴露到公网。
