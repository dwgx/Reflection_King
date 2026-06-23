# 安全边界

媒体抓取后端风险很高：它会访问用户提供的 URL，解析第三方页面，并运行 ffmpeg、Chromium、
yt-dlp 等重型组件。Reflection King 的默认目标是“安全地处理用户有权访问的公开或授权内容”，
并把生成后的媒体产物作为 `/media/...` raw URL 提供给 VRChat 或其他播放器。

## 已实现的基础防线

- 只接受 `http` 和 `https` 来源。
- 请求前解析域名。
- 阻止私网、回环、链路本地、多播、保留和文档网段。
- 手动跟随重定向，并对每一次重定向目标重新执行 URL 策略。
- 通过 `RK_MAX_DOWNLOAD_MB` 限制下载大小。
- 对候选资源记录可复放状态、失败原因、授权需求、区域限制、DRM/广告风险。
- `POST /api/jobs`、候选选择和管理接口可以通过 `RK_API_KEY` 强制鉴权。
- 用户密钥可以限制是否允许浏览器探测、yt-dlp、外部适配器和 Profile 登录。
- 浏览器 Profile 登录通过已鉴权 API 控制。前端只接收截图并发送点击/键盘事件，Cookie 值留在服务器 Profile。
- 媒体产物通过 `/media/...` raw URL 提供，不带 Cookie 和 API key，便于播放器读取；这是允许且预期的交付边界。

## 公网部署前必须做

- 设置 `RK_API_KEY`，不要无密钥公网部署。
- 使用 HTTPS，尤其是控制台需要输入管理密钥时。
- 给 VPS 或容器配置磁盘配额，防止下载和转码占满磁盘。
- 设置反向代理请求体限制、连接超时和访问日志。
- 增加按 IP、用户密钥或租户的速率限制。
- 只记录任务 ID、平台和错误分类，不记录完整 Cookie、密钥或私密 URL。
- 保持 Playwright sidecar、CDP、VNC、调试端口只监听 localhost 或内网。
- `/media/...` raw URL 知道地址即可访问，应配合保留时间、访问日志、限速和清理策略使用。
- 定期清理旧任务、失败临时文件和过期 Profile。

## Cookie 和授权

Profile Cookie 只能用于操作者已经有权限访问的内容。它可以帮助服务端复用登录态和必要 Header，

不要把 Cookie JSON、浏览器 Profile、SQLite 数据库、`.env`、`.env.docker`、
`/etc/reflection-king/reflection.env` 或 `/root/reflection-king-admin-key.txt` 提交到 GitHub。

## SSRF 要求

SSRF 校验不能只在原始 URL 上做一次。以下位置都必须重复校验：

- 用户提交的来源 URL。
- HTTP 重定向目标。
- 外部适配器返回的媒体 URL。
- 浏览器探测得到的 manifest、分片和直接文件 URL。
- HLS/DASH manifest 内部引用的子 URL、初始化片段、密钥 URI 和媒体分片。
- 未来对象存储、代理下载或录制入口。

如果 URL 解析、DNS、重定向或内容类型不确定，默认拒绝并把原因写入任务错误摘要。

## 版权和平台规则

项目不能被描述为“绕过平台限制”的工具。使用者应只处理自己拥有、创作、授权、许可或依法可使用的媒体。
把产物公开给 VRChat、网页播放器或其他平台时，仍然需要遵守来源站点和目标平台的规则。
