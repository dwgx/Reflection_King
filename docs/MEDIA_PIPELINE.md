# 媒体管线

Reflection King 的媒体管线分为“发现候选资源”和“捕获转码产物”两段。前端展示的是候选资源，
最终给 VRChat 或外部播放器使用的是 `/media/...` raw URL。

## 当前能力

```text
来源 URL
  -> 任务写入 SQLite
  -> 直接 URL / 外部适配器 / 浏览器探测
  -> 候选资源评分与过滤
  -> 下载、remux 或 ffmpeg 转码
  -> storage/public/<job-id>/<artifact>
  -> /media/<job-id>/<artifact>
```

已支持：

- 直接音频、视频、图片和 HLS manifest URL；direct 模式会先生成候选，再走统一候选选择和产物管线。
- HLS manifest 捕获前会解析并校验内部子 URL、初始化片段、密钥 URI 和媒体分片；DASH/MPD 在子 URL 校验闭环前默认拒绝进入 ffmpeg。
- `yt-dlp`、`you-get`、`streamlink` 外部适配器。
- Playwright 浏览器探测页面脚本、播放器信息和网络请求。
- Bilibili 音视频分离候选合并。
- MP3 音频输出。
- MP4 faststart 视频输出。
- 单 Range HTTP 播放探测。

实际可用性取决于来源站点、地区、登录态、签名 URL、DRM 和当前 ffmpeg 构建。

## 输出类型

- `audio_mp3_vrc`：MP3，适合音频播放器和 VRChat 音频使用。
- `audio_aac`：AAC，适合浏览器和视频容器。
- `video_mp4_faststart`：H.264/AAC MP4，`moov` 前置，适合大多数网页和 VRChat PC 播放。
- `image`：图片抓取。
- `page_html`：网页前端包，包含入口 HTML、页面文本、截图、资源清单和 `archive.zip`；zip 内保存已通过 URL 策略与大小限制校验的 CSS、JS、图片、字体和媒体资源。

## 候选资源状态

候选资源不会因为“被发现”就等于可用。后端会尽量标记：

- `ready`：可尝试选择和转码。
- `requires_profile`：需要 Cookie/Profile 或登录态 Header。
- `region_blocked`：区域或 CDN 拦截。
- `drm_or_encrypted`：疑似 DRM、加密分片或平台运行态限制。
- `ad_or_decoy`：疑似广告、预热视频或假候选。
- `failed`：已验证不可下载或不可复放。

后端会阻止已知失败候选被手动选择，避免前端误点后才失败。

## 不应该跳过的检查

- 对候选 URL 重复执行 SSRF 策略。
- 对重定向目标再次校验。
- 在下载前用 `ffprobe` 或首段请求确认媒体可复放。
- 对 HLS/DASH 验证子 URL、首段和必要 Header；未能完成子资源策略校验时默认拒绝。
- 控制任务时长、下载大小和分片数量。
- 按站点记录失败分类，避免把区域限制、DRM、缺 Cookie 和假广告混成一个错误。
- 生成产物后跑 raw URL 自检，确认 `HEAD`、`Range`、MIME 和 MP4 faststart。

## 后续方向

- 独立 worker 和 lease 队列。
- 更细的站点适配器注册表。
- 候选评分模型可配置化。
- 失败候选自动回写站点规则。
- 对长视频支持分段捕获和续传。
- S3 兼容对象存储和 CDN。
