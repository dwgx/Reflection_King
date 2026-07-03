import type {
  ArchiveFileView,
  Artifact,
  Candidate,
  CandidateMetadata,
  JobIssue,
  JobView,
  JobStats,
  OutputKind,
  OutputMode,
  PlatformHint,
  RuntimeSettingsForm,
  RuntimeSettingsView,
  ViewMode,
} from "../types";

export function errorMessage(error: unknown): string {
  return friendlyError(error instanceof Error ? error.message : String(error));
}

export function apiHeaders(apiKey: string): Record<string, string> {
  return apiKey ? { "x-api-key": apiKey } : {};
}

export function formatShortDate(value: string): string {
  return new Date(value).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatBytes(value: number | null | undefined): string {
  if (!value) return "-";
  const units = ["B", "KB", "MB", "GB"];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

export function runtimeSettingsToForm(settings: RuntimeSettingsView): RuntimeSettingsForm {
  return {
    public_base_url: settings.public_base_url,
    max_download_mb: String(bytesToMib(settings.max_download_bytes)),
    download_timeout_seconds: String(settings.download_timeout_seconds),
    yt_dlp_timeout_seconds: String(settings.yt_dlp_timeout_seconds),
    yt_dlp_max_json_mb: String(bytesToMib(settings.yt_dlp_max_json_bytes)),
    job_ttl_hours: String(settings.job_ttl_hours),
    page_archive_max_resources: String(settings.page_archive_max_resources ?? 200),
    page_archive_max_resource_mb: String(bytesToMib(settings.page_archive_max_resource_bytes ?? 16 * 1024 * 1024)),
    page_archive_max_total_mb: String(bytesToMib(settings.page_archive_max_total_bytes ?? 200 * 1024 * 1024)),
    page_archive_capture_cdp_enabled: settings.page_archive_capture_cdp_enabled ?? true,
    page_archive_save_mhtml_enabled: settings.page_archive_save_mhtml_enabled ?? true,
    page_archive_save_har_enabled: settings.page_archive_save_har_enabled ?? true,
    page_archive_save_warc_enabled: settings.page_archive_save_warc_enabled ?? true,
    page_archive_cdp_body_max_mb: String(bytesToMib(settings.page_archive_cdp_body_max_bytes ?? 2 * 1024 * 1024)),
    page_archive_cdp_body_total_mb: String(bytesToMib(settings.page_archive_cdp_body_total_bytes ?? 64 * 1024 * 1024)),
    cache_cleanup_min_age_hours: String(settings.cache_cleanup_min_age_hours ?? 24),
  };
}

export function bytesToMib(value: number): number {
  return Math.max(1, Math.round(value / 1024 / 1024));
}

export function parsePositiveInt(value: string, label: string): number {
  const trimmed = value.trim();
  const parsed = Number(trimmed);
  if (!trimmed || !Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(`${label} 必须是正整数`);
  }
  return parsed;
}

export function parseOptionalPositiveInt(value: string, label: string): number | undefined {
  if (!value.trim()) return undefined;
  return parsePositiveInt(value, label);
}

export function paginate<T>(items: T[], page: number, pageSize: number): { items: T[]; page: number; start: number } {
  const safePage = clampPage(page, items.length, pageSize);
  const start = (safePage - 1) * pageSize;
  return {
    items: items.slice(start, start + pageSize),
    page: safePage,
    start,
  };
}

export function clampPage(page: number, total: number, pageSize: number): number {
  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  return Math.min(Math.max(1, page), pageCount);
}

export function capabilityStatus(value: boolean | undefined, apiKey: string): string {
  if (value === true) return "已配置";
  if (value === false) return "未配置";
  return apiKey ? "读取中" : "需要管理密钥";
}

export function viewModeLabel(value: ViewMode): string {
  return ({
    console: "控制台",
    admin: "高级设置",
    help: "帮助",
  } as Record<ViewMode, string>)[value];
}

export function roleLabel(value: string): string {
  return ({
    admin: "管理密钥",
    user: "用户密钥",
  } as Record<string, string>)[value] ?? value;
}

export function statusLabel(value: string): string {
  return ({
    queued: "排队中",
    resolving: "解析中",
    candidates_ready: "待选择",
    candidate_selected: "已提交",
    downloading: "下载中",
    capturing: "捕获中",
    probing: "探测中",
    transcoding: "转码中",
    remuxing: "封装中",
    needs_profile: "需授权",
    ready: "已完成",
    error: "失败",
  } as Record<string, string>)[value] ?? value;
}

export function discoveryLabel(value: string): string {
  return ({
    direct: "直链",
    external: "外部解析",
    browser: "浏览器探测",
    auto: "自动解析",
  } as Record<string, string>)[value] ?? value;
}

export function platformLabel(value: string): string {
  return ({
    auto: "自动",
    bilibili: "哔哩哔哩",
    youtube: "YouTube",
    soundcloud: "SoundCloud",
    ximalaya: "喜马拉雅",
    douyin: "抖音",
    kuaishou: "快手",
    pornhub: "Pornhub",
    acfun: "AcFun",
    iqiyi: "爱奇艺",
    youku: "优酷",
    tiktok: "TikTok",
    vimeo: "Vimeo",
    weibo: "微博",
    dailymotion: "Dailymotion",
    rumble: "Rumble",
    peertube: "PeerTube",
    archive_org: "Archive.org",
    wayback: "Wayback",
    archive_it: "Archive-It",
    perma_cc: "Perma.cc",
    archive_today: "archive.today",
    ghostarchive: "Ghostarchive",
    webcitation: "WebCitation",
    memento: "Memento",
    wikimedia: "Wikimedia",
    twitch: "Twitch",
    twitter: "X / Twitter",
    reddit: "Reddit",
    instagram: "Instagram",
    facebook: "Facebook",
    pinterest: "Pinterest",
    imgur: "Imgur",
    flickr: "Flickr",
    bandcamp: "Bandcamp",
    mixcloud: "Mixcloud",
    niconico: "Niconico",
    fc2: "FC2",
    spotify: "Spotify",
    live: "直播/清单",
    generic: "通用",
  } as Record<string, string>)[value] ?? value;
}

export function authModeLabel(value: string): string {
  return ({
    auto: "自动",
    none: "无",
    profile: "浏览器配置",
    cookies: "Cookie",
  } as Record<string, string>)[value] ?? value;
}

export function outputLabel(value: string): string {
  return ({
    audio: "音频",
    video: "视频",
    image: "图片",
    page_html: "网页包",
  } as Record<string, string>)[value] ?? value;
}

export function outputModeLabel(value: string): string {
  return ({
    auto: "自动（媒体）",
    video: "视频",
    audio: "音频",
    image: "图片",
    page_html: "HTML/CSS/JS",
  } as Record<string, string>)[value] ?? value;
}

export function cacheCategoryLabel(value: string): string {
  return ({
    public_artifacts: "公开产物",
    temporary_jobs: "临时任务",
    browser_profiles: "浏览器 Profile",
  } as Record<string, string>)[value] ?? value;
}

export function artifactLabel(artifact: Artifact): string {
  const url = artifact.media_url.toLowerCase();
  const contentType = artifact.content_type.toLowerCase();
  if (artifact.kind === "page_html") {
    if (url.endsWith("/archive.zip") || contentType.includes("zip")) return "网页前端包";
    if (url.endsWith("/archive.har")) return "HAR 归档";
    if (url.endsWith("/archive.mhtml")) return "MHTML 快照";
    if (url.endsWith("/archive.warc")) return "WARC 归档";
    if (url.endsWith("/index.html")) return "原始入口 HTML";
    if (url.endsWith("/page.html") || contentType.includes("html")) return "入口 HTML";
    if (url.endsWith("/resources.json") || contentType.includes("json")) return "资源清单";
    if (url.endsWith("/page.txt") || contentType.includes("text/plain")) return "页面文本";
    if (url.endsWith("/screenshot.png") || contentType.startsWith("image/")) return "页面截图";
  }
  return outputLabel(artifact.kind);
}

export function artifactOpenLabel(artifact: Artifact): string {
  if (isDownloadArtifact(artifact)) return "下载";
  return "打开";
}

export function archiveFileLabel(file: ArchiveFileView): string {
  const normalized = file.path.toLowerCase();
  if (normalized === "index.html") return "网页入口";
  if (normalized === "index.inline.html") return "内联预览";
  if (normalized === "page.html") return "原始页面";
  if (normalized === "page.txt") return "页面文本";
  if (normalized === "preview/screenshot.png" || normalized === "screenshot.png") return "页面截图";
  if (normalized === "metadata/resources.json" || normalized === "resources.json") return "资源清单";
  if (normalized === "metadata/archive.har" || normalized === "archive.har") return "HAR 归档";
  if (normalized === "metadata/archive.mhtml" || normalized === "archive.mhtml") return "MHTML 快照";
  if (normalized === "metadata/archive.warc" || normalized === "archive.warc") return "WARC 归档";
  return file.name;
}

export function compareArtifacts(left: Artifact, right: Artifact): number {
  return artifactRank(left) - artifactRank(right)
    || left.media_url.localeCompare(right.media_url);
}

export function artifactRank(artifact: Artifact): number {
  const url = artifact.media_url.toLowerCase();
  const contentType = artifact.content_type.toLowerCase();
  if (artifact.kind !== "page_html") return contentType.startsWith("video/") ? 10 : contentType.startsWith("audio/") ? 20 : 30;
  if (url.endsWith("/archive.zip") || contentType.includes("zip")) return 0;
  if (url.endsWith("/index.html")) return 1;
  if (url.endsWith("/page.html") || contentType.includes("html")) return 2;
  if (url.endsWith("/screenshot.png") || contentType.startsWith("image/")) return 3;
  if (url.endsWith("/resources.json")) return 4;
  if (url.endsWith("/page.txt") || contentType.includes("text/plain")) return 5;
  if (url.endsWith("/archive.mhtml")) return 20;
  if (url.endsWith("/archive.har")) return 21;
  if (url.endsWith("/archive.warc")) return 22;
  return 40;
}

export function compareArchiveFiles(left: ArchiveFileView, right: ArchiveFileView): number {
  return archiveFileRank(left) - archiveFileRank(right)
    || left.path.localeCompare(right.path);
}

export function archiveFileRank(file: ArchiveFileView): number {
  const path = file.path.toLowerCase();
  const contentType = file.content_type.toLowerCase();
  if (path === "index.html") return 0;
  if (path === "index.inline.html") return 1;
  if (path === "page.html") return 2;
  if (path === "preview/screenshot.png" || path === "screenshot.png") return 3;
  if (path === "metadata/resources.json" || path === "resources.json") return 4;
  if (path === "page.txt") return 5;
  if (path.startsWith("assets/") && contentType.startsWith("text/css")) return 20;
  if (path.startsWith("assets/") && (contentType.includes("javascript") || path.endsWith(".js"))) return 21;
  if (path.startsWith("assets/") && contentType.startsWith("image/")) return 30;
  if (path.startsWith("assets/") && contentType.startsWith("font/")) return 40;
  if (path.startsWith("metadata/")) return 80;
  return 60;
}

export function isDownloadArtifact(artifact: Artifact): boolean {
  const url = artifact.media_url.toLowerCase();
  const contentType = artifact.content_type.toLowerCase();
  return contentType.includes("zip")
    || contentType.includes("warc")
    || contentType.includes("multipart")
    || url.endsWith(".zip")
    || url.endsWith(".warc")
    || url.endsWith(".mhtml")
    || url.endsWith(".har");
}

export function jobMediaContentType(job: JobView, artifacts: Artifact[]): string | null {
  if (!job.media_url) return null;
  const matching = artifacts.find((artifact) => artifact.media_url === job.media_url);
  if (matching) return matching.content_type;
  if (job.outputs.includes("page_html")) return "application/zip";
  return null;
}

export function outputsLabel(outputs: OutputKind[]): string {
  if (outputs.includes("video") && outputs.includes("audio")) {
    return "媒体";
  }
  return outputs.map(outputLabel).join(", ");
}

export function outputsForMode(mode: OutputMode): OutputKind[] {
  if (mode === "auto") {
    return ["video", "audio"];
  }
  return [mode];
}

export function isPageArchiveJob(job: JobView | null): boolean {
  return job?.outputs.includes("page_html") ?? false;
}

export function bitrateLabel(value: string): string {
  return ({
    auto: "自动（最高可用）",
    "2160p": "4K / 2160p",
    "1440p": "2K / 1440p",
    "1080p": "1080p",
    "720p": "720p",
    "480p": "480p",
    "360p": "360p",
  } as Record<string, string>)[value] ?? value;
}

export function candidateDisplayList(candidates: Candidate[], job: JobView | null, showAll: boolean): Candidate[] {
  const ranked = [...candidates].sort((left, right) => candidateRank(right, job) - candidateRank(left, job));
  if (showAll) return ranked;

  const wantedKinds = preferredCandidateKinds(job);
  const primary = ranked.filter((candidate) => wantedKinds.has(candidate.kind) && isUsableCandidate(candidate));
  if (primary.length) return primary.slice(0, 8);
  const fallback = ranked.filter((candidate) => candidate.kind !== "image" && candidate.kind !== "html");
  return fallback.slice(0, 8);
}

export function bestCandidate(candidates: Candidate[], job: JobView | null): Candidate | null {
  return candidateDisplayList(candidates, job, false).find(isUsableCandidate) ?? null;
}

export function defaultCandidatesForJob(
  candidates: Candidate[],
  job: JobView | null,
  fallback: Candidate | null,
): Candidate[] {
  const outputs = new Set(job?.outputs ?? ["audio"]);
  if (outputs.has("video")) {
    const mediaCandidates = candidateDisplayList(candidates, job, false)
      .filter((candidate) => candidate.kind === "video" || candidate.kind === "manifest")
      .filter(isUsableCandidate);
    if (!mediaCandidates.length) {
      const imageCandidates = candidates.filter((candidate) => candidate.kind === "image");
      if (imageCandidates.length) return imageCandidates;
    }
    const videoCandidate = mediaCandidates[0];
    if (!videoCandidate) return [];
    if (candidateNeedsAudioCompanion(videoCandidate)) {
      const audioCandidate = bestAudioCompanion(videoCandidate, candidates, job);
      if (audioCandidate) {
        return [videoCandidate, audioCandidate];
      }
    }
    return [videoCandidate];
  }
  if (outputs.has("audio") && !outputs.has("video")) {
    const audioCandidate = candidateDisplayList(candidates, job, false)
      .find((candidate) => candidate.kind === "audio" && isUsableCandidate(candidate));
    if (audioCandidate) {
      return [audioCandidate];
    }
  }
  return fallback && isUsableCandidate(fallback) ? [fallback] : [];
}

export function candidateNeedsAudioCompanion(candidate: Candidate): boolean {
  if (candidate.kind !== "video") return false;
  const acodec = metadataString(candidate, "acodec");
  const vcodec = metadataString(candidate, "vcodec");
  if (codecPresent(vcodec) && !codecPresent(acodec)) {
    return true;
  }
  const value = `${candidate.url} ${candidate.resource_type ?? ""} ${candidate.quality_label ?? ""}`.toLowerCase();
  return (
    value.includes("bilibili") ||
    value.includes(".m4s") ||
    value.includes("dash") ||
    value.includes("video-only")
  );
}

export function metadataString(candidate: Candidate, key: string): string {
  const metadata = candidate.metadata_json as Record<string, unknown> | undefined;
  const nested = metadata?.candidate as Record<string, unknown> | undefined;
  const value = metadata?.[key] ?? nested?.[key];
  return typeof value === "string" ? value.toLowerCase() : "";
}

export function codecPresent(value: string): boolean {
  return Boolean(value && !["none", "null", "unknown"].includes(value));
}

export function bestAudioCompanion(
  videoCandidate: Candidate,
  candidates: Candidate[],
  job: JobView | null,
): Candidate | null {
  const videoFamily = candidateFamily(videoCandidate);
  const audioCandidates = candidates
    .filter((candidate) => candidate.kind === "audio")
    .filter(isUsableCandidate)
    .filter((candidate) => candidateFamily(candidate) === videoFamily || isBilibiliFamily(videoCandidate, candidate));

  if (!audioCandidates.length) return null;
  return audioCandidates.sort((left, right) => candidateRank(right, job) - candidateRank(left, job))[0] ?? null;
}

export function candidateFamily(candidate: Candidate): string {
  const url = safeUrl(candidate.url);
  if (candidate.resource_type === "bilibili_playinfo" || candidate.resource_type === "bilibili_api") {
    return `${candidate.extractor}:bilibili`;
  }
  return `${candidate.extractor}:${candidate.initiator_url ?? url?.hostname ?? candidate.platform ?? "unknown"}`;
}

export function isBilibiliFamily(left: Candidate, right: Candidate): boolean {
  const leftValue = `${left.url} ${left.resource_type ?? ""} ${left.quality_label ?? ""}`.toLowerCase();
  const rightValue = `${right.url} ${right.resource_type ?? ""} ${right.quality_label ?? ""}`.toLowerCase();
  return leftValue.includes("bilibili") && rightValue.includes("bilibili") && left.extractor === right.extractor;
}

export function candidateRank(candidate: Candidate, job: JobView | null): number {
  let rank = candidate.score;
  const outputs = new Set(job?.outputs ?? ["audio"]);

  if (outputs.has("audio")) {
    if (candidate.kind === "audio") rank += 1000;
    if (candidate.kind === "manifest") rank += 650;
    if (candidate.kind === "video") rank += 450;
    if (candidate.kind === "image" || candidate.kind === "html") rank -= 1000;
  }

  if (outputs.has("video")) {
    if (candidate.kind === "video" || candidate.kind === "manifest") rank += 1000;
    if (candidate.kind === "image") rank += 700;
    if (candidate.kind === "audio") rank += 50;
    rank += mp4CompatibilityRank(candidate);
  }

  if (outputs.has("image") && candidate.kind === "image") rank += 900;
  if (candidate.requires_authorization) rank -= 50;
  const height = qualityFromUrl(candidate.url) ?? candidate.quality_label;
  const parsedHeight = height?.match(/([1-9]\d{2,3})p/i)?.[1];
  if (parsedHeight) rank += Math.min(Number(parsedHeight), 2160) / 2;
  if (isLikelyAdCandidate(candidate)) rank -= 5000;
  if (candidate.validation_state === "usable") rank += 500;
  if (candidate.protection === "drm" || candidate.validation_state === "drm") rank -= 8000;
  if (candidate.validation_state === "expired" || candidate.validation_state === "region_blocked") rank -= 6000;
  if ((candidate.evidence_count ?? 1) > 1) rank += Math.min((candidate.evidence_count ?? 1) * 25, 150);
  if (candidate.validation_status?.startsWith("failed")) rank -= 4000;
  return rank;
}

export function mp4CompatibilityRank(candidate: Candidate): number {
  const metadata = candidate.metadata_json as Record<string, unknown> | undefined;
  const nested = metadata?.candidate as Record<string, unknown> | undefined;
  const value = [
    candidate.content_type,
    candidate.url,
    metadata?.ext,
    metadata?.vcodec,
    metadata?.acodec,
    nested?.ext,
    nested?.vcodec,
    nested?.acodec,
  ].filter(Boolean).join(" ").toLowerCase();
  let rank = 0;
  if (value.includes("video/mp4") || value.includes(".mp4") || value.includes(" mp4 ")) rank += 300;
  if (value.includes("avc1") || value.includes("h264")) rank += 300;
  if (value.includes("mp4a") || value.includes("aac")) rank += 120;
  if (value.includes("video/webm") || value.includes(".webm") || value.includes("vp9") || value.includes("vp09") || value.includes("av01")) rank -= 700;
  if (value.includes("opus") || value.includes("vorbis")) rank -= 180;
  return rank;
}

export function isUsableCandidate(candidate: Candidate): boolean {
  if (isLikelyAdCandidate(candidate)) return false;
  if (candidate.validation_status?.startsWith("failed")) return false;
  if (candidate.failure_reason) return false;
  if (["drm", "expired", "failed", "region_blocked", "suspect_ad"].includes(candidate.validation_state ?? "")) return false;
  if (["drm", "region_blocked"].includes(candidate.protection ?? "")) return false;
  return true;
}

export function preferredCandidateKinds(job: JobView | null): Set<string> {
  const outputs = new Set(job?.outputs ?? ["audio"]);
  if (outputs.has("audio") && !outputs.has("video")) {
    return new Set(["audio", "manifest", "video"]);
  }
  if (outputs.has("video")) {
    return new Set(["video", "manifest", "image", "audio"]);
  }
  if (outputs.has("image")) {
    return new Set(["image"]);
  }
  return new Set(["audio", "video", "manifest"]);
}

export function candidateMeta(candidate: Candidate): string {
  return [
    extractorLabel(candidate.extractor),
    candidate.quality_label,
    candidate.content_type,
    candidate.resource_type,
  ].filter(Boolean).join(" / ") || "媒体资源";
}

export function candidateSummary(candidate: Candidate): {
  quality: string;
  source: string;
  kindDetail: string;
  adRisk: boolean;
} {
  const url = safeUrl(candidate.url);
  const path = url?.pathname.toLowerCase() ?? candidate.url.toLowerCase();
  const host = url?.hostname ?? "-";
  const quality = candidate.quality_label ?? qualityFromUrl(candidate.url) ?? "未知质量";
  const source = candidate.resource_type?.replace(/^inline_/, "页面脚本/")
    .replace(/^performance_/, "页面请求/")
    .replace(/^dom_/, "页面元素/")
    ?? host;
  const format = candidate.content_type ?? extensionFromPath(path) ?? "未知格式";
  const kindDetail = candidate.kind === "manifest"
    ? `流媒体清单 / ${format}`
    : `${candidateKindLabel(candidate.kind)} / ${format}`;
  return {
    quality,
    source,
    kindDetail,
    adRisk: isLikelyAdCandidate(candidate),
  };
}

export function safeUrl(value: string): URL | null {
  try {
    return new URL(value);
  } catch {
    return null;
  }
}

export function normalizeSourceInput(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return "";
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed)) return trimmed;
  if (looksLikeBareHostUrl(trimmed)) return `https://${trimmed}`;
  return trimmed;
}

