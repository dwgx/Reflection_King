import fs from "node:fs/promises";
import path from "node:path";
import { chromium, type BrowserContext, type Request, type Response } from "playwright";
import type { BrowserCandidate, CandidateKind, HeadersForUrlResponse, ProbeRequest, ProbeResponse } from "./types.js";
import type { RuntimeConfig } from "./config.js";

const MANIFEST_EXTENSIONS = [".m3u8", ".mpd"];
const MEDIA_EXTENSIONS = [".mp4", ".m4a", ".mp3", ".aac", ".wav", ".webm", ".flv", ".mov", ".mkv"];
const IMAGE_EXTENSIONS = [".jpg", ".jpeg", ".png", ".webp", ".gif", ".avif"];

interface ContextEntry {
  context: BrowserContext;
  lastUsedAt: number;
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
    const warnings: string[] = [];
    let eventCount = 0;
    let timedOut = false;

    const addCandidate = (candidate: BrowserCandidate) => {
      if (candidates.size >= maxCandidates && !candidates.has(candidate.url)) {
        return;
      }
      const current = candidates.get(candidate.url);
      if (!current || candidate.score > current.score) {
        candidates.set(candidate.url, candidate);
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
      await page.waitForTimeout(2_000);
    } catch (error) {
      timedOut = isTimeoutError(error);
      warnings.push(error instanceof Error ? error.message : String(error));
    }

    const title = await page.title().catch(() => undefined);
    const finalUrl = page.url();
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
    const target = new URL(url);
    const cookies = await context.cookies(url);
    const cookieHeader = cookies.map((cookie) => `${cookie.name}=${cookie.value}`).join("; ");
    const headers: Record<string, string> = {
      "user-agent": await contextUserAgent(context),
    };
    if (cookieHeader) {
      headers.cookie = cookieHeader;
    }
    if (referer) {
      const refererUrl = new URL(referer);
      if (sameSiteOrParent(target, refererUrl)) {
        headers.referer = referer;
        headers.origin = `${refererUrl.protocol}//${refererUrl.host}`;
      }
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

  const headers = response.headers();
  const contentType = headers["content-type"]?.split(";")[0]?.trim().toLowerCase();
  const contentLength = Number(headers["content-length"]);
  const kind = classifyCandidate(url, contentType, request);
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

function classifyCandidate(url: string, contentType: string | undefined, request: Request): CandidateKind {
  const lowerUrl = url.toLowerCase();
  if (contentType?.includes("mpegurl") || contentType?.includes("dash+xml") || hasExtension(lowerUrl, MANIFEST_EXTENSIONS)) {
    return "manifest";
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
  if (hasExtension(lowerUrl, MEDIA_EXTENSIONS)) {
    return lowerUrl.includes(".mp3") || lowerUrl.includes(".m4a") || lowerUrl.includes(".aac") || lowerUrl.includes(".wav")
      ? "audio"
      : "video";
  }
  if (hasExtension(lowerUrl, IMAGE_EXTENSIONS)) {
    return "image";
  }
  return "unknown";
}

function scoreCandidate(kind: CandidateKind, contentType: string | undefined, url: string): number {
  let score = 0;
  if (kind === "video") score += 80;
  if (kind === "audio") score += 70;
  if (kind === "manifest") score += 65;
  if (kind === "image") score += 30;
  if (kind === "html") score += 10;
  if (contentType) score += 5;
  if (/(\.m3u8|\.mpd|\.mp4|\.m4a|\.mp3)(\?|$)/i.test(url)) score += 10;
  if (/(\.ts|segment|chunk|frag)/i.test(url)) score -= 25;
  return score;
}

function qualityLabel(url: string, contentType?: string): string | undefined {
  const match = url.match(/(?:^|[^\d])([1-9]\d{2,3})p(?:[^\d]|$)/i);
  if (match) {
    return `${match[1]}p`;
  }
  return contentType;
}

function hasExtension(url: string, extensions: string[]): boolean {
  return extensions.some((extension) => url.includes(`${extension}?`) || url.endsWith(extension));
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

function sameSiteOrParent(left: URL, right: URL): boolean {
  return left.hostname === right.hostname || left.hostname.endsWith(`.${right.hostname}`) || right.hostname.endsWith(`.${left.hostname}`);
}

async function contextUserAgent(context: BrowserContext): Promise<string> {
  const page = await context.newPage();
  try {
    return await page.evaluate(() => navigator.userAgent);
  } finally {
    await page.close().catch(() => undefined);
  }
}
