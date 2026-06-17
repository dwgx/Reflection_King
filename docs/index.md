# Reflection King 文档索引

优先阅读：

- [架构说明](ARCHITECTURE.md)
- [安全边界](SECURITY.md)
- [部署指南](DEPLOYMENT.md)
- [运维手册](OPERATIONS.md)
- [媒体管线](MEDIA_PIPELINE.md)
- [VRChat 播放](VRCHAT_PLAYBACK.md)
- [路线图](ROADMAP.md)
- [工作流](WORKFLOW.md)
- [下一个 Agent 交接文档](NEXT_AGENT_HANDOFF.md)
- [媒体抓取设计](crawler/media-acquisition-design.md)
- [爬虫后端调研 2026-06-09](research/crawler-backend-survey-2026-06-09.md)
- [通用媒体发现调研 2026-06-09](research/generic-media-discovery-survey-2026-06-09.md)

验证证据：

- [Bilibili 公开探测证据](evidence/bilibili-public-probe-2026-06-09.md)
- [通用浏览器探测证据](evidence/generic-browser-discovery-2026-06-09.md)
- [SoundCloud / YouTube 公开探测证据](evidence/soundcloud-youtube-public-probe-2026-06-09.md)
- [外部 yt-dlp 探测证据](evidence/external-yt-dlp-probe-2026-06-09.md)
- [平台 smoke 证据](evidence/platform-smoke-2026-06-12.md)
- [网页归档与缓存维护证据](evidence/page-archive-cache-2026-06-16.md)
- [仓库与 Agent 工作痕迹审计 2026-06-17](evidence/repository-ai-audit-2026-06-17.md)
- [VPS 临时 Docker smoke 2026-06-17](evidence/vps-smoke-2026-06-17.md)
- [公开源 catalog smoke 2026-06-17](evidence/public-source-catalog-smoke-2026-06-17.md)

目录说明：

- `architecture/`：组件边界和系统视图。
- `adr/`：架构决策记录。
- `api/`：公开 API 和错误模型。
- `crawler/`：媒体抓取和候选资源设计。
- `evidence/`：真实站点 smoke、候选和失败原因记录。
- `media/`：下载、转码、直播和输出格式。
- `operations/`：部署、备份、日志和运行维护。
- `policies/`：版权、授权和 URL 抓取策略。
- `research/`：外部项目和通用方案调研。
- `security/`：SSRF、威胁模型和安全要求。
- `testing/`：测试策略。