export function platformHintForSourceUrl(value: string): PlatformHint | null {
  const parsed = safeUrl(normalizeSourceInput(value));
  if (!parsed) return null;
  const host = parsed.hostname.toLowerCase();
  const path = parsed.pathname.toLowerCase();
  if (isWaybackHost(host, path)) return "wayback";
  if (hostMatches(host, "archive-it.org")) return "archive_it";
  if (hostMatches(host, "perma.cc")) return "perma_cc";
  if (isArchiveTodayHost(host)) return "archive_today";
  if (hostMatches(host, "ghostarchive.org")) return "ghostarchive";
  if (hostMatches(host, "webcitation.org")) return "webcitation";
  if (hostMatches(host, "mementoweb.org") || hostMatches(host, "mementoarchive.lanl.gov")) return "memento";
  return null;
}

export function hostMatches(host: string, domain: string): boolean {
  return host === domain || host.endsWith(`.${domain}`);
}

export function isWaybackHost(host: string, path: string): boolean {
  return hostMatches(host, "web.archive.org") || (hostMatches(host, "archive.org") && path.startsWith("/web/"));
}

export function isArchiveTodayHost(host: string): boolean {
  return ["archive.today", "archive.ph", "archive.is", "archive.vn", "archive.md", "archive.li", "archive.fo"].includes(host);
}

