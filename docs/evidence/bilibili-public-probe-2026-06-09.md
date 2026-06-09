# B 站公开视频 probe 证据 - 2026-06-09

## 范围

本次烟测使用匿名、无登录 Cookie 的 headless Playwright profile 访问 B 站
公开视频页面。没有绕过付费、验证码、登录权限或访问控制。

## 输入

- URL: `https://www.bilibili.com/video/BV1AUkBBpELC`
- 观测页面标题: `【VRChat】最新版新人入坑快速教程_哔哩哔哩bilibili_攻略`
- 观测最终 URL: `https://www.bilibili.com/video/BV1AUkBBpELC/`
- Probe 入口: browser sidecar `/probe`
- Profile: `bilibili_public_smoke`
- Timeout: 60 秒
- Max events: 1200
- Max candidates: 120

## 失败基线样本

旧样本 `https://www.bilibili.com/video/BV1GJ411x7h7` 被重定向到
`https://www.bilibili.com/?spm_id_from=333.788.selfDef.errorpage`，没有产生
audio/video 候选，只产生 image/html 候选。因此它不能作为当前 smoke-test
fixture。

## 实际候选结构

当前公开样本成功进入视频页：

- `eventCount`: 192
- `timedOut`: false
- `warnings`: none
- candidates by kind: `video=6`, `audio=3`, `image=31`, `html=4`

稳定的音视频候选来自页面 `__playinfo__` 的 DASH 结构：

| kind | source | example path | quality label | content type |
| --- | --- | --- | --- | --- |
| video | `bilibili_playinfo` | `33907541787-1-100023.m4s` | `480p` | `video/mp4` |
| video | `bilibili_playinfo` | `33907541787-1-100022.m4s` | `360p` | `video/mp4` |
| audio | `bilibili_playinfo` | `33907541787-1-30216.m4s` | `bilibili-audio-30216` | `audio/mp4` |
| audio | `bilibili_playinfo` | `33907541787-1-30232.m4s` | `bilibili-audio-30232` | `audio/mp4` |
| audio | `bilibili_playinfo` | `33907541787-1-30280.m4s` | `bilibili-audio-30280` | `audio/mp4` |

## 筛选结论

第一次成功 probe 发现，`data.bilibili.com/log/web` 埋点请求会把真实 `.m4s`
URL 放进 query string。如果用完整 URL 判断扩展名，会把埋点误判成 video。

当前 sidecar 已改成只用 URL pathname 判断媒体扩展名，并从 B 站页面
`__playinfo__` 直接补充 DASH 候选。对于本次观测到的公开页面结构，audio 和
video 候选已经能稳定分开。

## 本机限制

当前 Windows 本机仍没有 `cargo`，所以 Rust 格式化、clippy、测试和 API
端到端验证在服务器与 GitHub Actions 中执行。本机仅执行 sidecar TypeScript
检查。

```powershell
cargo check --workspace
.\scripts\check.ps1
```

## 服务器端到端验证

验证时间：2026-06-09。

部署目标：

- OS: Debian GNU/Linux 12 (bookworm)
- 仓库：`https://github.com/dwgx/Reflection_King`
- 部署路径：`/opt/reflection-king`
- 更新方式：匿名 HTTPS `git fetch/reset` 到 `origin/master`
- 公开入口：`http://<public-host>/`
- API 健康检查：`http://<public-host>/api/health`

服务器检查结果：

- `cargo fmt --all -- --check`: pass
- `cargo clippy --workspace --all-targets -- -D warnings`: pass
- `cargo test --workspace`: pass
- `services/reflection-browser npm run check`: pass
- GitHub Actions CI for runtime commit `de29470`: pass

生产服务状态：

- `nginx`: active
- `reflection-browser`: active on `127.0.0.1:8791`
- `reflection-api`: active on `127.0.0.1:8787`
- nginx reverse proxy: public port `80` to API

真实 B 站生产任务：

- Job ID: `43092151-5834-49cc-b300-94fe4b8a2687`
- Input URL: `https://www.bilibili.com/video/BV1AUkBBpELC`
- Discovery: `browser`
- Platform hint: `bilibili`
- Outputs: `audio`, `video`
- Candidate result: `video=6`, `audio=3`, `image=31`, `html=4`
- Selected audio candidate: `bilibili-audio-30280`
- Final status: `ready`

生成 artifact：

- Media URL:
  `/media/43092151-5834-49cc-b300-94fe4b8a2687/audio.mp3`
- Content type: `audio/mpeg`
- Bytes: `5662555`

公网播放响应验证：

```text
HEAD /media/.../audio.mp3
HTTP/1.1 200 OK
Content-Type: audio/mpeg
Content-Length: 5662555
Accept-Ranges: bytes
Access-Control-Allow-Origin: *
```

```text
GET /media/.../audio.mp3
Range: bytes=0-1023

HTTP/1.1 206 Partial Content
Content-Type: audio/mpeg
Content-Length: 1024
Content-Range: bytes 0-1023/5662555
Accept-Ranges: bytes
Access-Control-Allow-Origin: *
```

结论：在服务器生产部署上，B 站公开视频候选发现、音频候选选择、MP3 转码、
公网媒体 URL 和 HTTP Range 播放链路均已验证通过。

## 输出过滤回归验证

验证时间：2026-06-09。

变更：browser sidecar 在候选入库前按 job `outputs` 过滤候选类型。对
`outputs=["audio","video"]` 的 B 站任务，只保留 audio/video/manifest，不再把
image/html 噪声送入 API。

生产任务：

- Job ID: `d7341f26-abe5-46a3-9653-3fe0ec0fcdc9`
- Input URL: `https://www.bilibili.com/video/BV1AUkBBpELC`
- Discovery: `browser`
- Platform hint: `bilibili`
- Outputs: `audio`, `video`
- Candidate result after filtering: `video=6`, `audio=3`
- Image/html candidates after filtering: `0`
- Selected audio candidate: `bilibili-audio-30280`
- Final status: `ready`

生成 artifact：

- Media URL:
  `/media/d7341f26-abe5-46a3-9653-3fe0ec0fcdc9/audio.mp3`
- Content type: `audio/mpeg`
- Bytes: `5662555`

公网播放响应验证：

```text
HEAD /media/.../audio.mp3
HTTP/1.1 200 OK
Content-Type: audio/mpeg
Content-Length: 5662555
Accept-Ranges: bytes
Access-Control-Allow-Origin: *
```

```text
GET /media/.../audio.mp3
Range: bytes=0-1023

HTTP/1.1 206 Partial Content
Content-Type: audio/mpeg
Content-Length: 1024
Content-Range: bytes 0-1023/5662555
Accept-Ranges: bytes
Access-Control-Allow-Origin: *
```

结论：输出过滤没有破坏 B 站公开视频解析、候选选择、MP3 转码或 Range 播放。
