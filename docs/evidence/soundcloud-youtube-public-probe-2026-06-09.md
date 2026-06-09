# SoundCloud / YouTube 公开页 probe 证据 - 2026-06-09

## 范围

本次验证使用匿名、无登录 Cookie 的 headless Playwright profile 访问公开视频或
公开音频页面。没有绕过付费、验证码、登录权限、DRM 或访问控制。

目标不是声称平台已经完整支持，而是确认当前 browser-probe 路径在真实页面上的
候选结构，并把失败状态记录为可复现证据。

## 代码变更

browser sidecar 已在候选入库前增加两类过滤：

- 按 job `outputs` 只保留请求的候选类型。
- 屏蔽已实测确认的 YouTube 页面 UI 音效 URL：
  `youtube.com/s/search/audio/*.mp3`。

过滤前，YouTube 样本会把 `failure.mp3`、`open.mp3`、`success.mp3`、
`no_input.mp3` 这类页面交互音效误报为 audio 候选。过滤后，这些 URL 不再进入
API 候选列表。

## SoundCloud 样本

无效基线样本：

- URL: `https://soundcloud.com/zooshapes/the-moon-is-a-harsh-mistress`
- 观测标题: `This track was not found | Free Listening on SoundCloud`
- 结果：页面自身不可用，不能作为 smoke-test fixture。

有效公开页样本：

- URL: `https://soundcloud.com/flowkingbrave/baddie`
- 观测标题: `Stream Baddie by FLOW KING BRAVE | Listen online for free on SoundCloud`
- 变更前候选：只观察到 image/html，没有 audio 候选。
- 增加简单播放触发后候选：仍只观察到 image/html，没有 audio 候选。

过滤后生产任务：

- Job ID: `391cd02e-7afd-4457-bef5-abe7152e5bb2`
- Discovery: `browser`
- Platform hint: `soundcloud`
- Outputs: `audio`
- Final status: `error`
- Error: `remote source error: browser probe did not find media candidates`
- Candidate list: empty

结论：当前 browser-probe 路径尚不能从该 SoundCloud 公开页发现真实 audio
候选。后续需要站点专用 extractor 或更明确的播放/manifest 解析证据，不能把
SoundCloud 标记为已支持。

## YouTube 样本

公开页样本：

- URL: `https://www.youtube.com/watch?v=xSZqX5Io6AY`
- 观测标题: `Cans Without Labels by John K. - YouTube`
- 变更前候选：`audio=4`，但全部是 YouTube 页面 UI 音效，不是视频或音轨媒体。
- 变更前还观察到若干账号登录相关的 HTML 响应，未作为媒体候选使用。

过滤后生产任务：

- Job ID: `a5dd2544-d719-4213-ae0a-d1edc2660579`
- Discovery: `browser`
- Platform hint: `youtube`
- Outputs: `audio`, `video`
- Final status: `error`
- Error: `remote source error: browser probe did not find media candidates`
- Candidate list: empty

结论：当前 browser-probe 路径尚不能从该 YouTube 公开页发现真实 video/audio
候选。过滤后系统会明确失败，不再把页面 UI 音效误报为可下载音频。

## 工程结论

- B 站公开视频解析已通过真实生产链路验证。
- SoundCloud 和 YouTube 当前只完成了失败路径证据记录，还没有可交付的真实媒体
  extractor。
- 下一步应把 SoundCloud/YouTube 拆成站点专用 extractor 任务，并为每个平台保留
  “无候选即失败”的行为，避免错误生成可播放承诺。
