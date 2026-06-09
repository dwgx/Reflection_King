import fs from "node:fs/promises";
import path from "node:path";
import { chromium, type BrowserContext, type Page, type Request, type Response } from "playwright";
import type { BrowserCandidate, CandidateKind, HeadersForUrlResponse, ProbeRequest, ProbeResponse } from "./types.js";
import type { RuntimeConfig } from "./config.js";

const MANIFEST_EXTENSIONS = [".m3u8", ".mpd"];
const MEDIA_EXTENSIONS = [".mp4", ".m4s", ".m4a", ".mp3", ".aac", ".wav", ".webm", ".flv", ".mov", ".mkv"];
const IMAGE_EXTENSIONS = [".jpg", ".jpeg", ".png", ".webp", ".gif", ".avif"];
const BILIBILI_AUDIO_QUALITY_IDS = new Set(["30216", "30232", "30280"]);
const URL_PATTERN = /https?:\/\/[^\s"'<>\\]+|(?:\/|\.\.?\/)[^\s"'<>\\]+\.(?:m3u8|mpd|mp4|m4s|m4a|mp3|aac|wav|webm|flv|mov|mkv)(?:\?[^\s"'<>\\]*)?/gi;

interface ContextEntry {
  context: BrowserContext;
  lastUsedAt: number;
}

interface DiscoveredUrl {
  url: string;
  source: string;
  contentType?: string;
  initiatorType?: string;
  contentLength?: number;
  scoreBoost?: number;
}

export class BrowserProbeService {
  private readonly contexts = new Map<string, ContextEntry>();

  constructor(private readonly config: RuntimeConfig) {}

  async probe(request: ProbeRequest): Promise<ProbeResponse> {
    const profileId = sanitizeProfileId(request.profileId ?? this.config.defaultProfileId);
    const timeoutMs = clamp(
      request.timeoutMs ?? this.config.defaultTimeoutMs,
      1_000,
      this.config.maxTimeoutMs,
    );
    const maxEvents = clamp(request.maxEvents ?? this.config.defaultMaxEvents, 25, 5_000);
    const maxCandidates = clamp(
      request.maxCandidates ?? this.config.defaultMaxCandidates,
      1,
      500,
    );
    const context = await this.context(profileId, request.headed ?? this.config.headed);
    const page = await context.newPage();
    const candidates = new Map<string, BrowserCandidate>();
    const acceptedKinds = requestedCandidateKinds(request.outputs);
    const warnings: string[] = [];
    let eventCount = 0;
    let timedOut = false;

    const addCandidate = (candidate: BrowserCandidate) => {
      if (!acceptedKinds.has(candidate.kind) || isRejectedCandidate(candidate)) {
        return;
      }
      const normalizedCandidate = {
        ...candidate,
        score: candidate.score + outputPreferenceBoost(candidate.kind, request.outputs),
      };
      if (candidates.size >= maxCandidates && !candidates.has(candidate.url)) {
        return;
      }
      const current = candidates.get(candidate.url);
      if (!current || normalizedCandidate.score > current.score) {
        candidates.set(candidate.url, normalizedCandidate);
      }
    };

    page.on("response", async (response) => {
      if (eventCount >= maxEvents) {
        return;
      }
      eventCount += 1;
      const candidate = await candidateFromResponse(response);
      if (candidate) {
        addCandidate(candidate);
      }
    });

    try {
      await page.goto(request.url, {
        waitUntil: "domcontentloaded",
        timeout: timeoutMs,
      });
      await page.waitForLoadState("networkidle", { timeout: Math.min(timeoutMs, 15_000) }).catch(() => {
        warnings.push("networkidle timeout");
      });
      if (shouldTriggerPlayback(page.url(), request.platformHint, request.outputs)) {
        await triggerPlayback(page, warnings);
      }
      await page.waitForTimeout(2_000);
    } catch (error) {
      timedOut = isTimeoutError(error);
      warnings.push(error instanceof Error ? error.message : String(error));
    }

    const finalUrl = page.url();
    if (isBilibiliUrl(finalUrl)) {
      const playInfoCandidates = await bilibiliCandidatesFromPage(page, finalUrl).catch((error) => {
        warnings.push(`bilibili playinfo parse failed: ${error instanceof Error ? error.message : String(error)}`);
        return [];
      });
      for (const candidate of playInfoCandidates) {
        addCandidate(candidate);
      }
    }

    const genericCandidates = await genericCandidatesFromPage(page, finalUrl).catch((error) => {
      warnings.push(`generic discovery failed: ${error instanceof Error ? error.message : String(error)}`);
      return [];
    });
    for (const candidate of genericCandidates) {
      addCandidate(candidate);
    }

    const title = await page.title().catch(() => undefined);
    await page.close().catch(() => undefined);

    return {
      finalUrl,
      title,
      platformHint: request.platformHint,
      candidates: [...candidates.values()].sort((a, b) => b.score - a.score),
      warnings,
      eventCount,
      timedOut,
    };
  }

