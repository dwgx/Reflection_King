# Reflection King — 真实站点验证回归 catalog

`chain_live_catalog`(`crates/reflection-core/src/extractors/mod.rs`,`#[ignore]`)
读 `RK_CHAIN_CATALOG` 指向的纯文本 catalog(每行一个 page/直链 URL,`#` 注释、
空行忽略),对每条跑完整 extract→verify 闭环并打印候选级诊断 + 状态分布。它是
**诊断**而非 pass/fail 门禁:从不对单站断言(真实页会腐烂/下线),只在 catalog 不可读
或全部页零候选(=链路本身断了)时失败。这些 catalog 是 verify 改动的**回归安全网**:
任何 classify_probe / verify / generic 改动后,跑一遍看状态分布有无非预期漂移。

## catalog 清单

| 文件 | URL 数 | 覆盖 |
|---|---|---|
| `catalog-archive.txt` | 41 | archive.org/details 公共领域影视(advancedsearch.php harvest,验过含 og:video) |
| `catalog-manifest.txt` | 7 | 真实 HLS/DASH demo 流(直链),验 manifest 分类:clean→Usable、axprod→Drm |
| `catalog-mixed.txt` | 102 | 4 路 harvest 合并:archive-extra + streaming-demos(含 DRM)+ jsonld-shells + social/video JS 壳。最全,首选回归集 |

## 跑法

```bash
# 在 home-cloud repo 内(本地 SOCKS 代理已配,真实站点才连得通)。
# 注意:cargo test 的 cwd 不是 repo 根,catalog 路径要用绝对路径($PWD/)。
RK_CHAIN_CATALOG=$PWD/loop/catalog-mixed.txt RK_VERIFY_ENABLED=1 \
  cargo test -p reflection-core --release chain_live_catalog -- --ignored --nocapture
```

输出尾部的 `candidate state breakdown : {...}` 是状态直方图。候选级诊断行格式
`* [HTTP状态 content-type] <媒体URL> <- <来源页>`,按状态分组列出
Failed / SuspectAd / Drm 等,便于一眼区分真死(404)、瞬时(5xx,已 retry)、
S3 denied(403 xml)、embed 播放页(200 text/html)、真媒体(200 application/ogg)。

## 已知基线(2026-06-30,网络抖动会让数字小幅浮动)

`catalog-mixed.txt`(102 页)典型分布:`Usable` ~168、`Drm` ~8、`SuspectAd` ~10、
`Failed` ~15(瞬时 5xx 会临时抬高,multi-retry 后多半恢复;90/102 页 ≥1 候选,
81/102 页 top=Usable,12 页零候选=JS 壳社交站如 instagram/tiktok 无 og 标记)。
SuspectAd 主体是 archive.org `/embed/` 播放页(200 text/html,非直链媒体,分类正确)。

> 注:catalog 里的真实页会随时间腐烂/下线;Failed 升高优先看候选级 `[status]`
> 判断是真死还是网络抖动,别误读成代码回归。
