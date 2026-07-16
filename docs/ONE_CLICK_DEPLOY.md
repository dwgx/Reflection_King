# 一键部署（digest 锁定，AI 照做保证一模一样）

本文档是**确定性部署剧本**。目标：任何人或 AI 逐条照做，得到**字节级完全相同**
的部署。做到这一点的唯一手段是用镜像 **digest（sha256）** 而不是 tag —— digest
不可变，同一个 digest 永远拉到完全相同的镜像；tag（`latest`、`v0.1.0`）可被
重新推送覆盖，**不保证一模一样**。

> 面向 AI 执行者的铁律：
> 1. **只用 digest 引用镜像**。禁止 `latest`。禁止把本地 `docker build` 当生产。
> 2. 部署后**必须执行 digest 校验步骤**（第 4 步）。校验不通过 = 部署无效，停止并报告。
> 3. 不要把真实 `RK_API_KEY`、`/data`、`.env.docker` 提交到任何仓库。

## 当前发布

| 项 | 值 |
|---|---|
| 版本 tag | `v0.1.0` |
| 镜像 digest | `sha256:ad780a20bb0aa8b1717ecc9fead9b77595d1cba51f9241ba69d7d491b355b2ac` |
| 完整引用 | `ghcr.io/dwgx/reflection-king@sha256:ad780a20bb0aa8b1717ecc9fead9b77595d1cba51f9241ba69d7d491b355b2ac` |

`docker-compose.ghcr.yml` 的默认 `image` 已内置这个 digest，无需额外设置即锁定。

## 前置条件

- Docker Engine 24+ 与 Docker Compose v2（`docker compose version` 可用）。
- 放行公网端口（默认 `8780/tcp`），或用 `RK_PUBLIC_PORT` 改端口。
- `RK_PUBLIC_BASE_URL` 必须是外部播放器（如 VRChat）能访问的地址，否则
  `/media/...` raw URL 只能本机访问。

## 部署步骤（逐条照做）

### 1. 取得 compose 与环境模板（锁定到 v0.1.0，避免 master 漂移）

```bash
curl -fsSLO https://raw.githubusercontent.com/dwgx/Reflection_King/v0.1.0/docker-compose.ghcr.yml
curl -fsSLO https://raw.githubusercontent.com/dwgx/Reflection_King/v0.1.0/.env.docker.example
cp .env.docker.example .env.docker
```

> 这个 `v0.1.0` 的 compose 文件里 `image` 默认就是上面那个 digest。不要改成 tag。

### 2. 填写 `.env.docker`

至少设置：

```bash
# 外部可达地址（域名或 IP:端口）；VRChat 等外部播放器要用它访问 /media raw URL
RK_PUBLIC_BASE_URL=https://rk.example.com
# 建议显式设一个强 admin key；留空则容器自动生成并写入 /data/admin-key.txt
RK_API_KEY=<设一个只有你知道的强随机值>
```

其余 `RK_MAX_DOWNLOAD_MB` / `RK_MAX_CONCURRENT_JOBS` / `RK_BROWSER_*` 按需保留默认。

### 3. 启动

```bash
docker compose -f docker-compose.ghcr.yml --env-file .env.docker up -d
docker compose -f docker-compose.ghcr.yml logs -f reflection-king
```

### 4. 校验 digest —— 必做，这是"一模一样"的证明

```bash
docker inspect --format '{{index .RepoDigests 0}}' \
  "$(docker compose -f docker-compose.ghcr.yml ps -q reflection-king)"
```

输出必须**精确等于**：

```
ghcr.io/dwgx/reflection-king@sha256:ad780a20bb0aa8b1717ecc9fead9b77595d1cba51f9241ba69d7d491b355b2ac
```

不相等 = 拉到的不是预期镜像，部署无效。停止并排查（多半是有人改了 compose 用了 tag）。

### 5. 健康检查与首个 admin key

```bash
curl -fsS http://127.0.0.1:8780/api/health        # 期望 ok:true，且 public_base_url 是你填的地址
```

若第 2 步没设 `RK_API_KEY`，读取自动生成的：

```bash
docker compose -f docker-compose.ghcr.yml exec reflection-king cat /data/admin-key.txt
```

（该 key 只留在容器 `/data` 卷里，不要外传或提交。）

## 升级 / 回滚 —— 也靠 digest

换版本只改镜像引用，不动其它：

```bash
# 升级到新 digest（发布新版本时把新 digest 记进本文档的"当前发布"表）
RK_IMAGE_REF=ghcr.io/dwgx/reflection-king@sha256:<新digest> \
  docker compose -f docker-compose.ghcr.yml --env-file .env.docker up -d

# 回滚 = 用旧 digest 再 up -d 一次
RK_IMAGE_REF=ghcr.io/dwgx/reflection-king@sha256:ad780a20bb0aa8b1717ecc9fead9b77595d1cba51f9241ba69d7d491b355b2ac \
  docker compose -f docker-compose.ghcr.yml --env-file .env.docker up -d
```

数据（SQLite、产物、Profile）在 `reflection_data` 卷里，升级/回滚镜像不影响。

## 如何为新版本取得 digest

发布新镜像后，用下面任一方式拿到 digest，更新本文档"当前发布"表和 compose 默认值：

```bash
# 方式 A：本地拉取后查看
docker pull ghcr.io/dwgx/reflection-king:v0.1.0
docker inspect --format '{{index .RepoDigests 0}}' ghcr.io/dwgx/reflection-king:v0.1.0

# 方式 B：不拉取，直接问 registry（匿名，公开镜像）
TOKEN=$(curl -fsSL "https://ghcr.io/token?scope=repository:dwgx/reflection-king:pull" \
  | sed -E 's/.*"token":"([^"]+)".*/\1/')
curl -fsSL -H "Authorization: Bearer $TOKEN" \
  -H "Accept: application/vnd.oci.image.index.v1+json" \
  -I "https://ghcr.io/v2/dwgx/reflection-king/manifests/v0.1.0" \
  | grep -i docker-content-digest
```

## 与其它部署方式的关系

- `docs/DEPLOYMENT.md` — VPS 一键脚本（源码构建 + systemd + nginx）与本地 Docker
  build。那些方式**从源码构建，不保证与 GHCR 镜像字节一致**；要"一模一样"就用本文档的 digest 方式。
- 本文档是生产/复现的推荐路径。