  async importCookies(profileId: string, cookies: unknown): Promise<{ imported: number }> {
    const normalizedProfileId = sanitizeProfileId(profileId);
    const context = await this.context(normalizedProfileId, this.config.headed);
    if (!Array.isArray(cookies)) {
      throw new Error("cookies must be an array");
    }
    await context.addCookies(cookies as Parameters<BrowserContext["addCookies"]>[0]);
    return { imported: cookies.length };
  }

  async headersForUrl(profileId: string, url: string, referer?: string): Promise<HeadersForUrlResponse> {
    const normalizedProfileId = sanitizeProfileId(profileId || this.config.defaultProfileId);
    const context = await this.context(normalizedProfileId, this.config.headed);
    new URL(url);
    const cookies = await context.cookies(url);
    const cookieHeader = cookies.map((cookie) => `${cookie.name}=${cookie.value}`).join("; ");
    const headers: Record<string, string> = {
      "user-agent": await contextUserAgent(context),
    };
    if (cookieHeader) {
      headers.cookie = cookieHeader;
    }
    if (referer && isHttpUrl(referer)) {
      const refererUrl = new URL(referer);
      headers.referer = referer;
      headers.origin = `${refererUrl.protocol}//${refererUrl.host}`;
    }
    return { headers };
  }

  async close(): Promise<void> {
    await Promise.all([...this.contexts.values()].map((entry) => entry.context.close()));
    this.contexts.clear();
  }

  private async context(profileId: string, headed: boolean): Promise<BrowserContext> {
    const existing = this.contexts.get(profileId);
    if (existing) {
      existing.lastUsedAt = Date.now();
      return existing.context;
    }

    const profileDir = path.join(this.config.profileRoot, profileId);
    await fs.mkdir(profileDir, { recursive: true });
    const context = await chromium.launchPersistentContext(profileDir, {
      headless: !headed,
      viewport: { width: 1366, height: 768 },
      locale: "zh-CN",
      userAgent:
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124 Safari/537.36",
    });
    this.contexts.set(profileId, { context, lastUsedAt: Date.now() });
    return context;
  }
}

async function candidateFromResponse(response: Response): Promise<BrowserCandidate | undefined> {
  const request = response.request();
  const url = response.url();
  if (!isHttpUrl(url)) {
    return undefined;
  }
  const parsedUrl = new URL(url);

  const headers = response.headers();
  const contentType = headers["content-type"]?.split(";")[0]?.trim().toLowerCase();
  const contentLength = Number(headers["content-length"]);
  const kind = classifyCandidate(parsedUrl, contentType, request);
  if (kind === "unknown") {
    return undefined;
  }

  return {
    url,
    kind,
    method: request.method(),
    status: response.status(),
    contentType,
    contentLength: Number.isFinite(contentLength) ? contentLength : undefined,
    resourceType: request.resourceType(),
    initiatorUrl: request.frame()?.url(),
    qualityLabel: qualityLabel(url, contentType),
    score: scoreCandidate(kind, contentType, url),
    requiresAuthorization: Boolean(headers["set-cookie"] || request.headers().cookie),
  };
}

