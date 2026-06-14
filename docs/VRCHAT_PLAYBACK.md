# VRChat 播放

VRChat 播放器需要公网可访问的媒体 URL。`localhost`、私网 IP 和只在服务器本机可访问的地址，
其他玩家客户端都无法读取。

官方参考：

- VRChat video players: https://creators.vrchat.com/worlds/udon/video-players/
- VRChat video player allowlist: https://creators.vrchat.com/worlds/udon/video-players/www-whitelist/

## Raw Media URL

API 会把生成的媒体产物作为不需要 API key 的 raw URL 暴露：

```text
/media/<job-id>/<artifact-filename>
```

响应目标：

- `Content-Type: audio/mpeg`
- `Content-Type: video/mp4`
- `Accept-Ranges: bytes`
- 满足 Range 请求时返回 `Content-Range`
- `Content-Length`
- `HEAD` 支持
- 简单 CORS 支持

当前实现支持单段 Range 请求，覆盖浏览器、VRChat 播放器和常见代理对大音频/视频的探测。

## 视频播放器兼容目标

给 VRChat 视频播放器使用时，优先生成：

- 直接 `.mp4` URL，不是 HTML 页面。
- MP4 带 `-movflags +faststart`。
- H.264 视频，`yuv420p` 像素格式。
- AAC 音频；纯音频产物可使用 MP3。
- 公网 `http` 或 `https` URL，不依赖 API key、Cookie 或复杂重定向。
- 面向 Android/Quest 时使用 HTTPS。

示例公网地址：

```text
<public-base-url>
```

如果你的域名不在 VRChat 官方 allowlist 内，PC 端测试需要在 VRChat 里启用
`Allow Untrusted URLs`。Android/Quest 对非 allowlist 地址通常要求 HTTPS，
生产使用应绑定域名并配置 TLS。

## 自检命令

检查单个 raw URL：

```powershell
python scripts\smoke\vrchat_raw_url_check.py `
  --url "<public-base-url>/media/<job-id>/<artifact>.mp4"
```

检查某个任务的所有产物：

```powershell
$env:RK_API_KEY = "<user-or-admin-key>"
python scripts\smoke\vrchat_raw_url_check.py `
  --base-url "<public-base-url>" `
  --job-id "<job-id>"
```

自检内容：

- `HEAD` 返回 `200`。
- `Range: bytes=0-511` 返回 `206`。
- 存在 `Accept-Ranges`、`Content-Length`、`Content-Range` 和 MIME。
- MP4 的 `moov` atom 位于媒体数据前。
- MP4 视频为 H.264，音频为 AAC/MP3。
- MP3 纯音频产物包含 MP3 音频流。

## 重要限制

- 非 allowlist 域名需要用户启用不受信任 URL。
- Android/Quest 对非 allowlist 地址应使用 HTTPS。
- Public world 可能有更严格的 URL 和同步规则。
- 某些 VRChat 视频播放器拒绝纯音频文件；这种场景应生成“静态图片 + 音频轨”的 MP4。
- 长音频和长视频必须在目标世界实际播放器里测试。
- 不要高频切换 URL，避免触发 VRChat 视频播放器加载频率限制。