export function looksLikeBareHostUrl(value: string): boolean {
  const hostPort = value.split(/[/?#]/, 1)[0] ?? "";
  if (!hostPort || hostPort.startsWith("//") || hostPort.includes("@") || /\s/.test(hostPort)) return false;
  const host = hostPort.startsWith("[")
    ? hostPort.slice(1).split("]", 1)[0]
    : hostPort.split(":", 1)[0];
  return host.toLowerCase() === "localhost" || host.includes(".");
}

export function sourceInputIssue(value: string, publicBaseUrl?: string): string | null {
  const parsed = safeUrl(value.trim());
  if (!parsed) return null;
  const host = parsed.hostname.toLowerCase();
  const path = parsed.pathname.toLowerCase();
  const base = publicBaseUrl ? safeUrl(publicBaseUrl) : null;
  const isSameService = parsed.origin === window.location.origin || parsed.origin === base?.origin;
  if (isSameService && (path.startsWith("/media/") || path.startsWith("/api/jobs/"))) {
    return "这是 Reflection King 已生成的产物/API 地址，不是要解析的源网页。请下载或打开产物，或粘贴原始公网页面 URL。";
  }
  if (isSameService) {
    return "这是当前 Reflection King 服务地址，不是源网页。请粘贴 Steam、视频站或普通网页的原始公网 URL。";
  }
  if (
    (host === "www.youtube.com" || host === "youtube.com" || host.endsWith(".youtube.com")) &&
    path === "/watch" &&
    !parsed.searchParams.get("v")
  ) {
    return "YouTube 链接不完整：`/watch` 还缺少 `?v=...` 视频 ID。请粘贴完整视频页地址。";
  }
  if ((host === "youtu.be" || host.endsWith(".youtu.be")) && (!path || path === "/")) {
    return "YouTube 短链接不完整：还缺少视频 ID。请粘贴完整视频页地址。";
  }
  return null;
}

export function qualityFromUrl(value: string): string | null {
  const match = value.match(/(?:^|[^\d])([1-9]\d{2,3})p(?:[^\d]|$)/i);
  return match ? `${match[1]}p` : null;
}

export function extensionFromPath(path: string): string | null {
  const match = path.match(/\.([a-z0-9]{2,5})$/i);
  return match ? match[1].toUpperCase() : null;
}

export function isLikelyAdCandidate(candidate: Candidate): boolean {
  if (candidate.ad_risk) return true;
  const value = `${candidate.url} ${candidate.resource_type ?? ""}`.toLowerCase();
  return [
    "trafficjunky",
    "doubleclick",
    "googlesyndication",
    "adservice",
    "preroll",
    "pre-roll",
    "vast",
    "vpaid",
    "tracking",
    "tracker",
    "pixel",
  ].some((needle) => value.includes(needle));
}

export function validationLabel(value: string): string {
  if (value === "usable") return "已验证可用";
  if (value === "needs_profile") return "需要授权";
  if (value === "suspect_ad") return "疑似广告";
  if (value === "expired") return "已过期";
  if (value === "drm") return "DRM/受保护";
  if (value === "region_blocked") return "地区限制";
  if (value === "untested") return "未验证";
  if (value === "ok") return "已验证可用";
  if (value.startsWith("failed:")) return `不可用：${friendlyError(value.slice("failed:".length).trim())}`;
  return value;
}

export function protectionLabel(value: string): string {
  return ({
    needs_profile: "需要 Profile",
    signed_url: "签名链接",
    drm: "DRM",
    region_blocked: "地区限制",
    unknown: "保护未知",
  } as Record<string, string>)[value] ?? value;
}

export function routeLabel(value: string): string {
  return value
    .replace(/^external:/, "外部/")
    .replace("browser_probe", "浏览器")
    .replace("yt_dlp", "yt-dlp")
    .replace("you_get", "you-get")
    .replace("streamlink", "Streamlink")
    .replace("direct", "直链");
}

export function qualityAvailabilityLabel(candidates: Candidate[], preference: string): string {
  const qualities = Array.from(
    new Set(
      candidates
        .map((candidate) => candidate.quality_label ?? qualityFromUrl(candidate.url))
        .filter((value): value is string => Boolean(value && /^\d{3,4}p$/i.test(value))),
    ),
  ).sort((left, right) => qualityNumber(right) - qualityNumber(left));

  if (!qualities.length) {
    return "未识别清晰度，将按可用媒体排序。";
  }
  const advertised = Math.max(0, ...candidates.map((candidate) => candidateAvailability(candidate).highestAdvertisedHeight ?? 0));
  const highestVisible = qualityNumber(qualities[0]);
  if (advertised > highestVisible) {
    return `当前可用最高 ${qualities[0]}；站点提示最高 ${advertised}p，通常需要登录 Profile。`;
  }
  if (preference === "auto") {
    return `自动选择最高可用：${qualities[0]}。`;
  }
  if (qualities.includes(preference)) {
    return `目标清晰度可用：${preference}。`;
  }
  return `目标 ${preference} 不可用，可用：${qualities.join("、")}。`;
}

export function qualityNumber(value: string): number {
  return Number(value.match(/(\d{3,4})p/i)?.[1] ?? 0);
}

export function candidateAvailability(candidate: Candidate): {
  higherQualityRequiresProfile: boolean;
  highestAdvertisedHeight?: number;
} {
  const metadata = candidate.metadata_json as CandidateMetadata | undefined;
  const raw = metadata?.candidate;
  const highestAdvertisedHeight = Number(raw?.highestAdvertisedHeight ?? 0);
  return {
    higherQualityRequiresProfile: Boolean(raw?.higherQualityRequiresProfile),
    highestAdvertisedHeight: Number.isFinite(highestAdvertisedHeight) && highestAdvertisedHeight > 0
      ? highestAdvertisedHeight
      : undefined,
  };
}

export function extractorLabel(value: string): string {
  return ({
    browser_probe: "浏览器抓取",
    yt_dlp: "站点解析",
  } as Record<string, string>)[value] ?? value;
}

export function candidateKindLabel(value: string): string {
  return ({
    audio: "音频",
    video: "视频",
    image: "图片",
    manifest: "清单",
    html: "HTML",
    unknown: "未知",
  } as Record<string, string>)[value] ?? value;
}

export function sourceTitle(value: string): string {
  try {
    const url = new URL(value);
    return `${url.hostname}${url.pathname === "/" ? "" : url.pathname}`;
  } catch {
    return value;
  }
}

export function compactUrl(value: string): string {
  try {
    const url = new URL(value);
    return `${url.hostname}${url.pathname}${url.search ? "?" : ""}`;
  } catch {
    return value;
  }
}

export function summarizeJobs(items: JobView[]): JobStats {
  const stats: JobStats = {
    total: items.length,
    ready: 0,
    candidates: 0,
    running: 0,
    error: 0,
    cookie: 0,
    dependency: 0,
    unsupported: 0,
  };
  for (const item of items) {
    if (item.status === "ready") {
      stats.ready += 1;
    } else if (item.status === "candidates_ready") {
      stats.candidates += 1;
    } else if (item.status === "needs_profile" || item.status === "error") {
      stats.error += 1;
      const issue = jobIssue(item);
      if (issue?.kind === "cookie" || issue?.kind === "profile") stats.cookie += 1;
      if (issue?.kind === "dependency") stats.dependency += 1;
      if (issue?.kind === "unsupported") stats.unsupported += 1;
    } else {
      stats.running += 1;
    }
  }
  return stats;
}

export function jobIssue(job: JobView | null): JobIssue | null {
  if (job?.issue_kind && job.issue_kind !== "none") {
    const map: Record<NonNullable<JobView["issue_kind"]>, JobIssue | null> = {
      none: null,
      failed: { kind: "error", label: "失败", tone: "error" },
      needs_profile: { kind: "profile", label: "需授权", tone: "warn" },
      unsupported: { kind: "unsupported", label: "待适配", tone: "info" },
      too_large: { kind: "error", label: "过大", tone: "warn" },
      timeout: { kind: "timeout", label: "超时", tone: "warn" },
      policy_blocked: { kind: "policy", label: "策略阻止", tone: "warn" },
    };
    return map[job.issue_kind];
  }
  if (!job?.error) return null;
  const lower = `${job.source_url} ${job.error}`.toLowerCase();
  if (lower.includes("fresh cookies") || lower.includes("sign in") || lower.includes("login required")) {
    return { kind: "cookie", label: "需 Cookie", tone: "warn" };
  }
  if (lower.includes("phantomjs")) {
    return { kind: "dependency", label: "缺依赖", tone: "warn" };
  }
  if (
    lower.includes("unsupported url") ||
    lower.includes("kuaishou") ||
    lower.includes("no media candidates from chain")
  ) {
    return { kind: "unsupported", label: "待适配", tone: "info" };
  }
  if (
    lower.includes("requires headers") ||
    lower.includes("requires authorization") ||
    lower.includes("profile")
  ) {
    return { kind: "profile", label: "需授权", tone: "warn" };
  }
  if (lower.includes("timed out")) {
    return { kind: "timeout", label: "超时", tone: "warn" };
  }
  if (lower.includes("url policy denied request")) {
    return { kind: "policy", label: "策略阻止", tone: "warn" };
  }
  if (lower.includes("yt-dlp probe exited") || lower.includes("external resolver")) {
    return { kind: "resolver", label: "解析器失败", tone: "error" };
  }
  return { kind: "error", label: "失败", tone: "error" };
}

export function friendlyError(value: string, job?: JobView | null): string {
  if (!value) return "-";
  const lower = `${job?.source_url ?? ""} ${value}`.toLowerCase();

  if (lower.includes("url policy denied request") && lower.includes("blocked address")) {
    return "URL 安全策略阻止了这个地址：目标解析到内网、localhost、保留地址或链路本地地址。请提交原始公网 URL，不要把本服务的 192.168/127.0.0.1 产物地址再作为解析来源。";
  }
  if (lower.includes("steam") && lower.includes("no media candidates")) {
    return "Steam 商店页通常是网页内容，不是直连视频/音频媒体页。请选择 HTML/CSS/JS 输出保存网页前端包；如果要下载商店视频，需要页面里真实公开的媒体 URL 或后续专门适配。";
  }
  if (lower.includes("fresh cookies")) {
    return "该链接需要 fresh cookies。请导入对应站点 Cookie/Profile 后重试；这不是媒体管线损坏。";
  }
  if (
    lower.includes("cloudflare") ||
    lower.includes("turnstile") ||
    lower.includes("captcha") ||
    lower.includes("human verification") ||
    lower.includes("security challenge") ||
    lower.includes("security verification")
  ) {
    if (isPageArchiveJob(job ?? null)) {
      return "站点提示登录或安全验证；如果只需要未登录主页 UI 和公开资源，请点“强制解析”。";
    }
    return "站点正在进行安全验证。请打开验证浏览器，像远程桌面一样完成真人验证或登录确认，然后继续解析。";
  }
  if (lower.includes("phantomjs")) {
    return "爱奇艺/iQ.com 当前解析器需要 PhantomJS 兼容依赖；这是待补依赖/适配项。";
  }
  if (lower.includes("kuaishou") && lower.includes("unsupported url")) {
    return "快手当前样本还没有可用自动适配器；yt-dlp 不支持，浏览器探测也未抓到媒体。";
  }
  if (lower.includes("kuaishou") && lower.includes("no media candidates")) {
    return "快手当前样本所有解析链路都未发现可用媒体候选，属于待适配站点。";
  }
  if (lower.includes("no media candidates from chain")) {
    return "所有解析链路都没有找到可用媒体候选。可尝试登录 Profile、切换浏览器探测，或把该站点列入新适配。";
  }
  if (lower.includes("browser probe did not find media candidates")) {
    return "浏览器探测没有发现可用媒体资源";
  }
  if (lower.includes("deprecated feature")) {
    return "yt-dlp 站点规则过期，需要更新 yt-dlp 或更换解析方式";
  }
  if (lower.includes("[bilibili]") && (lower.includes("412") || lower.includes("error"))) {
    return "哔哩哔哩拒绝外部解析请求，建议改用浏览器探测";
  }
  if (lower.includes("raw candidate failed") && lower.includes("delegated")) {
    return "原始媒体地址被站点拒绝，系统已尝试 yt-dlp 代理下载 fallback 但仍失败。";
  }
  if (lower.includes("yt-dlp probe exited")) {
    return "外部解析失败，建议更新 yt-dlp 或改用浏览器探测";
  }
  if (lower.includes("timed out")) {
    return "解析超时";
  }
  if (lower.includes("requires headers") || lower.includes("requires authorization") || lower.includes("profile")) {
    if (isPageArchiveJob(job ?? null)) {
      return "当前任务是网页包；如果只需要未登录页面，请点“强制解析”重新保存 HTML/CSS/JS。";
    }
    return "资源需要登录态、页面授权或安全验证。请打开验证浏览器，使用共享 Profile 处理后继续解析。";
  }
  return value.length > 140 ? `${value.slice(0, 140)}...` : value;
}

export async function copy(value: string): Promise<boolean> {
  let ok = false;
  try {
    await navigator.clipboard.writeText(value);
    ok = true;
  } catch {
    const textarea = document.createElement("textarea");
    textarea.value = value;
    textarea.setAttribute("readonly", "true");
    textarea.style.position = "fixed";
    textarea.style.left = "-9999px";
    textarea.style.top = "0";
    document.body.appendChild(textarea);
    textarea.select();
    textarea.setSelectionRange(0, textarea.value.length);
    try {
      ok = document.execCommand("copy");
    } finally {
      textarea.remove();
    }
  }
  window.dispatchEvent(new CustomEvent("reflection-copy-result", { detail: { ok } }));
  return ok;
}