function classifyCandidate(url: URL, contentType: string | undefined, request: Request): CandidateKind {
  const lowerPath = url.pathname.toLowerCase();
  const acceptHeader = request.headers().accept?.toLowerCase();
  if (isManifestCandidate(lowerPath, contentType)) {
    return "manifest";
  }
  if (hasExtension(lowerPath, [".m4s"])) {
    const mediaId = m4sMediaId(lowerPath);
    return mediaId && BILIBILI_AUDIO_QUALITY_IDS.has(mediaId) ? "audio" : "video";
  }
  if (contentType?.startsWith("audio/")) {
    return "audio";
  }
  if (contentType?.startsWith("video/")) {
    return "video";
  }
  if (contentType?.startsWith("image/")) {
    return "image";
  }
  if (contentType?.includes("text/html") && request.resourceType() === "document") {
    return "html";
  }
  if (request.resourceType() === "media" && acceptHeader?.includes("audio")) {
    return "audio";
  }
  if (request.resourceType() === "media") {
    return "video";
  }
  if (hasExtension(lowerPath, MEDIA_EXTENSIONS)) {
    return lowerPath.includes(".mp3") || lowerPath.includes(".m4a") || lowerPath.includes(".aac") || lowerPath.includes(".wav")
      ? "audio"
      : "video";
  }
  if (hasExtension(lowerPath, IMAGE_EXTENSIONS)) {
    return "image";
  }
  return "unknown";
}

function classifyDiscoveredCandidate(url: URL, contentType: string | undefined, initiatorType: string | undefined): CandidateKind {
  const lowerPath = url.pathname.toLowerCase();
  const lowerInitiator = initiatorType?.toLowerCase();
  const hintedContentType = contentType ?? contentTypeFromUrlHint(url);
  if (isManifestCandidate(lowerPath, hintedContentType)) {
    return "manifest";
  }
  if (hasExtension(lowerPath, [".m4s"])) {
    const mediaId = m4sMediaId(lowerPath);
    return mediaId && BILIBILI_AUDIO_QUALITY_IDS.has(mediaId) ? "audio" : "video";
  }
  if (hintedContentType?.startsWith("audio/") || lowerInitiator === "audio") {
    return "audio";
  }
  if (hintedContentType?.startsWith("video/") || lowerInitiator === "video" || lowerInitiator === "media") {
    return "video";
  }
  if (hintedContentType?.startsWith("image/") || lowerInitiator === "img" || lowerInitiator === "image") {
    return "image";
  }
  if (hasExtension(lowerPath, MEDIA_EXTENSIONS)) {
    return lowerPath.includes(".mp3") || lowerPath.includes(".m4a") || lowerPath.includes(".aac") || lowerPath.includes(".wav")
      ? "audio"
      : "video";
  }
  if (hasExtension(lowerPath, IMAGE_EXTENSIONS)) {
    return "image";
  }
  return "unknown";
}

function isManifestCandidate(pathname: string, contentType: string | undefined): boolean {
  return Boolean(contentType?.includes("mpegurl") || contentType?.includes("dash+xml") || hasExtension(pathname, MANIFEST_EXTENSIONS));
}

function contentTypeFromUrlHint(url: URL): string | undefined {
  for (const key of ["mime", "type", "content_type", "contentType"]) {
    const value = url.searchParams.get(key)?.toLowerCase();
    if (
      value?.startsWith("audio/")
      || value?.startsWith("video/")
      || value?.startsWith("image/")
      || value?.includes("mpegurl")
      || value?.includes("dash+xml")
    ) {
      return value;
    }
  }
  return undefined;
}

function scoreCandidate(kind: CandidateKind, contentType: string | undefined, url: string): number {
  let score = 0;
  const path = urlPath(url).toLowerCase();
  if (kind === "video") score += 80;
  if (kind === "audio") score += 70;
  if (kind === "manifest") score += 65;
  if (kind === "image") score += 30;
  if (kind === "html") score += 10;
  if (contentType) score += 5;
  if (/(\.m3u8|\.mpd|\.mp4|\.m4s|\.m4a|\.mp3)$/i.test(path)) score += 10;
  if (/(\.ts|segment|chunk|frag)/i.test(path)) score -= 25;
  return score;
}

function qualityLabel(url: string, contentType?: string): string | undefined {
  const path = urlPath(url);
  const mediaId = m4sMediaId(path);
  if (mediaId) {
    return BILIBILI_AUDIO_QUALITY_IDS.has(mediaId)
      ? `bilibili-audio-${mediaId}`
      : `bilibili-video-${mediaId}`;
  }
  const match = url.match(/(?:^|[^\d])([1-9]\d{2,3})p(?:[^\d]|$)/i);
  if (match) {
    return `${match[1]}p`;
  }
  return contentType;
}

async function bilibiliCandidatesFromPage(page: Page, pageUrl: string): Promise<BrowserCandidate[]> {
  const playInfo = await page.evaluate(() => {
    const currentWindow = window as Window & { __playinfo__?: unknown };
    return currentWindow.__playinfo__;
  });
  const candidates = candidatesFromBilibiliPlayInfo(playInfo, pageUrl);
  const apiCandidates = await bilibiliCandidatesFromApi(page, pageUrl).catch(() => []);
  return [...candidates, ...apiCandidates];
}

function candidatesFromBilibiliPlayInfo(playInfo: unknown, pageUrl: string): BrowserCandidate[] {
  const root = unwrapBilibiliPlayInfo(playInfo);
  const dash = asRecord(root?.dash);
  const candidates: BrowserCandidate[] = [];

  for (const entry of asArray(dash?.video)) {
    const candidate = candidateFromBilibiliDashEntry(entry, "video", pageUrl);
    if (candidate) {
      candidates.push(candidate);
    }
  }

  for (const entry of asArray(dash?.audio)) {
    const candidate = candidateFromBilibiliDashEntry(entry, "audio", pageUrl);
    if (candidate) {
      candidates.push(candidate);
    }
  }

  for (const entry of asArray(root?.durl)) {
    const candidate = candidateFromBilibiliDurlEntry(entry, pageUrl);
    if (candidate) {
      candidates.push(candidate);
    }
  }

  return candidates;
}

function unwrapBilibiliPlayInfo(playInfo: unknown): Record<string, unknown> | undefined {
  const record = asRecord(playInfo);
  if (!record) {
    return undefined;
  }
  return asRecord(record.data) ?? asRecord(record.result) ?? record;
}

function candidateFromBilibiliDashEntry(entry: unknown, kind: "audio" | "video", pageUrl: string): BrowserCandidate | undefined {
  const record = asRecord(entry);
  const url = firstString(record?.baseUrl, record?.base_url);
  if (!url || !isHttpUrl(url)) {
    return undefined;
  }

  const mimeType = firstString(record?.mimeType, record?.mime_type) ?? (kind === "audio" ? "audio/mp4" : "video/mp4");
  const mediaId = firstString(record?.id) ?? m4sMediaId(urlPath(url));
  const height = asNumber(record?.height);
  const bandwidth = asNumber(record?.bandwidth);
  const width = asNumber(record?.width);
  const codecs = firstString(record?.codecs);
  return {
    url,
    kind,
    method: "GET",
    contentType: mimeType,
    contentLength: undefined,
    resourceType: "bilibili_playinfo",
    initiatorUrl: pageUrl,
    qualityLabel: kind === "video"
      ? (height ? `${height}p` : mediaId ? `bilibili-video-${mediaId}` : mimeType)
      : mediaId ? `bilibili-audio-${mediaId}` : bandwidth ? `audio-${bandwidth}` : mimeType,
    score: scoreCandidate(kind, mimeType, url) + 35 + (height ? Math.min(Math.floor(height / 20), 60) : 0),
    requiresAuthorization: false,
    metadata: {
      source: "bilibili_playinfo",
      mediaId,
      height,
      width,
      bandwidth,
      codecs,
      backupUrls: asArray(record?.backupUrl ?? record?.backup_url).filter((value): value is string => typeof value === "string"),
    },
  };
}

function candidateFromBilibiliDurlEntry(entry: unknown, pageUrl: string): BrowserCandidate | undefined {
  const record = asRecord(entry);
  const url = firstString(record?.url);
  if (!url || !isHttpUrl(url)) {
    return undefined;
  }
  const kind: CandidateKind = "video";
  return {
    url,
    kind,
    method: "GET",
    contentType: "video/mp4",
    contentLength: asNumber(record?.size),
    resourceType: "bilibili_playinfo",
    initiatorUrl: pageUrl,
    qualityLabel: qualityLabel(url, "video/mp4"),
    score: scoreCandidate(kind, "video/mp4", url) + 30,
    requiresAuthorization: false,
    metadata: {
      source: "bilibili_durl",
      size: asNumber(record?.size),
    },
  };
}

async function bilibiliCandidatesFromApi(page: Page, pageUrl: string): Promise<BrowserCandidate[]> {
  const ids = bilibiliIdsFromUrl(pageUrl);
  if (!ids.bvid && !ids.aid) {
    return [];
  }

  const response = await page.evaluate(async ({ bvid, aid }) => {
    const viewParams = bvid ? `bvid=${encodeURIComponent(bvid)}` : `aid=${encodeURIComponent(String(aid))}`;
    const view = await fetch(`https://api.bilibili.com/x/web-interface/view?${viewParams}`, {
      credentials: "include",
      headers: { accept: "application/json, text/plain, */*" },
    }).then((item) => item.json());
    const cid = view?.data?.cid;
    if (!cid) {
      return { view, play: null };
    }
    const idParams = bvid ? `bvid=${encodeURIComponent(bvid)}` : `avid=${encodeURIComponent(String(aid))}`;
    const play = await fetch(`https://api.bilibili.com/x/player/playurl?${idParams}&cid=${encodeURIComponent(String(cid))}&qn=120&fnval=4048&fourk=1`, {
      credentials: "include",
      headers: { accept: "application/json, text/plain, */*" },
    }).then((item) => item.json());
    return { view, play };
  }, ids);

  const play = asRecord(response)?.play;
  return candidatesFromBilibiliPlayInfo(play, pageUrl).map((candidate) => ({
    ...candidate,
    resourceType: "bilibili_api",
    score: candidate.score + 8,
    metadata: {
      ...(candidate.metadata ?? {}),
      source: "bilibili_api",
    },
  }));
}

async function genericCandidatesFromPage(page: Page, pageUrl: string): Promise<BrowserCandidate[]> {
  const discovered = await page.evaluate((urlPatternSource) => {
    type BrowserDiscoveredUrl = {
      url: string;
      source: string;
      contentType?: string;
      initiatorType?: string;
      contentLength?: number;
      scoreBoost?: number;
    };

    const found = new Map<string, BrowserDiscoveredUrl>();
    const add = (value: string | null | undefined, source: string, options: Omit<BrowserDiscoveredUrl, "url" | "source"> = {}) => {
      if (!value || value.startsWith("blob:") || value.startsWith("data:")) {
        return;
      }
      try {
        const url = new URL(value, document.baseURI).href;
        if (!url.startsWith("http://") && !url.startsWith("https://")) {
          return;
        }
        const current = found.get(url);
        const candidate = { url, source, ...options };
        if (!current || (candidate.scoreBoost ?? 0) > (current.scoreBoost ?? 0)) {
          found.set(url, candidate);
        }
      } catch {
        return;
      }
    };

    for (const element of Array.from(document.querySelectorAll("video,audio"))) {
      const media = element as HTMLMediaElement;
      const source = element.tagName.toLowerCase();
      add(media.currentSrc || media.getAttribute("src"), `dom_${source}`, {
        initiatorType: source,
        scoreBoost: 30,
      });
      for (const child of Array.from(element.querySelectorAll("source"))) {
        add(child.getAttribute("src") || (child as HTMLSourceElement).src, `dom_${source}_source`, {
          contentType: child.getAttribute("type") || undefined,
          initiatorType: source,
          scoreBoost: 28,
        });
      }
    }

    for (const source of Array.from(document.querySelectorAll("source[src]"))) {
      add(source.getAttribute("src"), "dom_source", {
        contentType: source.getAttribute("type") || undefined,
        scoreBoost: 24,
      });
    }

    for (const link of Array.from(document.querySelectorAll("link[href]"))) {
      const asValue = link.getAttribute("as")?.toLowerCase();
      const typeValue = link.getAttribute("type") || undefined;
      if (asValue === "audio" || asValue === "video" || asValue === "image" || typeValue?.startsWith("audio/") || typeValue?.startsWith("video/")) {
        add(link.getAttribute("href"), `dom_link_${asValue || "resource"}`, {
          contentType: typeValue,
          initiatorType: asValue,
          scoreBoost: 18,
        });
      }
    }

    for (const anchor of Array.from(document.querySelectorAll("a[href]"))) {
      add(anchor.getAttribute("href"), "dom_anchor", { scoreBoost: 12 });
    }

    for (const meta of Array.from(document.querySelectorAll("meta[property],meta[name]"))) {
      const key = (meta.getAttribute("property") || meta.getAttribute("name") || "").toLowerCase();
      if (key.includes("video") || key.includes("audio") || key.includes("image")) {
        add(meta.getAttribute("content"), `dom_meta_${key.replace(/[^a-z0-9_-]/g, "_")}`, { scoreBoost: 18 });
      }
    }

    for (const link of Array.from(document.querySelectorAll("link[href]"))) {
      const rel = (link.getAttribute("rel") || "").toLowerCase();
      const asValue = (link.getAttribute("as") || "").toLowerCase();
      if (rel.includes("preload") || rel.includes("prefetch") || asValue === "audio" || asValue === "video") {
        add(link.getAttribute("href"), `dom_link_${asValue || rel.replace(/[^a-z0-9_-]/g, "_")}`, {
          initiatorType: asValue || undefined,
          scoreBoost: asValue === "audio" || asValue === "video" ? 20 : 8,
        });
      }
    }

    for (const entry of performance.getEntriesByType("resource")) {
      const resource = entry as PerformanceResourceTiming;
      add(resource.name, `performance_${resource.initiatorType || "resource"}`, {
        initiatorType: resource.initiatorType,
        contentLength: resource.decodedBodySize || resource.transferSize || undefined,
        scoreBoost: resource.initiatorType === "video" || resource.initiatorType === "audio" ? 22 : 10,
      });
    }

    const urlPattern = new RegExp(urlPatternSource, "gi");
    const scanJsonValue = (value: unknown, source: string, depth = 0) => {
      if (depth > 8 || value === null || value === undefined) {
        return;
      }
      if (typeof value === "string") {
        add(value, source, { scoreBoost: 20 });
        return;
      }
      if (Array.isArray(value)) {
        for (const item of value.slice(0, 500)) {
          scanJsonValue(item, source, depth + 1);
        }
        return;
      }
      if (typeof value === "object") {
        for (const item of Object.values(value as Record<string, unknown>).slice(0, 500)) {
          scanJsonValue(item, source, depth + 1);
        }
      }
    };

    for (const script of Array.from(document.scripts)) {
      if (script.src) {
        add(script.src, "dom_script_src", { scoreBoost: 4 });
        continue;
      }
      const text = (script.textContent || "")
        .replace(/\\u002[fF]/g, "/")
        .replace(/\\u0026/g, "&")
        .replace(/\\u003[dD]/g, "=")
        .replace(/\\\//g, "/")
        .slice(0, 1_000_000);
      const type = script.type?.toLowerCase() ?? "";
      if (type.includes("json") || text.trimStart().startsWith("{") || text.trimStart().startsWith("[")) {
        try {
          scanJsonValue(JSON.parse(text), `inline_json_${type || "script"}`);
        } catch {
          // Fall back to bounded URL regex extraction below.
        }
      }
      for (const match of text.matchAll(urlPattern)) {
        add(match[0], "inline_script_url", { scoreBoost: 16 });
      }
    }

    return Array.from(found.values()).slice(0, 750);
  }, URL_PATTERN.source);

  return discovered
    .map((candidate) => candidateFromDiscoveredUrl(candidate, pageUrl))
    .filter((candidate): candidate is BrowserCandidate => Boolean(candidate));
}

function candidateFromDiscoveredUrl(discovered: DiscoveredUrl, pageUrl: string): BrowserCandidate | undefined {
  if (!isHttpUrl(discovered.url)) {
    return undefined;
  }

  const parsedUrl = new URL(discovered.url);
  const contentType = discovered.contentType?.split(";")[0]?.trim().toLowerCase();
  const kind = classifyDiscoveredCandidate(parsedUrl, contentType, discovered.initiatorType);
  if (kind === "unknown") {
    return undefined;
  }

  return {
    url: discovered.url,
    kind,
    method: "GET",
    contentType,
    contentLength: Number.isFinite(discovered.contentLength) ? discovered.contentLength : undefined,
    resourceType: discovered.source,
    initiatorUrl: pageUrl,
    qualityLabel: qualityLabel(discovered.url, contentType),
    score: scoreCandidate(kind, contentType, discovered.url) + (discovered.scoreBoost ?? 0),
    requiresAuthorization: false,
  };
}

function requestedCandidateKinds(outputs: string[] | undefined): ReadonlySet<CandidateKind> {
  const normalizedOutputs = outputs?.length ? outputs : ["audio"];
  const kinds = new Set<CandidateKind>();

  for (const output of normalizedOutputs) {
    switch (output) {
      case "audio":
        kinds.add("audio");
        kinds.add("video");
        kinds.add("manifest");
        break;
      case "video":
        kinds.add("video");
        kinds.add("manifest");
        kinds.add("audio");
        break;
      case "image":
        kinds.add("image");
        break;
      case "page_html":
      case "html":
      case "markdown":
        kinds.add("html");
        break;
      default:
        break;
    }
  }

  return kinds.size > 0 ? kinds : new Set<CandidateKind>(["audio"]);
}

function outputPreferenceBoost(kind: CandidateKind, outputs: string[] | undefined): number {
  const normalizedOutputs = outputs?.length ? outputs : ["audio"];
  const wantsAudio = normalizedOutputs.includes("audio");
  const wantsVideo = normalizedOutputs.includes("video");

  if (wantsAudio && !wantsVideo) {
    if (kind === "audio") return 60;
    if (kind === "video" || kind === "manifest") return 5;
  }

  if (wantsVideo && !wantsAudio) {
    if (kind === "video" || kind === "manifest") return 60;
    if (kind === "audio") return 15;
  }

  return 0;
}

function isRejectedCandidate(candidate: BrowserCandidate): boolean {
  try {
    const parsedUrl = new URL(candidate.url);
    const host = parsedUrl.hostname.toLowerCase();
    const pathname = parsedUrl.pathname.toLowerCase();

    if (isYouTubeHost(host) && pathname.startsWith("/s/search/audio/")) {
      return true;
    }
  } catch {
    return true;
  }
  return false;
}

function hasExtension(pathname: string, extensions: string[]): boolean {
  return extensions.some((extension) => pathname.endsWith(extension));
}

function m4sMediaId(pathname: string): string | undefined {
  return pathname.match(/-(\d+)\.m4s$/i)?.[1];
}

function urlPath(url: string): string {
  try {
    return new URL(url).pathname;
  } catch {
    return url;
  }
}

function isBilibiliUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "bilibili.com" || host.endsWith(".bilibili.com");
  } catch {
    return false;
  }
}

function bilibiliIdsFromUrl(value: string): { bvid?: string; aid?: string } {
  try {
    const url = new URL(value);
    const pathMatch = url.pathname.match(/\/video\/(BV[a-zA-Z0-9]+|av\d+)/i);
    const id = pathMatch?.[1];
    if (!id) {
      return {};
    }
    if (id.toLowerCase().startsWith("av")) {
      return { aid: id.slice(2) };
    }
    return { bvid: id };
  } catch {
    return {};
  }
}

function shouldTriggerPlayback(url: string, platformHint: string | undefined, outputs: string[] | undefined): boolean {
  const wantsMedia = !outputs?.length || outputs.some((output) => output === "audio" || output === "video");
  if (!wantsMedia) {
    return false;
  }

  const platform = platformHint?.toLowerCase();
  return platform === "soundcloud" || platform === "youtube" || isSoundCloudUrl(url) || isYouTubeUrl(url);
}

async function triggerPlayback(page: Page, warnings: string[]): Promise<void> {
  await dismissConsentPrompt(page);

  const clicked = await clickFirstVisible(page, [
    "button[aria-label='Play']",
    "button[aria-label^='Play ']",
    "button[title='Play']",
    "button.playButton",
    ".playButton",
    ".sc-button-play",
    "[data-testid='play-button']",
    "button.ytp-large-play-button",
    "button.ytp-play-button",
  ]);

  if (!clicked) {
    await page.locator("video").first().click({ timeout: 1_000 }).catch(() => undefined);
  }

  await page.waitForTimeout(5_000);
}

async function dismissConsentPrompt(page: Page): Promise<void> {
  for (const name of [/accept all/i, /i agree/i, /^agree$/i, /allow all/i, /reject all/i]) {
    const button = page.getByRole("button", { name }).first();
    if (await isVisible(button)) {
      await button.click({ timeout: 1_000 }).catch(() => undefined);
      await page.waitForTimeout(500);
      return;
    }
  }
}

async function clickFirstVisible(page: Page, selectors: string[]): Promise<boolean> {
  for (const selector of selectors) {
    const locator = page.locator(selector).first();
    if (await isVisible(locator)) {
      await locator.click({ timeout: 2_000 }).catch(() => undefined);
      return true;
    }
  }
  return false;
}

async function isVisible(locator: ReturnType<Page["locator"]>): Promise<boolean> {
  return await locator.isVisible({ timeout: 500 }).catch(() => false);
}

function isSoundCloudUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "soundcloud.com" || host.endsWith(".soundcloud.com");
  } catch {
    return false;
  }
}

function isYouTubeUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return isYouTubeHost(host) || host === "youtu.be" || host.endsWith(".googlevideo.com");
  } catch {
    return false;
  }
}

function isYouTubeHost(host: string): boolean {
  return host === "youtube.com" || host.endsWith(".youtube.com");
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : undefined;
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function firstString(...values: unknown[]): string | undefined {
  for (const value of values) {
    if (typeof value === "string" && value) {
      return value;
    }
    if (typeof value === "number" && Number.isFinite(value)) {
      return String(value);
    }
  }
  return undefined;
}

function asNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function sanitizeProfileId(value: string): string {
  const sanitized = value.replace(/[^a-zA-Z0-9_-]/g, "_").slice(0, 64);
  return sanitized || "admin_default";
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

function isHttpUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

function isTimeoutError(error: unknown): boolean {
  return error instanceof Error && error.message.toLowerCase().includes("timeout");
}

async function contextUserAgent(context: BrowserContext): Promise<string> {
  const page = await context.newPage();
  try {
    return await page.evaluate(() => navigator.userAgent);
  } finally {
    await page.close().catch(() => undefined);
  }
}
