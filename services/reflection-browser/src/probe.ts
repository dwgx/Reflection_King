import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import os from "node:os";
import dns from "node:dns/promises";
import { chromium, type BrowserContext, type CDPSession, type Page, type Request, type Response } from "playwright";
import type {
  BrowserCandidate,
  CandidateKind,
  CookiesForUrlResponse,
  HeadersForUrlResponse,
  LoginSessionSnapshot,
  LoginSessionView,
  PageResource,
  PageSnapshot,
  ProbeRequest,
  ProbeResponse,
} from "./types.js";
import type { RuntimeConfig } from "./config.js";

const MANIFEST_EXTENSIONS = [".m3u8", ".mpd"];
const MEDIA_EXTENSIONS = [".mp4", ".m4s", ".m4a", ".mp3", ".aac", ".wav", ".webm", ".flv", ".mov", ".mkv"];
const IMAGE_EXTENSIONS = [".jpg", ".jpeg", ".png", ".webp", ".gif", ".avif"];
const ARCHIVE_RESPONSE_BODY_MAX_BYTES = 2 * 1024 * 1024;
const BILIBILI_AUDIO_QUALITY_IDS = new Set(["30216", "30232", "30280"]);
const URL_PATTERN = /https?:\/\/[^\s"'<>\\]+|(?:\/|\.\.?\/)[^\s"'<>\\]+\.(?:m3u8|mpd|mp4|m4s|m4a|mp3|aac|wav|webm|flv|mov|mkv)(?:\?[^\s"'<>\\]*)?/gi;
const AD_HOST_PARTS = [
  "trafficjunky",
  "doubleclick",
  "googlesyndication",
  "googleadservices",
  "adservice.google",
  "adsystem",
  "adnxs",
  "exoclick",
  "popads",
  "taboola",
  "outbrain",
  "imasdk",
  "adform",
  "pubmatic",
  "rubiconproject",
  "openx",
  "smartadserver",
  "scorecardresearch",
];
const AD_PATH_PATTERN = /(?:^|[/?&._-])(ads?|adserver|advert|advertising|banner|creative|pre[-_]?roll|mid[-_]?roll|post[-_]?roll|vast|vpaid|ima|tracking|tracker|pixel)(?:$|[/?&._=-])/i;

interface ContextEntry {
  context: BrowserContext;
  lastUsedAt: number;
}

interface LoginSessionEntry {
  id: string;
  profileId: string;
  page: Page;
  createdAt: number;
  lastActiveAt: number;
  expiresAt: number;
}

interface DiscoveredUrl {
  url: string;
  source: string;
  contentType?: string;
  initiatorType?: string;
  contentLength?: number;
  scoreBoost?: number;
}

interface CdpResourceRecord {
  requestId: string;
  url: string;
  method?: string;
  status?: number;
  contentType?: string;
  contentLength?: number;
  resourceType?: string;
  initiatorUrl?: string;
  initiatorType?: string;
  frameUrl?: string;
  redirectChain: string[];
  requestHeaders?: Record<string, string>;
  responseHeaders?: Record<string, string>;
  servedFromCache?: boolean;
  fromServiceWorker?: boolean;
  bodyBase64?: string;
  bodySource?: string;
  startedAt?: number;
  endedAt?: number;
}

interface CdpCapture {
  session: CDPSession;
  resources: Map<string, CdpResourceRecord>;
  pendingBodies: Set<Promise<void>>;
  warnings: string[];
  bodyBytes: number;
  bodyMaxBytes: number;
  bodyTotalBytes: number;
}

export class BrowserProbeService {
  private readonly contexts = new Map<string, ContextEntry>();
  private readonly loginSessions = new Map<string, LoginSessionEntry>();
  private readonly blockedHosts = new Set<string>();

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
    const pageResources = new Map<string, PageResource>();
    const pendingPageResourceTasks = new Set<Promise<void>>();
    const acceptedKinds = requestedCandidateKinds(request.outputs);
    const capturePageSnapshot = request.outputs?.includes("page_html") ?? false;
    const warnings: string[] = [];
    const consoleErrors: string[] = [];
    const cdpCaptureEnabled = Boolean(capturePageSnapshot && (request.captureCdp ?? true));
    const cdpBodyMaxBytes = clamp(
      request.cdpBodyMaxBytes ?? ARCHIVE_RESPONSE_BODY_MAX_BYTES,
      64 * 1024,
      256 * 1024 * 1024,
    );
    const cdpBodyTotalBytes = clamp(
      request.cdpBodyTotalBytes ?? 64 * 1024 * 1024,
      64 * 1024,
      4 * 1024 * 1024 * 1024,
    );
    const cdpCapture = cdpCaptureEnabled
      ? await startCdpCapture(page, cdpBodyMaxBytes, cdpBodyTotalBytes).catch((error) => {
          warnings.push(`cdp capture failed: ${error instanceof Error ? error.message : String(error)}`);
          return undefined;
        })
      : undefined;
    let archivedResponseBodyBytes = 0;
    let eventCount = 0;
    let timedOut = false;
    let playbackTriggered = false;

    page.on("console", (message) => {
      if (message.type() === "error" && consoleErrors.length < 50) {
        consoleErrors.push(message.text().slice(0, 300));
      }
    });
    page.on("pageerror", (error) => {
      if (consoleErrors.length < 50) {
        consoleErrors.push(`pageerror: ${error.message}`.slice(0, 300));
      }
    });

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

    page.on("response", (response) => {
      if (eventCount >= maxEvents) {
        return;
      }
      eventCount += 1;
      const task = (async () => {
        const resource = await pageResourceFromResponse(
          response,
          capturePageSnapshot,
          cdpBodyMaxBytes,
          cdpBodyTotalBytes - archivedResponseBodyBytes,
        );
        if (resource?.bodyBase64) {
          const bytes = Buffer.byteLength(resource.bodyBase64, "base64");
          if (archivedResponseBodyBytes + bytes <= cdpBodyTotalBytes) {
            archivedResponseBodyBytes += bytes;
          } else {
            delete resource.bodyBase64;
          }
        }
        addPageResource(pageResources, resource);
        const candidate = await candidateFromResponse(response).catch((error) => {
          if (capturePageSnapshot) {
            warnings.push(`candidate response scan failed: ${error instanceof Error ? error.message : String(error)}`);
          }
          return undefined;
        });
        if (candidate) {
          addCandidate(candidate);
        }
      })();
      pendingPageResourceTasks.add(task);
      task.finally(() => pendingPageResourceTasks.delete(task)).catch(() => undefined);
    });

    try {
      await page.goto(request.url, {
        waitUntil: capturePageSnapshot ? "commit" : "domcontentloaded",
        timeout: capturePageSnapshot ? Math.min(timeoutMs, 20_000) : timeoutMs,
      });
      await page.waitForLoadState("networkidle", { timeout: capturePageSnapshot ? 5_000 : Math.min(timeoutMs, 15_000) }).catch(() => {
        warnings.push("networkidle timeout");
      });
      if (capturePageSnapshot) {
        await preparePageForArchive(page, warnings);
      }
      await addDomCandidates(page, page.url(), warnings, addCandidate);
      const hasStaticPlayableCandidate = hasPlayableMediaCandidate([...candidates.values()]);
      if (
        shouldTriggerPlayback(page.url(), request.platformHint, request.outputs, hasStaticPlayableCandidate) &&
        !shouldAvoidGenericPlaybackClick(page.url(), request.url, hasStaticPlayableCandidate)
      ) {
        playbackTriggered = await triggerPlayback(page, warnings);
      } else if (hasStaticPlayableCandidate) {
        warnings.push("playback trigger skipped: page already exposed media candidates");
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

    await addDomCandidates(page, finalUrl, warnings, addCandidate);

    const adultVideoCandidates = await adultVideoCandidatesFromPage(page, finalUrl).catch((error) => {
      warnings.push(`adult video discovery failed: ${error instanceof Error ? error.message : String(error)}`);
      return [];
    });
    for (const candidate of adultVideoCandidates) {
      addCandidate(candidate);
    }

    const title = await page.title().catch(() => undefined);
    await withTimeout(Promise.allSettled([...pendingPageResourceTasks]), 3_000).catch(() => {
      warnings.push("page resource body capture timeout");
    });
    if (cdpCapture) {
      await withTimeout(Promise.allSettled([...cdpCapture.pendingBodies]), 3_000).catch(() => {
        warnings.push("cdp response body capture timeout");
      });
      for (const warning of cdpCapture.warnings) {
        warnings.push(warning);
      }
      for (const resource of cdpResourcesToPageResources(cdpCapture)) {
        addPageResource(pageResources, resource);
      }
    }
    if (capturePageSnapshot) {
      for (const resource of await genericPageResourcesFromPage(page, finalUrl).catch((error) => {
        warnings.push(`page resource scan failed: ${error instanceof Error ? error.message : String(error)}`);
        return [];
      })) {
        addPageResource(pageResources, resource);
      }
      await enrichPageResourcesFromBrowser(page, pageResources, warnings, cdpBodyMaxBytes, cdpBodyTotalBytes);
    }
    const mhtmlBase64 = capturePageSnapshot && (request.saveMhtml ?? true)
      ? await captureMhtmlSnapshot(page, warnings)
      : undefined;
    const harJson = capturePageSnapshot && (request.saveHar ?? true) && cdpCapture
      ? buildHarFromCdp(cdpCapture, page.url(), title)
      : undefined;
    const pageSnapshot = await captureSnapshot(
      page,
      finalUrl,
      title,
      [...pageResources.values()],
      warnings,
      { includeScreenshot: capturePageSnapshot, mhtmlBase64, harJson },
    );
    await cdpCapture?.session.detach().catch(() => undefined);
    const userAgent = await contextUserAgent(context).catch(() => undefined);
    await page.close().catch(() => undefined);

    // A page URL is never a media file: drop candidates whose URL is the page
    // (or final) URL itself — this is the generic-scan false positive that made
    // JS-resolved sites like StreetVoice look "solved" when they were not.
    const pageUrls = new Set([request.url, finalUrl].map(normalizeUrlForCompare));
    let filtered = [...candidates.values()].filter(
      (candidate) => !pageUrls.has(normalizeUrlForCompare(candidate.url)),
    );
    // Douyin renders behind anti-bot/RSC, so the generic scan returns lots of
    // page noise. Keep only the real post media on Douyin's CDNs.
    if (hasPlatformHint(request.platformHint, "douyin") || isDouyinUrl(request.url) || isDouyinUrl(finalUrl)) {
      filtered = filterDouyinCandidates(filtered);
    }
    if (hasPlatformHint(request.platformHint, "kuaishou") || isKuaishouUrl(request.url) || isKuaishouUrl(finalUrl)) {
      filtered = filterKuaishouCandidates(filtered);
    }
    if (isAdultVideoPage(request.url) || isAdultVideoPage(finalUrl)) {
      filtered = filterAdultVideoCandidates(filtered);
    }
    if (hasPlatformHint(request.platformHint, "acfun") || isAcfunUrl(request.url) || isAcfunUrl(finalUrl)) {
      filtered = filterCnVideoPlatformCandidates(filtered, "acfun");
    }
    if (hasPlatformHint(request.platformHint, "iqiyi") || isIqiyiUrl(request.url) || isIqiyiUrl(finalUrl)) {
      filtered = filterCnVideoPlatformCandidates(filtered, "iqiyi");
    }
    if (hasPlatformHint(request.platformHint, "youku") || isYoukuUrl(request.url) || isYoukuUrl(finalUrl)) {
      filtered = filterCnVideoPlatformCandidates(filtered, "youku");
    }
    if (isHAnimeUrl(request.url) || isHAnimeUrl(finalUrl)) {
      filtered = filterHAnimeCandidates(filtered);
    }
    if (hasPlatformHint(request.platformHint, "tiktok") || isTikTokUrl(request.url) || isTikTokUrl(finalUrl)) {
      filtered = filterTikTokCandidates(filtered);
    }
    if (hasPlatformHint(request.platformHint, "vimeo") || isVimeoUrl(request.url) || isVimeoUrl(finalUrl)) {
      filtered = filterVimeoCandidates(filtered);
    }
    if (hasPlatformHint(request.platformHint, "weibo") || isWeiboUrl(request.url) || isWeiboUrl(finalUrl)) {
      filtered = filterWeiboCandidates(filtered);
    }
    if (hasPlatformHint(request.platformHint, "ximalaya") || isXimalayaUrl(request.url) || isXimalayaUrl(finalUrl)) {
      filtered = filterXimalayaCandidates(filtered);
    }
    if (hasPlatformHint(request.platformHint, "dailymotion") || isDailymotionUrl(request.url) || isDailymotionUrl(finalUrl)) {
      filtered = filterVideoSocialCandidates(filtered, "dailymotion");
    }
    if (hasPlatformHint(request.platformHint, "rumble") || isRumbleUrl(request.url) || isRumbleUrl(finalUrl)) {
      filtered = filterVideoSocialCandidates(filtered, "rumble");
    }
    if (hasPlatformHint(request.platformHint, "peertube") || isPeertubeUrl(request.url) || isPeertubeUrl(finalUrl)) {
      filtered = filterVideoSocialCandidates(filtered, "peertube");
    }
    if (hasPlatformHint(request.platformHint, "archive_org") || isArchiveOrgUrl(request.url) || isArchiveOrgUrl(finalUrl)) {
      filtered = filterLibraryArchiveCandidates(filtered, "archive_org");
    }
    if (hasPlatformHint(request.platformHint, "wayback") || isWaybackUrl(request.url) || isWaybackUrl(finalUrl)) {
      filtered = filterLibraryArchiveCandidates(filtered, "wayback");
    }
    if (hasPlatformHint(request.platformHint, "archive_it") || isArchiveItUrl(request.url) || isArchiveItUrl(finalUrl)) {
      filtered = filterLibraryArchiveCandidates(filtered, "archive_it");
    }
    if (hasPlatformHint(request.platformHint, "perma_cc") || isPermaCcUrl(request.url) || isPermaCcUrl(finalUrl)) {
      filtered = filterLibraryArchiveCandidates(filtered, "perma_cc");
    }
    if (hasPlatformHint(request.platformHint, "archive_today") || isArchiveTodayUrl(request.url) || isArchiveTodayUrl(finalUrl)) {
      filtered = filterLibraryArchiveCandidates(filtered, "archive_today");
    }
    if (hasPlatformHint(request.platformHint, "ghostarchive") || isGhostarchiveUrl(request.url) || isGhostarchiveUrl(finalUrl)) {
      filtered = filterLibraryArchiveCandidates(filtered, "ghostarchive");
    }
    if (hasPlatformHint(request.platformHint, "webcitation") || isWebcitationUrl(request.url) || isWebcitationUrl(finalUrl)) {
      filtered = filterLibraryArchiveCandidates(filtered, "webcitation");
    }
    if (hasPlatformHint(request.platformHint, "memento") || isMementoUrl(request.url) || isMementoUrl(finalUrl)) {
      filtered = filterLibraryArchiveCandidates(filtered, "memento");
    }
    if (hasPlatformHint(request.platformHint, "wikimedia") || isWikimediaUrl(request.url) || isWikimediaUrl(finalUrl)) {
      filtered = filterLibraryArchiveCandidates(filtered, "wikimedia");
    }
    if (hasPlatformHint(request.platformHint, "twitch") || isTwitchUrl(request.url) || isTwitchUrl(finalUrl)) {
      filtered = filterVideoSocialCandidates(filtered, "twitch");
    }
    if (hasPlatformHint(request.platformHint, "twitter") || isTwitterUrl(request.url) || isTwitterUrl(finalUrl)) {
      filtered = filterVideoSocialCandidates(filtered, "twitter");
    }
    if (hasPlatformHint(request.platformHint, "reddit") || isRedditUrl(request.url) || isRedditUrl(finalUrl)) {
      filtered = filterVideoSocialCandidates(filtered, "reddit");
    }
    if (hasPlatformHint(request.platformHint, "instagram") || isInstagramUrl(request.url) || isInstagramUrl(finalUrl)) {
      filtered = filterVideoSocialCandidates(filtered, "instagram");
    }
    if (hasPlatformHint(request.platformHint, "facebook") || isFacebookUrl(request.url) || isFacebookUrl(finalUrl)) {
      filtered = filterVideoSocialCandidates(filtered, "facebook");
    }
    if (hasPlatformHint(request.platformHint, "pinterest") || isPinterestUrl(request.url) || isPinterestUrl(finalUrl)) {
      filtered = filterImagePlatformCandidates(filtered, "pinterest");
    }
    if (hasPlatformHint(request.platformHint, "imgur") || isImgurUrl(request.url) || isImgurUrl(finalUrl)) {
      filtered = filterImagePlatformCandidates(filtered, "imgur");
    }
    if (hasPlatformHint(request.platformHint, "flickr") || isFlickrUrl(request.url) || isFlickrUrl(finalUrl)) {
      filtered = filterImagePlatformCandidates(filtered, "flickr");
    }
    if (hasPlatformHint(request.platformHint, "bandcamp") || isBandcampUrl(request.url) || isBandcampUrl(finalUrl)) {
      filtered = filterAudioPlatformCandidates(filtered, "bandcamp");
    }
    if (hasPlatformHint(request.platformHint, "mixcloud") || isMixcloudUrl(request.url) || isMixcloudUrl(finalUrl)) {
      filtered = filterAudioPlatformCandidates(filtered, "mixcloud");
    }
    if (hasPlatformHint(request.platformHint, "niconico") || isNiconicoUrl(request.url) || isNiconicoUrl(finalUrl)) {
      filtered = filterVideoSocialCandidates(filtered, "niconico");
    }
    if (hasPlatformHint(request.platformHint, "fc2") || isFc2Url(request.url) || isFc2Url(finalUrl)) {
      filtered = filterVideoSocialCandidates(filtered, "fc2");
    }
    if (hasPlatformHint(request.platformHint, "spotify") || isSpotifyUrl(request.url) || isSpotifyUrl(finalUrl)) {
      filtered = [];
      warnings.push("spotify media extraction skipped: Spotify links are treated as metadata/page targets only");
    }

    if (filtered.length === 0 && isKuaishouUrl(request.url)) {
      const fallbackUrl = kuaishouMobileFallbackUrl(request.url, finalUrl);
      if (fallbackUrl && !pageUrls.has(normalizeUrlForCompare(fallbackUrl))) {
        warnings.push("kuaishou mobile fallback attempted");
        const fallbackPage = await context.newPage();
        try {
          await fallbackPage.goto(fallbackUrl, {
            waitUntil: "domcontentloaded",
            timeout: Math.min(timeoutMs, 45_000),
          });
          await fallbackPage.waitForLoadState("networkidle", { timeout: 12_000 }).catch(() => {
            warnings.push("kuaishou mobile networkidle timeout");
          });
          await triggerPlayback(fallbackPage, warnings);
          await fallbackPage.mouse.wheel(0, 650).catch(() => undefined);
          await fallbackPage.waitForTimeout(2_000);
          await addDomCandidates(fallbackPage, fallbackPage.url(), warnings, addCandidate);
        } catch (error) {
          warnings.push(`kuaishou mobile fallback failed: ${error instanceof Error ? error.message : String(error)}`);
        } finally {
          await fallbackPage.close().catch(() => undefined);
        }
        filtered = [...candidates.values()].filter(
          (candidate) => !pageUrls.has(normalizeUrlForCompare(candidate.url)),
        );
        filtered = filterKuaishouCandidates(filtered);
      }
    }

    return {
      finalUrl,
      title,
      platformHint: request.platformHint,
      candidates: filtered.sort((a, b) => b.score - a.score),
      pageSnapshot: capturePageSnapshot ? pageSnapshot : undefined,
      requiresInteraction: pageSnapshot.requiresInteraction,
      interactionReason: pageSnapshot.interactionReason,
      warnings,
      eventCount,
      timedOut,
      userAgent,
      playbackTriggered,
      consoleErrors,
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

  async cookiesForUrl(profileId: string, url: string): Promise<CookiesForUrlResponse> {
    const normalizedProfileId = sanitizeProfileId(profileId || this.config.defaultProfileId);
    const context = await this.context(normalizedProfileId, this.config.headed);
    new URL(url);
    return { cookies: await context.cookies(url) };
  }

  async startLoginSession(profileId: string, url: string): Promise<LoginSessionSnapshot> {
    const normalizedProfileId = sanitizeProfileId(profileId || this.config.defaultProfileId);
    const loginUrl = normalizeLoginUrl(url);
    await assertSafeBrowserUrl(loginUrl);
    const context = await this.context(normalizedProfileId, this.config.headed);
    const page = await context.newPage();
    await page.setViewportSize(this.config.loginViewport);
    const now = Date.now();
    const entry: LoginSessionEntry = {
      id: crypto.randomUUID(),
      profileId: normalizedProfileId,
      page,
      createdAt: now,
      lastActiveAt: now,
      expiresAt: now + 30 * 60_000,
    };
    this.loginSessions.set(entry.id, entry);
    await page
      .goto(loginUrl, { waitUntil: "domcontentloaded", timeout: 45_000 })
      .catch(() => undefined);
    await page.waitForTimeout(800).catch(() => undefined);
    return this.snapshotLoginSession(entry.id);
  }

  async snapshotLoginSession(sessionId: string): Promise<LoginSessionSnapshot> {
    const entry = this.requireLoginSession(sessionId);
    entry.lastActiveAt = Date.now();
    const viewport = entry.page.viewportSize() ?? { width: 1280, height: 720 };
    const [title, image] = await Promise.all([
      entry.page.title().catch(() => undefined),
      entry.page.screenshot({ type: "jpeg", quality: 78, fullPage: false }),
    ]);
    return {
      session: this.loginSessionView(entry, title),
      image: `data:image/jpeg;base64,${image.toString("base64")}`,
      url: entry.page.url(),
      title,
      width: viewport.width,
      height: viewport.height,
    };
  }

  async loginClick(
    sessionId: string,
    x: number,
    y: number,
    button: "left" | "right" | "middle" = "left",
    clickCount = 1,
  ): Promise<LoginSessionSnapshot> {
    const entry = this.requireLoginSession(sessionId);
    entry.lastActiveAt = Date.now();
    await entry.page.mouse.click(x, y, {
      button,
      clickCount: clamp(Math.trunc(clickCount), 1, 3),
    });
    await entry.page.waitForTimeout(700).catch(() => undefined);
    return this.snapshotLoginSession(sessionId);
  }

  async loginMove(sessionId: string, x: number, y: number): Promise<LoginSessionSnapshot> {
    const entry = this.requireLoginSession(sessionId);
    entry.lastActiveAt = Date.now();
    const viewport = entry.page.viewportSize() ?? { width: 1280, height: 720 };
    await entry.page.mouse.move(
      clamp(x, 0, viewport.width),
      clamp(y, 0, viewport.height),
      { steps: 8 },
    );
    await entry.page.waitForTimeout(120).catch(() => undefined);
    return this.snapshotLoginSession(sessionId);
  }

  async loginMouseDown(
    sessionId: string,
    x: number,
    y: number,
    button: "left" | "right" | "middle" = "left",
  ): Promise<LoginSessionSnapshot> {
    const entry = this.requireLoginSession(sessionId);
    entry.lastActiveAt = Date.now();
    const viewport = entry.page.viewportSize() ?? { width: 1280, height: 720 };
    await entry.page.mouse.move(
      clamp(x, 0, viewport.width),
      clamp(y, 0, viewport.height),
      { steps: 6 },
    );
    await entry.page.mouse.down({ button });
    await entry.page.waitForTimeout(180).catch(() => undefined);
    return this.snapshotLoginSession(sessionId);
  }

  async loginMouseUp(
    sessionId: string,
    x: number,
    y: number,
    button: "left" | "right" | "middle" = "left",
  ): Promise<LoginSessionSnapshot> {
    const entry = this.requireLoginSession(sessionId);
    entry.lastActiveAt = Date.now();
    const viewport = entry.page.viewportSize() ?? { width: 1280, height: 720 };
    await entry.page.mouse.move(
      clamp(x, 0, viewport.width),
      clamp(y, 0, viewport.height),
      { steps: 6 },
    );
    await entry.page.mouse.up({ button });
    await entry.page.waitForTimeout(450).catch(() => undefined);
    return this.snapshotLoginSession(sessionId);
  }

  async loginType(sessionId: string, text: string): Promise<LoginSessionSnapshot> {
    const entry = this.requireLoginSession(sessionId);
    entry.lastActiveAt = Date.now();
    await entry.page.keyboard.type(text, { delay: 18 });
    await entry.page.waitForTimeout(350).catch(() => undefined);
    return this.snapshotLoginSession(sessionId);
  }

  async loginInsertText(sessionId: string, text: string): Promise<LoginSessionSnapshot> {
    const entry = this.requireLoginSession(sessionId);
    entry.lastActiveAt = Date.now();
    await entry.page.keyboard.insertText(text);
    await entry.page.waitForTimeout(350).catch(() => undefined);
    return this.snapshotLoginSession(sessionId);
  }

  async loginPress(sessionId: string, key: string): Promise<LoginSessionSnapshot> {
    const entry = this.requireLoginSession(sessionId);
    entry.lastActiveAt = Date.now();
    await entry.page.keyboard.press(key);
    await entry.page.waitForTimeout(500).catch(() => undefined);
    return this.snapshotLoginSession(sessionId);
  }

  async loginNavigate(sessionId: string, url: string): Promise<LoginSessionSnapshot> {
    const entry = this.requireLoginSession(sessionId);
    const loginUrl = normalizeLoginUrl(url);
    await assertSafeBrowserUrl(loginUrl);
    entry.lastActiveAt = Date.now();
    await entry.page
      .goto(loginUrl, { waitUntil: "domcontentloaded", timeout: 45_000 })
      .catch(() => undefined);
    await entry.page.waitForTimeout(800).catch(() => undefined);
    return this.snapshotLoginSession(sessionId);
  }

  async loginWheel(
    sessionId: string,
    deltaX: number,
    deltaY: number,
    x?: number,
    y?: number,
  ): Promise<LoginSessionSnapshot> {
    const entry = this.requireLoginSession(sessionId);
    entry.lastActiveAt = Date.now();
    if (Number.isFinite(x) && Number.isFinite(y)) {
      await entry.page.mouse.move(x as number, y as number);
    }
    await entry.page.mouse.wheel(clamp(deltaX, -3000, 3000), clamp(deltaY, -3000, 3000));
    await entry.page.waitForTimeout(350).catch(() => undefined);
    return this.snapshotLoginSession(sessionId);
  }

  async loginResize(sessionId: string, width: number, height: number): Promise<LoginSessionSnapshot> {
    const entry = this.requireLoginSession(sessionId);
    entry.lastActiveAt = Date.now();
    await entry.page.setViewportSize({
      width: clamp(Math.trunc(width), 640, 2560),
      height: clamp(Math.trunc(height), 360, 1600),
    });
    await entry.page.waitForTimeout(250).catch(() => undefined);
    return this.snapshotLoginSession(sessionId);
  }

  async loginClose(sessionId: string): Promise<{ closed: boolean }> {
    const entry = this.loginSessions.get(sessionId);
    if (!entry) {
      return { closed: false };
    }
    this.loginSessions.delete(sessionId);
    void entry.page.close().catch(() => undefined);
    return { closed: true };
  }

  async close(): Promise<void> {
    await Promise.all([...this.loginSessions.values()].map((entry) => entry.page.close().catch(() => undefined)));
    this.loginSessions.clear();
    await Promise.all([...this.contexts.values()].map((entry) => entry.context.close()));
    this.contexts.clear();
  }

  private requireLoginSession(sessionId: string): LoginSessionEntry {
    const entry = this.loginSessions.get(sessionId);
    if (!entry) {
      throw new Error("login session not found");
    }
    if (entry.expiresAt < Date.now()) {
      this.loginSessions.delete(sessionId);
      void entry.page.close().catch(() => undefined);
      throw new Error("login session expired");
    }
    return entry;
  }

  private loginSessionView(entry: LoginSessionEntry, title?: string): LoginSessionView {
    return {
      id: entry.id,
      profileId: entry.profileId,
      url: entry.page.url(),
      title,
      createdAt: new Date(entry.createdAt).toISOString(),
      lastActiveAt: new Date(entry.lastActiveAt).toISOString(),
      expiresAt: new Date(entry.expiresAt).toISOString(),
    };
  }

  private async context(profileId: string, headed: boolean): Promise<BrowserContext> {
    const existing = this.contexts.get(profileId);
    if (existing) {
      existing.lastUsedAt = Date.now();
      return existing.context;
    }

    const profileDir = path.join(this.config.profileRoot, profileId);
    await fs.mkdir(profileDir, { recursive: true });
    await removeStaleChromiumSingletonLocks(profileDir);
    const context = await chromium.launchPersistentContext(profileDir, {
      channel: this.config.browserChannel,
      headless: !headed,
      viewport: this.config.viewport,
      locale: this.config.locale,
      timezoneId: this.config.timezoneId,
      userAgent: this.config.userAgent,
    });
    await context.route("**/*", async (route) => {
      const url = route.request().url();
      try {
        await assertSafeBrowserUrl(url, this.blockedHosts);
        await route.continue();
      } catch {
        await route.abort("blockedbyclient").catch(() => undefined);
      }
    });
    // Make the headless context behave like the user's real browser by
    // normalizing the most obvious automation tells. This is not aimed at any
    // specific anti-bot product; it just keeps sites that break under plain
    // headless from misrendering.
    await context.addInitScript(() => {
      Object.defineProperty(navigator, "webdriver", { get: () => undefined });
      Object.defineProperty(navigator, "languages", { get: () => ["zh-CN", "zh", "en"] });
      try {
        const win = window as unknown as { chrome?: unknown };
        if (!win.chrome) {
          win.chrome = { runtime: {} };
        }
      } catch {
        // ignore
      }
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
    initiatorUrl: safeRequestFrameUrl(request),
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
  const height = qualityHeight(url);
  if (height) score += Math.min(Math.floor(height / 12), 140);
  if (/(\.ts|segment|chunk|frag)/i.test(path)) score -= 25;
  if (isLikelyAdOrTrackingUrl(url)) score -= 500;
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

function qualityHeight(url: string): number | undefined {
  const match = url.match(/(?:^|[^\d])([1-9]\d{2,3})p(?:[^\d]|$)/i);
  if (!match) {
    return undefined;
  }
  const height = Number(match[1]);
  return Number.isFinite(height) ? height : undefined;
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
  const availability = bilibiliAvailability(root);

  for (const entry of asArray(dash?.video)) {
    const candidate = candidateFromBilibiliDashEntry(entry, "video", pageUrl, availability);
    if (candidate) {
      candidates.push(candidate);
    }
  }

  for (const entry of asArray(dash?.audio)) {
    const candidate = candidateFromBilibiliDashEntry(entry, "audio", pageUrl, availability);
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

interface BilibiliAvailability {
  acceptQuality: number[];
  acceptDescription: string[];
  highestAdvertisedHeight?: number;
}

function bilibiliAvailability(root: Record<string, unknown> | undefined): BilibiliAvailability | undefined {
  if (!root) {
    return undefined;
  }
  const acceptQuality = asArray(root.accept_quality)
    .map((value) => asNumber(value))
    .filter((value): value is number => Boolean(value));
  const acceptDescription = asArray(root.accept_description)
    .map((value) => firstString(value))
    .filter((value): value is string => Boolean(value));
  const highestAdvertisedHeight = Math.max(
    0,
    ...acceptQuality.map(bilibiliQualityToHeight),
    ...acceptDescription.map((value) => qualityHeight(value) ?? 0),
  );
  if (!acceptQuality.length && !acceptDescription.length) {
    return undefined;
  }
  return {
    acceptQuality,
    acceptDescription,
    highestAdvertisedHeight: highestAdvertisedHeight || undefined,
  };
}

function bilibiliQualityToHeight(value: number): number {
  if (value >= 120) return 2160;
  if (value === 116) return 1080;
  if (value === 112) return 1080;
  if (value === 80) return 1080;
  if (value === 74) return 720;
  if (value === 64) return 720;
  if (value === 32) return 480;
  if (value === 16) return 360;
  return 0;
}

function candidateFromBilibiliDashEntry(
  entry: unknown,
  kind: "audio" | "video",
  pageUrl: string,
  availability?: BilibiliAvailability,
): BrowserCandidate | undefined {
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
  const higherQualityRequiresProfile = kind === "video"
    && Boolean(height)
    && Boolean(availability?.highestAdvertisedHeight)
    && (availability?.highestAdvertisedHeight ?? 0) > (height ?? 0);
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
    requiresAuthorization: higherQualityRequiresProfile,
    metadata: {
      source: "bilibili_playinfo",
      mediaId,
      height,
      width,
      bandwidth,
      codecs,
      acceptQuality: availability?.acceptQuality,
      acceptDescription: availability?.acceptDescription,
      highestAdvertisedHeight: availability?.highestAdvertisedHeight,
      higherQualityRequiresProfile,
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

async function addDomCandidates(
  page: Page,
  pageUrl: string,
  warnings: string[],
  addCandidate: (candidate: BrowserCandidate) => void,
): Promise<void> {
  const genericCandidates = await genericCandidatesFromPage(page, pageUrl).catch((error) => {
    warnings.push(`generic discovery failed: ${error instanceof Error ? error.message : String(error)}`);
    return [];
  });
  for (const candidate of genericCandidates) {
    addCandidate(candidate);
  }
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
    const platformMediaKeys = /(?:play|media|video|audio|stream|dash|hls|m3u8|mp4|m4s|source|src|url|uri|backup|main|origin|quality|download|file|manifest|playlist|data|bitrate|height|width)/i;
    const decodeMaybeEncodedUrl = (value: string): string[] => {
      const out = new Set<string>([value]);
      for (const item of [...out]) {
        try {
          out.add(decodeURIComponent(item));
        } catch {
          // keep raw value
        }
        try {
          if (/^[A-Za-z0-9+/=_-]{24,}$/.test(item) && item.length < 4096) {
            const decoded = atob(item.replace(/-/g, "+").replace(/_/g, "/"));
            if (/https?:\/\//i.test(decoded) || /\.(?:m3u8|mpd|mp4|m4s|m4a|mp3)(?:[?#]|$)/i.test(decoded)) {
              out.add(decoded);
            }
          }
        } catch {
          // not base64
        }
      }
      return [...out].map((item) => item
        .replace(/\\u002[fF]/g, "/")
        .replace(/\\u0026/g, "&")
        .replace(/\\u003[dD]/g, "=")
        .replace(/\\\//g, "/"));
    };
    const firstText = (record: Record<string, unknown>, keys: string[]): string | undefined => {
      for (const key of keys) {
        const value = record[key];
        if (typeof value === "string" && value.trim()) {
          return value.trim();
        }
      }
      return undefined;
    };
    const addPlatformUrlRecord = (
      record: Record<string, unknown>,
      source: string,
      addUrl: (value: string | null | undefined, source: string, options?: Omit<BrowserDiscoveredUrl, "url" | "source">) => void,
    ) => {
      const directKeys = [
        "url", "uri", "src", "source", "playUrl", "play_url", "downloadUrl", "download_url",
        "mainUrl", "main_url", "backupUrl", "backup_url", "baseUrl", "base_url", "hls", "dash",
        "manifest", "playlist", "videoUrl", "video_url", "audioUrl", "audio_url", "file",
      ];
      for (const key of directKeys) {
        const value = record[key];
        if (typeof value === "string") {
          const scoreBoost = platformMediaKeys.test(key) ? 28 : 20;
          for (const decoded of decodeMaybeEncodedUrl(value)) {
            addUrl(decoded, `${source}_${key}`, { scoreBoost });
            for (const match of decoded.matchAll(urlPattern)) {
              addUrl(match[0], `${source}_${key}_embedded`, { scoreBoost: scoreBoost + 2 });
            }
          }
        }
        if (Array.isArray(value)) {
          for (const item of value.slice(0, 50)) {
            if (typeof item === "string") {
              addUrl(item, `${source}_${key}_list`, { scoreBoost: 24 });
            }
          }
        }
      }

      const host = firstText(record, ["host", "domain", "cdn", "cdnHost", "cdn_host"]);
      const path = firstText(record, ["path", "pathname", "file", "stream", "streamPath", "stream_path"]);
      if (host && path) {
        const normalizedHost = /^https?:\/\//i.test(host) ? host : `https://${host}`;
        try {
          addUrl(new URL(path, normalizedHost).href, `${source}_host_path`, { scoreBoost: 26 });
        } catch {
          // ignore malformed split URL records
        }
      }
    };
    const scanJsonValue = (value: unknown, source: string, depth = 0) => {
      if (depth > 8 || value === null || value === undefined) {
        return;
      }
      if (typeof value === "string") {
        for (const decoded of decodeMaybeEncodedUrl(value)) {
          add(decoded, source, { scoreBoost: 22 });
          for (const match of decoded.matchAll(urlPattern)) {
            add(match[0], source, { scoreBoost: 24 });
          }
        }
        return;
      }
      if (Array.isArray(value)) {
        for (const item of value.slice(0, 500)) {
          scanJsonValue(item, source, depth + 1);
        }
        return;
      }
      if (typeof value === "object") {
        const record = value as Record<string, unknown>;
        addPlatformUrlRecord(record, source, add);
        for (const [key, item] of Object.entries(record).slice(0, 500)) {
          const keySource = platformMediaKeys.test(key) ? `${source}_${key.replace(/[^a-zA-Z0-9_-]/g, "_")}` : source;
          scanJsonValue(item, keySource, depth + 1);
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

async function pageResourceFromResponse(
  response: Response,
  includeBody: boolean,
  maxBodyBytes: number,
  remainingBodyBudget: number,
): Promise<PageResource | undefined> {
  const request = response.request();
  const method = request.method();
  if (method !== "GET" && method !== "HEAD") {
    return undefined;
  }
  const url = response.url();
  if (!isHttpUrl(url)) {
    return undefined;
  }
  const headers = response.headers();
  const contentType = headers["content-type"];
  const contentLength = parseContentLength(headers["content-length"]);
  const resourceType = request.resourceType();
  const bodyBase64 = includeBody
    ? await safeArchiveResponseBodyBase64(
        response,
        resourceType,
        contentType,
        contentLength,
        maxBodyBytes,
        remainingBodyBudget,
      )
    : undefined;
  return {
    url,
    method,
    status: response.status(),
    contentType,
    contentLength,
    resourceType,
    initiatorUrl: safeRequestFrameUrl(request),
    requestHeaders: safeArchiveRequestHeaders(request),
    bodyBase64,
    bodySource: bodyBase64 ? "network" : undefined,
    source: "network",
  };
}

async function safeArchiveResponseBodyBase64(
  response: Response,
  resourceType: string,
  contentType: string | undefined,
  contentLength: number | undefined,
  maxBodyBytes: number,
  remainingBudget: number,
): Promise<string | undefined> {
  if (
    remainingBudget <= 0 ||
    !archiveResponseBodyAllowed(response, resourceType, contentType, contentLength, maxBodyBytes, remainingBudget)
  ) {
    return undefined;
  }
  const buffer = await withTimeout(response.body(), 2_000).catch(() => undefined);
  if (!buffer || buffer.length > maxBodyBytes || buffer.length > remainingBudget) {
    return undefined;
  }
  return buffer.toString("base64");
}

function archiveResponseBodyAllowed(
  response: Response,
  resourceType: string,
  contentType: string | undefined,
  contentLength: number | undefined,
  maxBodyBytes: number,
  remainingBudget: number,
): boolean {
  if (!response.ok()) {
    return false;
  }
  if (contentLength !== undefined && (contentLength > maxBodyBytes || contentLength > remainingBudget)) {
    return false;
  }
  const loweredType = (contentType ?? "").toLowerCase();
  const maybeLargeBinary =
    loweredType.startsWith("image/") ||
    loweredType.startsWith("font/") ||
    loweredType === "application/font-woff" ||
    loweredType === "application/wasm";
  if (contentLength === undefined && maybeLargeBinary) {
    return false;
  }
  if (
    loweredType.startsWith("text/css") ||
    loweredType.includes("javascript") ||
    loweredType.startsWith("image/") ||
    loweredType.startsWith("font/") ||
    loweredType === "application/font-woff" ||
    loweredType === "application/manifest+json" ||
    loweredType === "application/wasm" ||
    loweredType === "image/svg+xml"
  ) {
    return true;
  }
  return ["stylesheet", "script", "image", "font", "manifest"].includes(resourceType);
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`timeout after ${timeoutMs}ms`)), timeoutMs);
  });
  return Promise.race([promise, timeout]).finally(() => {
    if (timer) {
      clearTimeout(timer);
    }
  });
}

function safeArchiveRequestHeaders(request: Request): Record<string, string> | undefined {
  const headers = request.headers();
  const out: Record<string, string> = {};
  for (const [name, value] of Object.entries(headers)) {
    const lowered = name.toLowerCase();
    if (
      lowered === "user-agent" ||
      lowered === "accept" ||
      lowered === "accept-language" ||
      lowered === "referer" ||
      lowered === "origin" ||
      lowered === "sec-fetch-dest" ||
      lowered === "sec-fetch-mode" ||
      lowered === "sec-fetch-site" ||
      lowered === "sec-ch-ua" ||
      lowered === "sec-ch-ua-mobile" ||
      lowered === "sec-ch-ua-platform"
    ) {
      out[lowered] = value;
    }
  }
  return Object.keys(out).length ? out : undefined;
}

function safeRequestFrameUrl(request: Request): string | undefined {
  try {
    return request.frame().url();
  } catch {
    return undefined;
  }
}

function addPageResource(resources: Map<string, PageResource>, resource: PageResource | undefined) {
  if (!resource || !isHttpUrl(resource.url)) {
    return;
  }
  const existing = resources.get(resource.url);
  if (!existing) {
    resources.set(resource.url, resource);
    return;
  }
  resources.set(resource.url, {
    ...existing,
    ...Object.fromEntries(
      Object.entries(resource).filter(([, value]) => value !== undefined && value !== ""),
    ),
    bodyBase64: existing.bodyBase64 ?? resource.bodyBase64,
    bodySource: existing.bodySource ?? resource.bodySource,
    source: existing.source === resource.source ? existing.source : `${existing.source},${resource.source}`,
  });
}

async function enrichPageResourcesFromBrowser(
  page: Page,
  resources: Map<string, PageResource>,
  warnings: string[],
  maxBodyBytes: number,
  totalBodyBytes: number,
): Promise<void> {
  const candidates = prioritizeBrowserFetchResources(
    [...resources.values()].filter((resource) => (
      !resource.bodyBase64 &&
      resource.source.toLowerCase().includes("network") &&
      browserFetchResourceAllowed(resource, maxBodyBytes)
    )),
  ).slice(0, 24);
  if (!candidates.length) {
    return;
  }

  let cachedBytes = [...resources.values()].reduce((total, resource) => {
    return total + (resource.bodyBase64 ? Buffer.byteLength(resource.bodyBase64, "base64") : 0);
  }, 0);
  for (const batch of chunk(candidates, 4)) {
    if (cachedBytes >= totalBodyBytes) {
      break;
    }
    const remainingBudget = totalBodyBytes - cachedBytes;
    const fetched = await withTimeout(
      page.evaluate(
        async ({ items, maxBytes, totalBudget }) => {
          type FetchResult = {
            url: string;
            status?: number;
            contentType?: string;
            contentLength?: number;
            bodyBase64?: string;
          };
          const out: FetchResult[] = [];
          let used = 0;
          for (const item of items) {
            if (used >= totalBudget) {
              break;
            }
            try {
              const response = await fetch(item.url, {
                credentials: "include",
                cache: "force-cache",
                mode: "cors",
              });
              const contentType = response.headers.get("content-type") ?? undefined;
              const contentLength = Number(response.headers.get("content-length") ?? "");
              if (!response.ok || !browserFetchContentAllowed(contentType, item.resourceType)) {
                out.push({ url: item.url, status: response.status, contentType });
                continue;
              }
              const buffer = await response.arrayBuffer();
              if (buffer.byteLength > maxBytes || used + buffer.byteLength > totalBudget) {
                out.push({
                  url: item.url,
                  status: response.status,
                  contentType,
                  contentLength: Number.isFinite(contentLength) ? contentLength : buffer.byteLength,
                });
                continue;
              }
              used += buffer.byteLength;
              out.push({
                url: item.url,
                status: response.status,
                contentType,
                contentLength: Number.isFinite(contentLength) ? contentLength : buffer.byteLength,
                bodyBase64: arrayBufferToBase64(buffer),
              });
            } catch {
              out.push({ url: item.url });
            }
          }
          return out;

          function browserFetchContentAllowed(contentType: string | undefined, resourceType: string | undefined): boolean {
            const loweredType = (contentType ?? "").toLowerCase();
            const loweredResource = (resourceType ?? "").toLowerCase();
            return loweredType.startsWith("text/css") ||
              loweredType.includes("javascript") ||
              loweredType.startsWith("image/") ||
              loweredType.startsWith("font/") ||
              loweredType === "application/font-woff" ||
              loweredType === "application/manifest+json" ||
              loweredType === "application/wasm" ||
              loweredType === "image/svg+xml" ||
              ["stylesheet", "script", "image", "font", "manifest"].includes(loweredResource);
          }

          function arrayBufferToBase64(buffer: ArrayBuffer): string {
            let binary = "";
            const bytes = new Uint8Array(buffer);
            const chunkSize = 0x8000;
            for (let offset = 0; offset < bytes.length; offset += chunkSize) {
              const chunk = bytes.subarray(offset, offset + chunkSize);
              binary += String.fromCharCode(...chunk);
            }
            return btoa(binary);
          }
        },
        {
          items: batch.map((resource) => ({
            url: resource.url,
            resourceType: resource.resourceType,
          })),
          maxBytes: maxBodyBytes,
          totalBudget: remainingBudget,
        },
      ),
      2_500,
    ).catch((error) => {
      warnings.push(`browser resource fetch failed: ${error instanceof Error ? error.message : String(error)}`);
      return [];
    });

    for (const item of fetched) {
      const resource = resources.get(item.url);
      if (!resource) {
        continue;
      }
      if (item.bodyBase64) {
        const bytes = Buffer.byteLength(item.bodyBase64, "base64");
        if (cachedBytes + bytes > totalBodyBytes) {
          continue;
        }
        cachedBytes += bytes;
      }
      addPageResource(resources, {
        ...resource,
        status: item.status ?? resource.status,
        contentType: item.contentType ?? resource.contentType,
        contentLength: item.contentLength ?? resource.contentLength,
        bodyBase64: item.bodyBase64 ?? resource.bodyBase64,
        bodySource: item.bodyBase64 ? "browser_fetch" : resource.bodySource,
        source: "browser_fetch",
      });
    }
  }
}

function prioritizeBrowserFetchResources(resources: PageResource[]): PageResource[] {
  return resources.sort((left, right) => browserFetchPriority(right) - browserFetchPriority(left));
}

function browserFetchPriority(resource: PageResource): number {
  const source = resource.source.toLowerCase();
  const type = (resource.contentType ?? "").toLowerCase();
  const resourceType = (resource.resourceType ?? "").toLowerCase();
  const path = urlPath(resource.url).toLowerCase();
  let score = source.includes("network") ? 100 : 0;
  if (resource.bodyBase64) score -= 1_000;
  if (type.startsWith("text/css") || resourceType === "stylesheet" || path.endsWith(".css")) score += 70;
  if (type.includes("javascript") || resourceType === "script" || /\.(js|mjs)(?:$|[?#])/i.test(path)) score += 60;
  if (type.startsWith("image/") || resourceType === "image" || /\.(png|jpe?g|webp|gif|avif|svg|ico)(?:$|[?#])/i.test(path)) score += 45;
  if (type.startsWith("font/") || resourceType === "font" || /\.(woff2?|ttf|otf|eot)(?:$|[?#])/i.test(path)) score += 30;
  const size = resource.contentLength ?? 0;
  if (size > 0 && size <= 128 * 1024) score += 20;
  if (size > 512 * 1024) score -= 50;
  if (source.includes("dns-prefetch")) score -= 200;
  return score;
}

function browserFetchResourceAllowed(resource: PageResource, maxBodyBytes: number): boolean {
  if (!isHttpUrl(resource.url)) {
    return false;
  }
  const contentLength = resource.contentLength;
  if (contentLength !== undefined && contentLength > maxBodyBytes) {
    return false;
  }
  const loweredType = (resource.contentType ?? "").toLowerCase();
  const loweredResource = (resource.resourceType ?? "").toLowerCase();
  const path = urlPath(resource.url).toLowerCase();
  return loweredType.startsWith("text/css") ||
    loweredType.includes("javascript") ||
    loweredType.startsWith("image/") ||
    loweredType.startsWith("font/") ||
    loweredType === "application/font-woff" ||
    loweredType === "application/manifest+json" ||
    loweredType === "application/wasm" ||
    loweredType === "image/svg+xml" ||
    ["stylesheet", "script", "image", "font", "manifest"].includes(loweredResource) ||
    /\.(css|js|mjs|png|jpe?g|webp|gif|avif|svg|ico|woff2?|ttf|otf|eot|webmanifest)(?:$|[?#])/i.test(path);
}

async function startCdpCapture(
  page: Page,
  bodyMaxBytes: number,
  bodyTotalBytes: number,
): Promise<CdpCapture> {
  const session = await page.context().newCDPSession(page);
  const capture: CdpCapture = {
    session,
    resources: new Map(),
    pendingBodies: new Set(),
    warnings: [],
    bodyBytes: 0,
    bodyMaxBytes,
    bodyTotalBytes,
  };
  await session.send("Network.enable", {
    maxTotalBufferSize: Math.min(bodyTotalBytes * 2, 256 * 1024 * 1024),
    maxResourceBufferSize: Math.min(bodyMaxBytes * 2, 64 * 1024 * 1024),
  });

  session.on("Network.requestWillBeSent", (event: any) => {
    if (!isHttpUrl(event.request?.url)) return;
    const requestId = String(event.requestId);
    const existing = capture.resources.get(requestId);
    const redirectChain = existing?.redirectChain ?? [];
    if (event.redirectResponse?.url) {
      redirectChain.push(event.redirectResponse.url);
    }
    capture.resources.set(requestId, {
      ...existing,
      requestId,
      url: event.request.url,
      method: event.request.method,
      requestHeaders: safeCdpHeaders(event.request.headers),
      initiatorType: event.initiator?.type,
      initiatorUrl: cdpInitiatorUrl(event.initiator),
      frameUrl: event.documentURL,
      redirectChain,
      startedAt: event.timestamp,
    });
  });

  session.on("Network.responseReceived", (event: any) => {
    const requestId = String(event.requestId);
    const existing = capture.resources.get(requestId);
    if (!existing || !isHttpUrl(existing.url)) return;
    const response = event.response ?? {};
    capture.resources.set(requestId, {
      ...existing,
      status: response.status,
      contentType: headerValue(response.headers, "content-type"),
      contentLength: parseContentLength(headerValue(response.headers, "content-length")),
      resourceType: String(event.type ?? "").toLowerCase(),
      responseHeaders: safeCdpHeaders(response.headers),
      servedFromCache: Boolean(response.fromDiskCache || response.fromPrefetchCache),
      fromServiceWorker: Boolean(response.fromServiceWorker),
    });
  });

  session.on("Network.loadingFinished", (event: any) => {
    const requestId = String(event.requestId);
    const resource = capture.resources.get(requestId);
    if (!resource || !cdpBodyAllowed(resource, capture)) return;
    const task = (async () => {
      try {
        const body = await session.send("Network.getResponseBody", { requestId });
        const raw = String((body as { body?: string }).body ?? "");
        const encoded = Boolean((body as { base64Encoded?: boolean }).base64Encoded);
        const bytes = encoded ? Buffer.byteLength(raw, "base64") : Buffer.byteLength(raw, "utf8");
        if (bytes > capture.bodyMaxBytes || capture.bodyBytes + bytes > capture.bodyTotalBytes) {
          return;
        }
        capture.bodyBytes += bytes;
        resource.bodyBase64 = encoded ? raw : Buffer.from(raw, "utf8").toString("base64");
        resource.bodySource = "cdp";
        resource.endedAt = event.timestamp;
      } catch (error) {
        if (capture.warnings.length < 25) {
          capture.warnings.push(`cdp getResponseBody failed: ${error instanceof Error ? error.message : String(error)}`);
        }
      }
    })();
    capture.pendingBodies.add(task);
    task.finally(() => capture.pendingBodies.delete(task)).catch(() => undefined);
  });

  session.on("Network.loadingFailed", (event: any) => {
    const requestId = String(event.requestId);
    const existing = capture.resources.get(requestId);
    if (!existing) return;
    capture.resources.set(requestId, {
      ...existing,
      endedAt: event.timestamp,
    });
  });

  return capture;
}

function cdpResourcesToPageResources(capture: CdpCapture): PageResource[] {
  return [...capture.resources.values()]
    .filter((resource) => isHttpUrl(resource.url))
    .map((resource) => ({
      url: resource.url,
      method: resource.method,
      status: resource.status,
      contentType: resource.contentType,
      contentLength: resource.contentLength,
      resourceType: resource.resourceType,
      initiatorUrl: resource.initiatorUrl,
      initiatorType: resource.initiatorType,
      frameUrl: resource.frameUrl,
      redirectChain: resource.redirectChain,
      servedFromCache: resource.servedFromCache,
      fromServiceWorker: resource.fromServiceWorker,
      requestHeaders: resource.requestHeaders,
      bodyBase64: resource.bodyBase64,
      bodySource: resource.bodySource,
      source: "cdp",
    }));
}

async function captureMhtmlSnapshot(page: Page, warnings: string[]): Promise<string | undefined> {
  try {
    const session = await page.context().newCDPSession(page);
    try {
      const result = await session.send("Page.captureSnapshot", { format: "mhtml" });
      const data = String((result as { data?: string }).data ?? "");
      return data ? Buffer.from(data, "utf8").toString("base64") : undefined;
    } finally {
      await session.detach().catch(() => undefined);
    }
  } catch (error) {
    warnings.push(`mhtml capture failed: ${error instanceof Error ? error.message : String(error)}`);
    return undefined;
  }
}

function buildHarFromCdp(
  capture: CdpCapture,
  pageUrl: string,
  title: string | undefined,
): Record<string, unknown> {
  const entries = [...capture.resources.values()]
    .filter((resource) => isHttpUrl(resource.url))
    .map((resource) => {
      const bodySize = resource.bodyBase64 ? Buffer.byteLength(resource.bodyBase64, "base64") : -1;
      return {
        startedDateTime: new Date().toISOString(),
        time: resource.startedAt && resource.endedAt
          ? Math.max(0, (resource.endedAt - resource.startedAt) * 1000)
          : 0,
        request: {
          method: resource.method ?? "GET",
          url: resource.url,
          httpVersion: "HTTP/1.1",
          headers: objectHeadersToHar(resource.requestHeaders),
          queryString: urlQueryToHar(resource.url),
          cookies: [],
          headersSize: -1,
          bodySize: 0,
        },
        response: {
          status: resource.status ?? 0,
          statusText: "",
          httpVersion: "HTTP/1.1",
          headers: objectHeadersToHar(resource.responseHeaders),
          cookies: [],
          content: {
            size: Math.max(bodySize, 0),
            mimeType: resource.contentType ?? "application/octet-stream",
            text: resource.bodyBase64,
            encoding: resource.bodyBase64 ? "base64" : undefined,
          },
          redirectURL: "",
          headersSize: -1,
          bodySize,
        },
        cache: {},
        timings: { send: 0, wait: 0, receive: 0 },
        _resourceType: resource.resourceType,
        _initiatorType: resource.initiatorType,
        _initiatorUrl: resource.initiatorUrl,
        _frameUrl: resource.frameUrl,
        _redirectChain: resource.redirectChain,
        _bodySource: resource.bodySource,
      };
    });
  return {
    log: {
      version: "1.2",
      creator: { name: "Reflection King", version: "0.1.0" },
      pages: [{ id: "page_0", startedDateTime: new Date().toISOString(), title: title ?? pageUrl, pageTimings: {} }],
      entries,
    },
  };
}

function cdpBodyAllowed(resource: CdpResourceRecord, capture: CdpCapture): boolean {
  if (!resource.status || resource.status < 200 || resource.status >= 400) return false;
  if (capture.bodyBytes >= capture.bodyTotalBytes) return false;
  if (resource.contentLength !== undefined && resource.contentLength > capture.bodyMaxBytes) return false;
  const contentType = (resource.contentType ?? "").toLowerCase();
  const resourceType = (resource.resourceType ?? "").toLowerCase();
  const path = urlPath(resource.url).toLowerCase();
  return contentType.startsWith("text/css") ||
    contentType.includes("javascript") ||
    contentType.startsWith("image/") ||
    contentType.startsWith("font/") ||
    contentType === "application/font-woff" ||
    contentType === "application/manifest+json" ||
    contentType === "application/wasm" ||
    contentType === "image/svg+xml" ||
    ["stylesheet", "script", "image", "font", "manifest"].includes(resourceType) ||
    /\.(css|js|mjs|png|jpe?g|webp|gif|avif|svg|ico|woff2?|ttf|otf|webmanifest|wasm)(?:$|[?#])/i.test(path);
}

function cdpInitiatorUrl(initiator: any): string | undefined {
  if (!initiator) return undefined;
  if (typeof initiator.url === "string") return initiator.url;
  const stack = initiator.stack?.callFrames ?? initiator.stack?.parent?.callFrames;
  if (Array.isArray(stack)) {
    const frame = stack.find((item) => typeof item.url === "string" && item.url);
    return frame?.url;
  }
  return undefined;
}

function safeCdpHeaders(headers: unknown): Record<string, string> | undefined {
  if (!headers || typeof headers !== "object") return undefined;
  const out: Record<string, string> = {};
  for (const [name, value] of Object.entries(headers as Record<string, unknown>)) {
    const lowered = name.toLowerCase();
    if (!pageArchiveHeaderAllowed(lowered)) continue;
    out[lowered] = Array.isArray(value) ? value.join(", ") : String(value);
  }
  return Object.keys(out).length ? out : undefined;
}

function pageArchiveHeaderAllowed(name: string): boolean {
  return [
    "user-agent",
    "accept",
    "accept-language",
    "referer",
    "origin",
    "content-type",
    "content-length",
    "sec-fetch-dest",
    "sec-fetch-mode",
    "sec-fetch-site",
    "sec-ch-ua",
    "sec-ch-ua-mobile",
    "sec-ch-ua-platform",
  ].includes(name);
}

function headerValue(headers: unknown, name: string): string | undefined {
  if (!headers || typeof headers !== "object") return undefined;
  const wanted = name.toLowerCase();
  for (const [key, value] of Object.entries(headers as Record<string, unknown>)) {
    if (key.toLowerCase() === wanted) {
      return Array.isArray(value) ? value.join(", ") : String(value);
    }
  }
  return undefined;
}

function objectHeadersToHar(headers: Record<string, string> | undefined): Array<{ name: string; value: string }> {
  return Object.entries(headers ?? {}).map(([name, value]) => ({ name, value }));
}

function urlQueryToHar(value: string): Array<{ name: string; value: string }> {
  try {
    return [...new URL(value).searchParams.entries()].map(([name, value]) => ({ name, value }));
  } catch {
    return [];
  }
}

function chunk<T>(items: T[], size: number): T[][] {
  const out: T[][] = [];
  for (let index = 0; index < items.length; index += size) {
    out.push(items.slice(index, index + size));
  }
  return out;
}

function parseContentLength(value: string | undefined): number | undefined {
  if (!value) {
    return undefined;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : undefined;
}

async function preparePageForArchive(page: Page, warnings: string[]): Promise<void> {
  await dismissConsentPrompt(page).catch(() => undefined);
  try {
    await page.evaluate(async () => {
      const sleep = (duration: number) => new Promise((resolve) => window.setTimeout(resolve, duration));
      const maxScrollY = Math.max(
        document.documentElement.scrollHeight,
        document.body?.scrollHeight ?? 0,
      );
      const viewportHeight = Math.max(window.innerHeight || 720, 360);
      const steps = Math.min(8, Math.max(2, Math.ceil(maxScrollY / viewportHeight)));
      for (let index = 1; index <= steps; index += 1) {
        window.scrollTo(0, Math.floor((maxScrollY * index) / steps));
        window.dispatchEvent(new Event("scroll", { bubbles: true }));
        await sleep(350);
      }
      window.scrollTo(0, 0);
      window.dispatchEvent(new Event("scroll", { bubbles: true }));
      await sleep(500);
    });
    await page.waitForLoadState("networkidle", { timeout: 8_000 }).catch(() => undefined);
  } catch (error) {
    warnings.push(`page archive warmup failed: ${error instanceof Error ? error.message : String(error)}`);
  }
}

async function genericPageResourcesFromPage(page: Page, pageUrl: string): Promise<PageResource[]> {
  return page.evaluate((baseUrl) => {
    const out = new Map<string, PageResource>();
    const add = (raw: string | null | undefined, source: string) => {
      if (!raw || raw.startsWith("blob:") || raw.startsWith("data:") || raw.startsWith("javascript:")) {
        return;
      }
      try {
        const url = new URL(raw, baseUrl).toString();
        if (!url.startsWith("http://") && !url.startsWith("https://")) {
          return;
        }
        const existing = out.get(url);
        out.set(url, {
          url,
          method: existing?.method ?? "GET",
          contentType: existing?.contentType,
          contentLength: existing?.contentLength,
          resourceType: existing?.resourceType,
          initiatorUrl: existing?.initiatorUrl ?? baseUrl,
          initiatorType: existing?.initiatorType ?? source,
          frameUrl: existing?.frameUrl ?? baseUrl,
          redirectChain: existing?.redirectChain ?? [],
          servedFromCache: existing?.servedFromCache,
          fromServiceWorker: existing?.fromServiceWorker,
          requestHeaders: existing?.requestHeaders,
          bodyBase64: existing?.bodyBase64,
          bodySource: existing?.bodySource,
          status: existing?.status,
          source: existing ? `${existing.source},${source}` : source,
        });
      } catch {
        // Ignore malformed DOM URLs.
      }
    };

    document.querySelectorAll<HTMLLinkElement>("link[href]").forEach((node) => add(node.href, `dom_link:${node.rel || "link"}`));
    document.querySelectorAll<HTMLScriptElement>("script[src]").forEach((node) => add(node.src, "dom_script"));
    document.querySelectorAll<HTMLImageElement>("img[src]").forEach((node) => add(node.currentSrc || node.src, "dom_image"));
    document.querySelectorAll<HTMLImageElement>("img[srcset]").forEach((node) => addSrcset(node.getAttribute("srcset"), "dom_image_srcset"));
    document.querySelectorAll<HTMLSourceElement>("source[src]").forEach((node) => add(node.src, "dom_source"));
    document.querySelectorAll<HTMLSourceElement>("source[srcset]").forEach((node) => addSrcset(node.getAttribute("srcset"), "dom_source_srcset"));
    document.querySelectorAll<HTMLVideoElement | HTMLAudioElement>("video[src],audio[src]").forEach((node) => add(node.currentSrc || node.src, "dom_media"));
    document.querySelectorAll<HTMLVideoElement>("video[poster]").forEach((node) => add(node.poster, "dom_video_poster"));
    document.querySelectorAll<HTMLElement>("[style]").forEach((node) => {
      for (const value of cssUrlValues(node.getAttribute("style") || "")) {
        add(value, "dom_inline_style_url");
      }
    });
    document.querySelectorAll<HTMLMetaElement>("meta[property],meta[name],meta[itemprop]").forEach((node) => {
      const key = (node.getAttribute("property") || node.getAttribute("name") || node.getAttribute("itemprop") || "").toLowerCase();
      if (/image|logo|icon|video|audio|manifest|thumbnail/.test(key)) {
        add(node.getAttribute("content"), `dom_meta:${key.replace(/[^a-z0-9_-]/g, "_")}`);
      }
    });
    document.querySelectorAll<HTMLElement>("[data-src],[data-srcset],[data-original],[data-lazy-src]").forEach((node) => {
      add(node.getAttribute("data-src"), "dom_data_src");
      add(node.getAttribute("data-original"), "dom_data_original");
      add(node.getAttribute("data-lazy-src"), "dom_data_lazy_src");
      addSrcset(node.getAttribute("data-srcset"), "dom_data_srcset");
    });

    for (const sheet of Array.from(document.styleSheets)) {
      let rules: CSSRuleList | undefined;
      try {
        rules = sheet.cssRules;
      } catch {
        continue;
      }
      for (const rule of Array.from(rules).slice(0, 2_000)) {
        const text = (rule as CSSRule).cssText || "";
        for (const value of cssUrlValues(text)) {
          add(value, "dom_cssom_url");
        }
      }
    }

    for (const entry of performance.getEntriesByType("resource") as PerformanceResourceTiming[]) {
      add(entry.name, `performance:${entry.initiatorType || "resource"}`);
      const current = out.get(entry.name);
      if (current) {
        current.contentLength = entry.transferSize || entry.encodedBodySize || current.contentLength;
        current.resourceType = current.resourceType ?? entry.initiatorType;
      }
    }

    return [...out.values()].slice(0, 2_000);

    function addSrcset(value: string | null | undefined, source: string) {
      if (!value) return;
      for (const item of value.split(",")) {
        const candidate = item.trim().split(/\s+/)[0];
        add(candidate, source);
      }
    }

    function cssUrlValues(value: string): string[] {
      const urls: string[] = [];
      const pattern = /url\(\s*(['"]?)(.*?)\1\s*\)/gi;
      let match: RegExpExecArray | null;
      while ((match = pattern.exec(value))) {
        urls.push(match[2]);
      }
      return urls;
    }
  }, pageUrl);
}

async function captureSnapshot(
  page: Page,
  finalUrl: string,
  title: string | undefined,
  resources: PageResource[],
  warnings: string[],
  options: {
    includeScreenshot?: boolean;
    mhtmlBase64?: string;
    harJson?: Record<string, unknown>;
  } = {},
): Promise<PageSnapshot> {
  const html = await page
    .evaluate(() => document.documentElement.outerHTML)
    .catch((error) => {
      warnings.push(`page html capture failed: ${error instanceof Error ? error.message : String(error)}`);
      return "";
    });
  const text = await page
    .evaluate(() => document.body?.innerText?.slice(0, 200_000) ?? "")
    .catch(() => "");
  const screenshotBuffer = options.includeScreenshot
    ? await page
        .screenshot({ type: "png", fullPage: false, timeout: 5_000 })
        .catch((error) => {
          warnings.push(`page screenshot capture failed: ${error instanceof Error ? error.message : String(error)}`);
          return undefined;
        })
    : undefined;
  const interaction = detectRequiredInteraction(finalUrl, title, html, text, resources);
  if (interaction.requiresInteraction && interaction.reason) {
    warnings.push(interaction.reason);
  }

  return {
    finalUrl,
    title,
    html: html.slice(0, 5_000_000),
    text,
    screenshot: screenshotBuffer ? `data:image/png;base64,${screenshotBuffer.toString("base64")}` : undefined,
    resources,
    mhtmlBase64: options.mhtmlBase64,
    harJson: options.harJson,
    capturedAt: new Date().toISOString(),
    requiresInteraction: interaction.requiresInteraction,
    interactionReason: interaction.reason,
  };
}

function detectRequiredInteraction(
  finalUrl: string,
  title: string | undefined,
  html: string,
  text: string,
  resources: PageResource[],
): { requiresInteraction: boolean; reason?: string } {
  return detectRequiredInteractionFromVisiblePage(finalUrl, title, html, text, resources);
}

function detectRequiredInteractionFromVisiblePage(
  finalUrl: string,
  title: string | undefined,
  html: string,
  text: string,
  resources: PageResource[],
): { requiresInteraction: boolean; reason?: string } {
  const visibleText = text.replace(/\s+/g, " ").trim().toLowerCase();
  const pageTitle = (title ?? "").replace(/\s+/g, " ").trim().toLowerCase();
  const visibleHaystack = `${finalUrl.toLowerCase()}\n${pageTitle}\n${visibleText}`;
  const markupHaystack = html.slice(0, 1_000_000).replace(/\s+/g, " ").toLowerCase();
  const resourceText = resources
    .slice(0, 500)
    .map((resource) => `${resource.url} ${resource.contentType ?? ""} ${resource.resourceType ?? ""}`)
    .join(" ")
    .toLowerCase();

  if (
    /\bcloudflare\b/.test(visibleHaystack) &&
    /\b(?:challenge|turnstile|ray id|security check|security verification|verify you are human|checking your browser)\b/.test(visibleHaystack)
  ) {
    return { requiresInteraction: true, reason: "page is blocked by a Cloudflare security challenge" };
  }
  if (/(?:checking your browser|just a moment|verify you are human|robot check|security verification|security check)/.test(visibleHaystack)) {
    return { requiresInteraction: true, reason: "page requires a human verification interaction" };
  }
  if (/(?:cf-turnstile|challenges\.cloudflare\.com|turnstile\.render|turnstile)/.test(markupHaystack)) {
    if (/(?:turnstile|verify|verification|challenge|human|captcha|security)/.test(visibleHaystack)) {
      return { requiresInteraction: true, reason: "page requires a Cloudflare Turnstile interaction" };
    }
  }
  if (/(?:hcaptcha\.com|h-captcha|data-hcaptcha|hcaptcha)/.test(markupHaystack)) {
    if (/(?:hcaptcha|captcha|verify|verification|human|robot)/.test(visibleHaystack)) {
      return { requiresInteraction: true, reason: "page requires an hCaptcha interaction" };
    }
  }
  if (/(?:www\.google\.com\/recaptcha|g-recaptcha|grecaptcha|recaptcha)/.test(markupHaystack)) {
    if (/(?:recaptcha|captcha|verify|verification|human|robot)/.test(visibleHaystack)) {
      return { requiresInteraction: true, reason: "page requires a reCAPTCHA interaction" };
    }
  }
  if (/(?:challenges\.cloudflare\.com|hcaptcha\.com|recaptcha|g-recaptcha)/.test(resourceText)) {
    if (/(?:captcha|verify|verification|human|robot|security check|challenge)/.test(visibleHaystack)) {
      return { requiresInteraction: true, reason: "page requires a human verification interaction" };
    }
  }

  return { requiresInteraction: false };
}

async function adultVideoCandidatesFromPage(page: Page, pageUrl: string): Promise<BrowserCandidate[]> {
  if (!isAdultVideoPage(pageUrl)) {
    return [];
  }

  const discovered = await page.evaluate(() => {
    type Item = {
      url: string;
      source: string;
      contentType?: string;
      initiatorType?: string;
      scoreBoost?: number;
    };
    const out = new Map<string, Item>();
    const add = (value: unknown, source: string, scoreBoost = 36) => {
      if (typeof value !== "string" || !value) {
        return;
      }
      const decoded = value
        .replace(/\\u002[fF]/g, "/")
        .replace(/\\u0026/g, "&")
        .replace(/\\u003[dD]/g, "=")
        .replace(/\\\//g, "/");
      if (!/^https?:\/\//i.test(decoded) || !/\.(m3u8|mp4|m4v|webm)(?:[?#]|$)/i.test(decoded)) {
        return;
      }
      const contentType = decoded.includes(".m3u8") ? "application/vnd.apple.mpegurl" : "video/mp4";
      const current = out.get(decoded);
      const item = { url: decoded, source, contentType, initiatorType: "video", scoreBoost };
      if (!current || scoreBoost > (current.scoreBoost ?? 0)) {
        out.set(decoded, item);
      }
    };

    const scan = (value: unknown, source: string, depth = 0) => {
      if (depth > 7 || value === null || value === undefined) {
        return;
      }
      if (typeof value === "string") {
        add(value, source);
        return;
      }
      if (Array.isArray(value)) {
        for (const item of value.slice(0, 400)) {
          scan(item, source, depth + 1);
        }
        return;
      }
      if (typeof value === "object") {
        for (const item of Object.values(value as Record<string, unknown>).slice(0, 400)) {
          scan(item, source, depth + 1);
        }
      }
    };

    const win = window as unknown as Record<string, unknown>;
    for (const key of [
      "flashvars",
      "flashVars",
      "mediaDefinitions",
      "qualityItems",
      "videoVars",
      "playerObjList",
      "playerConfig",
      "videoData",
    ]) {
      scan(win[key], `adult_player_${key}`);
    }

    for (const script of Array.from(document.scripts)) {
      const text = (script.textContent || "")
        .replace(/\\u002[fF]/g, "/")
        .replace(/\\u0026/g, "&")
        .replace(/\\u003[dD]/g, "=")
        .replace(/\\\//g, "/")
        .slice(0, 1_500_000);
      for (const match of text.matchAll(/https?:\/\/[^"'<>\\\s]+?\.(?:m3u8|mp4|m4v|webm)(?:\?[^"'<>\\\s]*)?/gi)) {
        add(match[0], "adult_inline_player", 34);
      }
    }

    return Array.from(out.values()).slice(0, 100);
  });

  return discovered
    .map((candidate) => candidateFromDiscoveredUrl(candidate, pageUrl))
    .filter((candidate): candidate is BrowserCandidate => Boolean(candidate))
    .map((candidate) => ({
      ...candidate,
      score: candidate.score + 45,
      metadata: { ...(candidate.metadata ?? {}), source: "adult_video_player" },
    }));
}

function isAdultVideoPage(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host.includes("pornhub.") || host.includes("phncdn.com");
  } catch {
    return false;
  }
}

function hasPlayableMediaCandidate(candidates: BrowserCandidate[]): boolean {
  return candidates.some((candidate) => candidate.kind === "video" || candidate.kind === "manifest" || candidate.kind === "audio");
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
        kinds.add("audio");
        kinds.add("video");
        kinds.add("manifest");
        kinds.add("image");
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
    if (isLikelyAdOrTrackingUrl(candidate.url) && !isAllowedMainMediaHost(host)) {
      return true;
    }
    if (
      candidate.kind === "image" &&
      /(?:avatar|sprite|logo|icon|badge|emoji|thumbnail|thumb|poster|banner)/i.test(pathname)
    ) {
      return true;
    }
  } catch {
    return true;
  }
  return false;
}

function isLikelyAdOrTrackingUrl(value: string): boolean {
  try {
    const url = new URL(value);
    const host = url.hostname.toLowerCase();
    const pathAndQuery = `${url.pathname}${url.search}`.toLowerCase();
    return AD_HOST_PARTS.some((part) => host.includes(part)) || AD_PATH_PATTERN.test(pathAndQuery);
  } catch {
    return true;
  }
}

function isAllowedMainMediaHost(host: string): boolean {
  return (
    host.endsWith("googlevideo.com") ||
    host.endsWith("bilivideo.com") ||
    host.endsWith("bilibili.com") ||
    host.endsWith("sndcdn.com") ||
    host.endsWith("ximalaya.com") ||
    host.endsWith("xmcdn.com") ||
    host.endsWith("douyinvod.com") ||
    host.endsWith("douyinpic.com") ||
    host.endsWith("kwaicdn.com") ||
    host.endsWith("gifshow.com") ||
    host.endsWith("phncdn.com") ||
    host.endsWith("hembed.com") ||
    host.endsWith("saawsedge.com") ||
    host.endsWith("acfun.cn") ||
    host.endsWith("aixifan.com") ||
    host.endsWith("iqiyi.com") ||
    host.endsWith("qiyipic.com") ||
    host.endsWith("youku.com") ||
    host.endsWith("ykimg.com") ||
    host.endsWith("tiktokcdn.com") ||
    host.endsWith("byteoversea.com") ||
    host.endsWith("vimeocdn.com") ||
    host.endsWith("weibo.com") ||
    host.endsWith("weibocdn.com") ||
    host.endsWith("sinaimg.cn") ||
    host.endsWith("dailymotion.com") ||
    host.endsWith("dmcdn.net") ||
    host.endsWith("rumble.com") ||
    host.endsWith("rumblevideo.com") ||
    host.endsWith("archive.org") ||
    host.endsWith("archive-it.org") ||
    host.endsWith("perma.cc") ||
    isArchiveTodayHost(host) ||
    host.endsWith("ghostarchive.org") ||
    host.endsWith("webcitation.org") ||
    host.endsWith("mementoweb.org") ||
    host.endsWith("mementoarchive.lanl.gov") ||
    host.endsWith("wikimedia.org") ||
    host.endsWith("wikipedia.org") ||
    host.endsWith("twitch.tv") ||
    host.endsWith("ttvnw.net") ||
    host.endsWith("jtvnw.net") ||
    host.endsWith("x.com") ||
    host.endsWith("twitter.com") ||
    host.endsWith("twimg.com") ||
    host.endsWith("reddit.com") ||
    host.endsWith("redd.it") ||
    host.endsWith("redditmedia.com") ||
    host.endsWith("instagram.com") ||
    host.endsWith("cdninstagram.com") ||
    host.endsWith("facebook.com") ||
    host.endsWith("fbcdn.net") ||
    host.endsWith("pinterest.com") ||
    host.endsWith("pinimg.com") ||
    host.endsWith("imgur.com") ||
    host.endsWith("flickr.com") ||
    host.endsWith("staticflickr.com") ||
    host.endsWith("bandcamp.com") ||
    host.endsWith("bcbits.com") ||
    host.endsWith("mixcloud.com") ||
    host.endsWith("nicovideo.jp") ||
    host.endsWith("nimg.jp") ||
    host.endsWith("fc2.com") ||
    isKnownPeertubeHost(host)
  );
}

function hasPlatformHint(value: string | undefined, expected: string): boolean {
  return (value ?? "").toLowerCase() === expected;
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

function isDouyinUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return (
      host === "douyin.com" ||
      host.endsWith(".douyin.com") ||
      host.endsWith(".iesdouyin.com")
    );
  } catch {
    return false;
  }
}

/// Filter raw Douyin candidates down to the real post media on Douyin's CDNs,
/// dropping the page noise (avatars, UI sprites, effect assets, emoji). Modern
/// douyin.com renders via obfuscated RSC flight behind anti-bot, so the media
/// URLs only survive as raw strings in the generic scan; this keeps the ones
/// that matter:
///   - 图文 gallery images: *.douyinpic.com paths under /tos-cn-i- (not avatars)
///   - video: *.douyinvod.com or /video/tos/ or /aweme/v1/play (real stream)
///   - music: *.douyinpic.com /obj/ ... (passed through as audio)
/// Images are deduped by path (signed query params differ per request).
function filterDouyinCandidates(candidates: BrowserCandidate[]): BrowserCandidate[] {
  const out: BrowserCandidate[] = [];
  const seenImage = new Set<string>();
  let imageIndex = 0;
  for (const candidate of candidates) {
    let host = "";
    let pathname = "";
    try {
      const parsed = new URL(candidate.url);
      host = parsed.hostname.toLowerCase();
      pathname = parsed.pathname.toLowerCase();
    } catch {
      continue;
    }

    if (candidate.kind === "image") {
      const isGallery =
        host.endsWith("douyinpic.com") &&
        pathname.includes("/tos-cn-i-") &&
        !pathname.includes("avatar") &&
        !/(100x100|72x72|168x168)/.test(pathname);
      if (!isGallery || seenImage.has(pathname)) {
        continue;
      }
      seenImage.add(pathname);
      out.push({
        ...candidate,
        qualityLabel: `douyin-image-${imageIndex.toString().padStart(2, "0")}`,
        score: candidate.score + 25,
        metadata: { ...(candidate.metadata ?? {}), source: "douyin_gallery", index: imageIndex },
      });
      imageIndex += 1;
    } else if (candidate.kind === "video") {
      const isRealVideo =
        host.includes("douyinvod") ||
        pathname.includes("/video/tos/") ||
        pathname.includes("/aweme/v1/play");
      if (!isRealVideo) {
        continue;
      }
      out.push({ ...candidate, score: candidate.score + 25 });
    } else {
      // Music / audio: keep douyin-hosted audio only.
      if (host.endsWith("douyinpic.com") || host.endsWith("douyinvod.com") || host.endsWith("amemv.com")) {
        out.push(candidate);
      }
    }
  }
  return out;
}

function isKuaishouUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return (
      host === "kuaishou.com" ||
      host.endsWith(".kuaishou.com") ||
      host === "v.kuaishou.com" ||
      host.endsWith(".gifshow.com")
    );
  } catch {
    return false;
  }
}

function kuaishouMobileFallbackUrl(...values: string[]): string | undefined {
  for (const value of values) {
    try {
      const url = new URL(value);
      const id = url.pathname.match(/\/short-video\/([^/?#]+)/)?.[1]
        ?? url.pathname.match(/\/fw\/photo\/([^/?#]+)/)?.[1];
      if (!id) {
        continue;
      }
      return `https://m.kuaishou.com/fw/photo/${encodeURIComponent(id)}`;
    } catch {
      continue;
    }
  }
  return undefined;
}

function filterKuaishouCandidates(candidates: BrowserCandidate[]): BrowserCandidate[] {
  const out: BrowserCandidate[] = [];
  const seen = new Set<string>();
  for (const candidate of candidates) {
    let host = "";
    let pathname = "";
    try {
      const parsed = new URL(candidate.url);
      host = parsed.hostname.toLowerCase();
      pathname = parsed.pathname.toLowerCase();
    } catch {
      continue;
    }

    const allowedHost =
      host.endsWith("kwaicdn.com") ||
      host.endsWith("gifshow.com") ||
      host.endsWith("kuaishou.com");
    if (!allowedHost || isLikelyAdOrTrackingUrl(candidate.url)) {
      continue;
    }
    if (candidate.kind === "image" && /(avatar|profile|icon|logo|emoji|sprite|badge)/i.test(pathname)) {
      continue;
    }
    const key = `${candidate.kind}:${pathname}:${candidate.qualityLabel ?? ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    out.push({
      ...candidate,
      score: candidate.score + (candidate.kind === "video" || candidate.kind === "manifest" ? 30 : 8),
      metadata: { ...(candidate.metadata ?? {}), source: "kuaishou_filter" },
    });
  }
  return out;
}

function filterAdultVideoCandidates(candidates: BrowserCandidate[]): BrowserCandidate[] {
  const out: BrowserCandidate[] = [];
  const seen = new Set<string>();
  for (const candidate of candidates) {
    let host = "";
    let pathname = "";
    try {
      const parsed = new URL(candidate.url);
      host = parsed.hostname.toLowerCase();
      pathname = parsed.pathname.toLowerCase();
    } catch {
      continue;
    }

    const isMainMedia =
      host.endsWith("phncdn.com") ||
      host.includes("pornhub.") ||
      pathname.includes("/videos/") ||
      pathname.includes("/hls/");
    if (!isMainMedia || isLikelyAdOrTrackingUrl(candidate.url)) {
      continue;
    }
    if (candidate.kind === "image") {
      continue;
    }
    const key = `${candidate.kind}:${pathname}:${candidate.qualityLabel ?? ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    out.push({
      ...candidate,
      score: candidate.score + (candidate.resourceType?.includes("adult") ? 60 : 20),
      metadata: { ...(candidate.metadata ?? {}), source: "adult_video_filter" },
    });
  }
  return out;
}

function filterHAnimeCandidates(candidates: BrowserCandidate[]): BrowserCandidate[] {
  const out: BrowserCandidate[] = [];
  const seen = new Set<string>();
  const hasCompleteMp4 = candidates.some((candidate) => {
    try {
      const parsed = new URL(candidate.url);
      const host = parsed.hostname.toLowerCase();
      const pathname = parsed.pathname.toLowerCase();
      return (
        candidate.kind === "video" &&
        host.endsWith("hembed.com") &&
        pathname.endsWith(".mp4") &&
        !isLikelySegmentFragment(pathname)
      );
    } catch {
      return false;
    }
  });

  for (const candidate of candidates) {
    let host = "";
    let pathname = "";
    try {
      const parsed = new URL(candidate.url);
      host = parsed.hostname.toLowerCase();
      pathname = parsed.pathname.toLowerCase();
    } catch {
      continue;
    }

    const allowedHost =
      host.endsWith("hembed.com") ||
      host.endsWith("saawsedge.com") ||
      host.endsWith("hanime1.me");
    if (!allowedHost || isLikelyAdOrTrackingUrl(candidate.url) || candidate.kind === "image") {
      continue;
    }
    if (hasCompleteMp4 && isLikelySegmentFragment(pathname)) {
      continue;
    }
    const key = `${candidate.kind}:${host}:${pathname}:${candidate.qualityLabel ?? ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    const completeMp4Boost =
      host.endsWith("hembed.com") && pathname.endsWith(".mp4") && !isLikelySegmentFragment(pathname)
        ? 95
        : 5;
    out.push({
      ...candidate,
      score: candidate.score + completeMp4Boost,
      metadata: { ...(candidate.metadata ?? {}), source: "hanime_filter", platform: "hanime1" },
    });
  }
  return out;
}

function isLikelySegmentFragment(pathname: string): boolean {
  return /(?:^|\/)[^/]+_(?:init|\d+)_/i.test(pathname) || /(?:segment|frag|chunk)[-_]?\d+/i.test(pathname);
}

type CnVideoPlatform = "acfun" | "iqiyi" | "youku";

function filterCnVideoPlatformCandidates(
  candidates: BrowserCandidate[],
  platform: CnVideoPlatform,
): BrowserCandidate[] {
  const out: BrowserCandidate[] = [];
  const seen = new Set<string>();
  for (const candidate of candidates) {
    let host = "";
    let pathname = "";
    try {
      const parsed = new URL(candidate.url);
      host = parsed.hostname.toLowerCase();
      pathname = parsed.pathname.toLowerCase();
    } catch {
      continue;
    }

    if (isLikelyAdOrTrackingUrl(candidate.url)) {
      continue;
    }
    const allowed = isCnPlatformMediaHost(platform, host, pathname);
    if (!allowed) {
      continue;
    }
    if (candidate.kind === "image" && /(avatar|profile|icon|logo|emoji|sprite|badge|poster|cover)/i.test(pathname)) {
      continue;
    }
    const key = `${candidate.kind}:${host}:${pathname}:${candidate.qualityLabel ?? ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    out.push({
      ...candidate,
      score: candidate.score + (candidate.kind === "manifest" ? 45 : candidate.kind === "video" ? 35 : 5),
      requiresAuthorization: platform === "iqiyi" ? true : candidate.requiresAuthorization,
      failureReason:
        platform === "iqiyi"
          ? "iQIYI browser manifest segment replay is blocked by QWS 403; needs dedicated iQIYI runtime signature adapter"
          : candidate.failureReason,
      validationState: platform === "iqiyi" ? "failed" : candidate.validationState,
      metadata: {
        ...(candidate.metadata ?? {}),
        source: `${platform}_filter`,
        platform,
        replayHeadersRequired: platform === "iqiyi" ? true : undefined,
      },
    });
  }
  return out;
}

function isCnPlatformMediaHost(platform: CnVideoPlatform, host: string, pathname: string): boolean {
  const isManifestOrMediaPath =
    pathname.includes(".m3u8") ||
    pathname.includes(".mpd") ||
    pathname.includes(".mp4") ||
    pathname.includes(".m4s") ||
    pathname.includes(".ts") ||
    pathname.includes("/dash") ||
    pathname.includes("/hls") ||
    pathname.includes("/stream");
  if (!isManifestOrMediaPath) {
    return false;
  }
  if (platform === "acfun") {
    return host.endsWith("acfun.cn") || host.endsWith("aixifan.com") || host.includes("acfun");
  }
  if (platform === "iqiyi") {
    if (host.startsWith("static-") || pathname.includes("/lequ/") || pathname.includes("/ad/")) {
      return false;
    }
    return (
      host.includes("cache.video") ||
      host.includes("cache.m") ||
      host.includes("data.video") ||
      host.includes("meta.video") ||
      host.includes("qiyi") ||
      host.includes("iqiyi")
    );
  }
  return host.endsWith("youku.com") || host.endsWith("ykimg.com") || host.includes("youku");
}

function filterTikTokCandidates(candidates: BrowserCandidate[]): BrowserCandidate[] {
  const out: BrowserCandidate[] = [];
  const seen = new Set<string>();
  for (const candidate of candidates) {
    let host = "";
    let pathname = "";
    try {
      const parsed = new URL(candidate.url);
      host = parsed.hostname.toLowerCase();
      pathname = parsed.pathname.toLowerCase();
    } catch {
      continue;
    }
    const allowed =
      host.endsWith("tiktokcdn.com") ||
      host.endsWith("tiktokv.com") ||
      host.endsWith("byteoversea.com") ||
      pathname.includes("/video/tos/");
    if (!allowed || isLikelyAdOrTrackingUrl(candidate.url)) {
      continue;
    }
    if (candidate.kind === "image" && /(avatar|profile|icon|logo|emoji|sprite|badge)/i.test(pathname)) {
      continue;
    }
    const key = `${candidate.kind}:${pathname}:${candidate.qualityLabel ?? ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    out.push({
      ...candidate,
      score: candidate.score + (candidate.kind === "video" || candidate.kind === "manifest" ? 35 : 8),
      metadata: { ...(candidate.metadata ?? {}), source: "tiktok_filter", platform: "tiktok" },
    });
  }
  return out;
}

function filterVimeoCandidates(candidates: BrowserCandidate[]): BrowserCandidate[] {
  const out: BrowserCandidate[] = [];
  const seen = new Set<string>();
  for (const candidate of candidates) {
    let host = "";
    let pathname = "";
    try {
      const parsed = new URL(candidate.url);
      host = parsed.hostname.toLowerCase();
      pathname = parsed.pathname.toLowerCase();
    } catch {
      continue;
    }
    const allowed =
      host.endsWith("vimeocdn.com") ||
      host.endsWith("vimeo.com") ||
      pathname.includes("/video/") ||
      pathname.includes("/play/");
    if (!allowed || isLikelyAdOrTrackingUrl(candidate.url) || candidate.kind === "image") {
      continue;
    }
    const key = `${candidate.kind}:${pathname}:${candidate.qualityLabel ?? ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    out.push({
      ...candidate,
      score: candidate.score + (candidate.kind === "manifest" ? 45 : 28),
      metadata: { ...(candidate.metadata ?? {}), source: "vimeo_filter", platform: "vimeo" },
    });
  }
  return out;
}

function filterWeiboCandidates(candidates: BrowserCandidate[]): BrowserCandidate[] {
  const out: BrowserCandidate[] = [];
  const seen = new Set<string>();
  for (const candidate of candidates) {
    let host = "";
    let pathname = "";
    try {
      const parsed = new URL(candidate.url);
      host = parsed.hostname.toLowerCase();
      pathname = parsed.pathname.toLowerCase();
    } catch {
      continue;
    }
    const allowed =
      host.endsWith("weibocdn.com") ||
      host.endsWith("weibo.com") ||
      (host.endsWith("sinaimg.cn") && (pathname.includes("/video/") || pathname.includes(".mp4") || pathname.includes(".m3u8")));
    if (!allowed || isLikelyAdOrTrackingUrl(candidate.url) || candidate.kind === "image") {
      continue;
    }
    const key = `${candidate.kind}:${host}:${pathname}:${candidate.qualityLabel ?? ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    out.push({
      ...candidate,
      score: candidate.score + (candidate.kind === "manifest" ? 42 : 26),
      metadata: { ...(candidate.metadata ?? {}), source: "weibo_filter", platform: "weibo" },
    });
  }
  return out;
}

function filterXimalayaCandidates(candidates: BrowserCandidate[]): BrowserCandidate[] {
  const out: BrowserCandidate[] = [];
  const seen = new Set<string>();
  for (const candidate of candidates) {
    let host = "";
    let pathname = "";
    try {
      const parsed = new URL(candidate.url);
      host = parsed.hostname.toLowerCase();
      pathname = parsed.pathname.toLowerCase();
    } catch {
      continue;
    }
    const allowed =
      host.endsWith("ximalaya.com") ||
      host.endsWith("xmcdn.com") ||
      pathname.endsWith(".m4a") ||
      pathname.endsWith(".mp3");
    if (!allowed || isLikelyAdOrTrackingUrl(candidate.url) || candidate.kind === "image") {
      continue;
    }
    const key = `${candidate.kind}:${host}:${pathname}:${candidate.qualityLabel ?? ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    out.push({
      ...candidate,
      score: candidate.score + (candidate.kind === "audio" ? 40 : candidate.kind === "manifest" ? 24 : 12),
      metadata: { ...(candidate.metadata ?? {}), source: "ximalaya_filter", platform: "ximalaya" },
    });
  }
  return out;
}

type VideoSocialPlatform =
  | "dailymotion"
  | "rumble"
  | "peertube"
  | "twitch"
  | "twitter"
  | "reddit"
  | "instagram"
  | "facebook"
  | "niconico"
  | "fc2";

function filterVideoSocialCandidates(
  candidates: BrowserCandidate[],
  platform: VideoSocialPlatform,
): BrowserCandidate[] {
  const out: BrowserCandidate[] = [];
  const seen = new Set<string>();
  for (const candidate of candidates) {
    let host = "";
    let pathname = "";
    try {
      const parsed = new URL(candidate.url);
      host = parsed.hostname.toLowerCase();
      pathname = parsed.pathname.toLowerCase();
    } catch {
      continue;
    }

    if (!isVideoSocialMediaHost(platform, host, pathname) || isLikelyAdOrTrackingUrl(candidate.url)) {
      continue;
    }
    if (candidate.kind === "image" && /(avatar|profile|icon|logo|emoji|sprite|badge|poster|thumbnail|thumb)/i.test(pathname)) {
      continue;
    }
    if (candidate.kind === "image" && !["instagram", "twitter", "reddit", "facebook"].includes(platform)) {
      continue;
    }
    const key = `${candidate.kind}:${host}:${canonicalCandidatePath(pathname)}:${candidate.qualityLabel ?? ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    out.push({
      ...candidate,
      score: candidate.score + videoSocialScoreBoost(candidate.kind, platform),
      metadata: { ...(candidate.metadata ?? {}), source: `${platform}_filter`, platform },
    });
  }
  return out;
}

function isVideoSocialMediaHost(platform: VideoSocialPlatform, host: string, pathname: string): boolean {
  const mediaPath =
    pathname.includes(".m3u8") ||
    pathname.includes(".mpd") ||
    pathname.includes(".mp4") ||
    pathname.includes(".m4s") ||
    pathname.includes(".ts") ||
    pathname.includes(".webm") ||
    pathname.includes("/video") ||
    pathname.includes("/media") ||
    pathname.includes("/hls") ||
    pathname.includes("/dash") ||
    pathname.includes("/vod") ||
    pathname.includes("/play") ||
    pathname.includes("/stream");
  if (platform === "dailymotion") {
    return (host.endsWith("dmcdn.net") || host.endsWith("dailymotion.com")) && mediaPath;
  }
  if (platform === "rumble") {
    return (host.endsWith("rumblevideo.com") || host.endsWith("rumble.com")) && mediaPath;
  }
  if (platform === "peertube") {
    return isKnownPeertubeHost(host) && mediaPath;
  }
  if (platform === "twitch") {
    return (host.endsWith("twitch.tv") || host.endsWith("ttvnw.net") || host.endsWith("jtvnw.net")) && mediaPath;
  }
  if (platform === "twitter") {
    return (host.endsWith("twimg.com") || host.endsWith("x.com") || host.endsWith("twitter.com")) && (mediaPath || pathname.includes("/tweet_video") || pathname.includes("/ext_tw_video"));
  }
  if (platform === "reddit") {
    return (host.endsWith("redditmedia.com") || host.endsWith("redd.it") || host.endsWith("reddit.com")) && (mediaPath || pathname.includes("/dashplaylist"));
  }
  if (platform === "instagram") {
    return (host.endsWith("cdninstagram.com") || host.endsWith("instagram.com") || host.endsWith("fbcdn.net")) && (mediaPath || pathname.includes("/o1/") || pathname.includes("/v/"));
  }
  if (platform === "facebook") {
    return (host.endsWith("fbcdn.net") || host.endsWith("facebook.com")) && mediaPath;
  }
  if (platform === "niconico") {
    return (host.endsWith("nicovideo.jp") || host.endsWith("nimg.jp") || host.includes("dmc.nico")) && mediaPath;
  }
  return host.endsWith("fc2.com") && mediaPath;
}

function videoSocialScoreBoost(kind: CandidateKind, platform: VideoSocialPlatform): number {
  if (kind === "manifest") return platform === "twitch" ? 55 : 45;
  if (kind === "video") return 35;
  if (kind === "audio") return 18;
  if (kind === "image") return 8;
  return 0;
}

type LibraryArchivePlatform =
  | "archive_org"
  | "wayback"
  | "archive_it"
  | "perma_cc"
  | "archive_today"
  | "ghostarchive"
  | "webcitation"
  | "memento"
  | "wikimedia";

function filterLibraryArchiveCandidates(
  candidates: BrowserCandidate[],
  platform: LibraryArchivePlatform,
): BrowserCandidate[] {
  const out: BrowserCandidate[] = [];
  const seen = new Set<string>();
  for (const candidate of candidates) {
    let host = "";
    let pathname = "";
    try {
      const parsed = new URL(candidate.url);
      host = parsed.hostname.toLowerCase();
      pathname = parsed.pathname.toLowerCase();
    } catch {
      continue;
    }
    const allowed = isLibraryArchiveMediaHost(platform, host, pathname);
    if (!allowed || isLikelyAdOrTrackingUrl(candidate.url)) {
      continue;
    }
    if (candidate.kind === "image" && /(avatar|logo|icon|sprite|badge|thumb|thumbnail)/i.test(pathname)) {
      continue;
    }
    const key = `${candidate.kind}:${host}:${canonicalCandidatePath(pathname)}:${candidate.qualityLabel ?? ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    out.push({
      ...candidate,
      score: candidate.score + (candidate.kind === "video" || candidate.kind === "audio" || candidate.kind === "manifest" ? 32 : 18),
      metadata: { ...(candidate.metadata ?? {}), source: `${platform}_filter`, platform },
    });
  }
  return out;
}

function isLibraryArchiveMediaHost(platform: LibraryArchivePlatform, host: string, pathname: string): boolean {
  switch (platform) {
    case "archive_org":
      return host.endsWith("archive.org") && !isWaybackHost(host, pathname);
    case "wayback":
      return isWaybackHost(host, pathname);
    case "archive_it":
      return host === "archive-it.org" || host.endsWith(".archive-it.org");
    case "perma_cc":
      return host === "perma.cc" || host.endsWith(".perma.cc");
    case "archive_today":
      return isArchiveTodayHost(host);
    case "ghostarchive":
      return host === "ghostarchive.org" || host.endsWith(".ghostarchive.org");
    case "webcitation":
      return host === "webcitation.org" || host.endsWith(".webcitation.org");
    case "memento":
      return host.endsWith("mementoweb.org") || host.endsWith("mementoarchive.lanl.gov");
    case "wikimedia":
      return host.endsWith("wikimedia.org") || host.endsWith("wikipedia.org");
  }
}

type ImagePlatform = "pinterest" | "imgur" | "flickr";

function filterImagePlatformCandidates(
  candidates: BrowserCandidate[],
  platform: ImagePlatform,
): BrowserCandidate[] {
  const out: BrowserCandidate[] = [];
  const seen = new Set<string>();
  for (const candidate of candidates) {
    let host = "";
    let pathname = "";
    try {
      const parsed = new URL(candidate.url);
      host = parsed.hostname.toLowerCase();
      pathname = parsed.pathname.toLowerCase();
    } catch {
      continue;
    }
    if (!isImagePlatformMediaHost(platform, host, pathname) || isLikelyAdOrTrackingUrl(candidate.url)) {
      continue;
    }
    if (candidate.kind === "image" && /(avatar|profile|icon|logo|emoji|sprite|badge)/i.test(pathname)) {
      continue;
    }
    if (candidate.kind !== "image" && candidate.kind !== "video" && candidate.kind !== "manifest") {
      continue;
    }
    const key = `${candidate.kind}:${host}:${canonicalCandidatePath(pathname)}:${candidate.qualityLabel ?? ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    out.push({
      ...candidate,
      score: candidate.score + (candidate.kind === "image" ? 42 : 18),
      metadata: { ...(candidate.metadata ?? {}), source: `${platform}_filter`, platform },
    });
  }
  return out;
}

function isImagePlatformMediaHost(platform: ImagePlatform, host: string, pathname: string): boolean {
  const hasImagePath = /\.(?:jpe?g|png|webp|gif|avif)(?:$|[?#])/i.test(pathname);
  const hasVideoPath = /\.(?:mp4|webm|m3u8|mpd)(?:$|[?#])/i.test(pathname);
  if (platform === "pinterest") {
    return (host.endsWith("pinimg.com") || host.endsWith("pinterest.com")) && (hasImagePath || hasVideoPath);
  }
  if (platform === "imgur") {
    return host.endsWith("imgur.com") && (hasImagePath || hasVideoPath);
  }
  return (host.endsWith("flickr.com") || host.endsWith("staticflickr.com")) && (hasImagePath || hasVideoPath);
}

type AudioPlatform = "bandcamp" | "mixcloud";

function filterAudioPlatformCandidates(
  candidates: BrowserCandidate[],
  platform: AudioPlatform,
): BrowserCandidate[] {
  const out: BrowserCandidate[] = [];
  const seen = new Set<string>();
  for (const candidate of candidates) {
    let host = "";
    let pathname = "";
    try {
      const parsed = new URL(candidate.url);
      host = parsed.hostname.toLowerCase();
      pathname = parsed.pathname.toLowerCase();
    } catch {
      continue;
    }
    const allowed = platform === "bandcamp"
      ? host.endsWith("bandcamp.com") || host.endsWith("bcbits.com")
      : host.endsWith("mixcloud.com") || pathname.includes("/stream");
    if (!allowed || isLikelyAdOrTrackingUrl(candidate.url) || candidate.kind === "image") {
      continue;
    }
    const key = `${candidate.kind}:${host}:${canonicalCandidatePath(pathname)}:${candidate.qualityLabel ?? ""}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    out.push({
      ...candidate,
      score: candidate.score + (candidate.kind === "audio" ? 42 : candidate.kind === "manifest" ? 22 : 10),
      metadata: { ...(candidate.metadata ?? {}), source: `${platform}_filter`, platform },
    });
  }
  return out;
}

function canonicalCandidatePath(pathname: string): string {
  return pathname.replace(/\/\d+x\d+(?=\/|$)/g, "/size").replace(/_[a-z]\.(jpg|jpeg|png|webp)$/i, ".$1");
}

function isAcfunUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "acfun.cn" || host.endsWith(".acfun.cn") || host.endsWith(".aixifan.com");
  } catch {
    return false;
  }
}

function isIqiyiUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "iqiyi.com" || host.endsWith(".iqiyi.com") || host.endsWith(".qiyi.com");
  } catch {
    return false;
  }
}

function isYoukuUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "youku.com" || host.endsWith(".youku.com") || host.endsWith(".ykimg.com");
  } catch {
    return false;
  }
}

function isTikTokUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "tiktok.com" || host.endsWith(".tiktok.com") || host.endsWith(".tiktokcdn.com");
  } catch {
    return false;
  }
}

function isVimeoUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "vimeo.com" || host.endsWith(".vimeo.com") || host.endsWith(".vimeocdn.com");
  } catch {
    return false;
  }
}

function isWeiboUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return (
      host === "weibo.com" ||
      host.endsWith(".weibo.com") ||
      host.endsWith(".weibocdn.com") ||
      host.endsWith(".sinaimg.cn")
    );
  } catch {
    return false;
  }
}

function isXimalayaUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return (
      host === "ximalaya.com" ||
      host.endsWith(".ximalaya.com") ||
      host.endsWith(".xmcdn.com")
    );
  } catch {
    return false;
  }
}

function isDailymotionUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "dailymotion.com" || host.endsWith(".dailymotion.com") || host.endsWith(".dmcdn.net");
  } catch {
    return false;
  }
}

function isRumbleUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "rumble.com" || host.endsWith(".rumble.com") || host.endsWith(".rumblevideo.com");
  } catch {
    return false;
  }
}

function isKnownPeertubeHost(host: string): boolean {
  return (
    host === "video.blender.org" ||
    host.endsWith(".video.blender.org") ||
    host === "peertube.tv" ||
    host.endsWith(".peertube.tv") ||
    host === "framatube.org" ||
    host.endsWith(".framatube.org") ||
    host === "tilvids.com" ||
    host.endsWith(".tilvids.com") ||
    host === "tube.tchncs.de" ||
    host.endsWith(".tube.tchncs.de") ||
    host === "diode.zone" ||
    host.endsWith(".diode.zone")
  );
}

function isPeertubeUrl(value: string): boolean {
  try {
    return isKnownPeertubeHost(new URL(value).hostname.toLowerCase());
  } catch {
    return false;
  }
}

function isArchiveOrgUrl(value: string): boolean {
  try {
    const parsed = new URL(value);
    const host = parsed.hostname.toLowerCase();
    return (host === "archive.org" || host.endsWith(".archive.org")) && !isWaybackHost(host, parsed.pathname.toLowerCase());
  } catch {
    return false;
  }
}

function isWaybackUrl(value: string): boolean {
  try {
    const parsed = new URL(value);
    return isWaybackHost(parsed.hostname.toLowerCase(), parsed.pathname.toLowerCase());
  } catch {
    return false;
  }
}

function isWaybackHost(host: string, pathname: string): boolean {
  return host === "web.archive.org" || host.endsWith(".web.archive.org") || ((host === "archive.org" || host.endsWith(".archive.org")) && pathname.startsWith("/web/"));
}

function isArchiveItUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "archive-it.org" || host.endsWith(".archive-it.org");
  } catch {
    return false;
  }
}

function isPermaCcUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "perma.cc" || host.endsWith(".perma.cc");
  } catch {
    return false;
  }
}

function isArchiveTodayUrl(value: string): boolean {
  try {
    return isArchiveTodayHost(new URL(value).hostname.toLowerCase());
  } catch {
    return false;
  }
}

function isArchiveTodayHost(host: string): boolean {
  return ["archive.today", "archive.ph", "archive.is", "archive.vn", "archive.md", "archive.li", "archive.fo"].includes(host);
}

function isGhostarchiveUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "ghostarchive.org" || host.endsWith(".ghostarchive.org");
  } catch {
    return false;
  }
}

function isWebcitationUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "webcitation.org" || host.endsWith(".webcitation.org");
  } catch {
    return false;
  }
}

function isMementoUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host.endsWith("mementoweb.org") || host.endsWith("mementoarchive.lanl.gov");
  } catch {
    return false;
  }
}

function isWikimediaUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host.endsWith("wikimedia.org") || host.endsWith("wikipedia.org");
  } catch {
    return false;
  }
}

function isTwitchUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "twitch.tv" || host.endsWith(".twitch.tv") || host.endsWith(".ttvnw.net") || host.endsWith(".jtvnw.net");
  } catch {
    return false;
  }
}

function isTwitterUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "x.com" || host.endsWith(".x.com") || host === "twitter.com" || host.endsWith(".twitter.com") || host.endsWith(".twimg.com");
  } catch {
    return false;
  }
}

function isRedditUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "reddit.com" || host.endsWith(".reddit.com") || host.endsWith(".redd.it") || host.endsWith(".redditmedia.com") || host.endsWith(".redditstatic.com");
  } catch {
    return false;
  }
}

function isInstagramUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "instagram.com" || host.endsWith(".instagram.com") || host.endsWith(".cdninstagram.com");
  } catch {
    return false;
  }
}

function isFacebookUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "facebook.com" || host.endsWith(".facebook.com") || host === "fb.watch" || host.endsWith(".fbcdn.net");
  } catch {
    return false;
  }
}

function isPinterestUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "pinterest.com" || host.endsWith(".pinterest.com") || host === "pin.it" || host.endsWith(".pinimg.com");
  } catch {
    return false;
  }
}

function isImgurUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "imgur.com" || host.endsWith(".imgur.com");
  } catch {
    return false;
  }
}

function isFlickrUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "flickr.com" || host.endsWith(".flickr.com") || host.endsWith(".staticflickr.com") || host === "flic.kr";
  } catch {
    return false;
  }
}

function isBandcampUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host.endsWith(".bandcamp.com") || host === "bandcamp.com" || host.endsWith(".bcbits.com");
  } catch {
    return false;
  }
}

function isMixcloudUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "mixcloud.com" || host.endsWith(".mixcloud.com");
  } catch {
    return false;
  }
}

function isNiconicoUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host.endsWith("nicovideo.jp") || host.endsWith("niconico.com") || host.endsWith("nimg.jp") || host.includes("dmc.nico");
  } catch {
    return false;
  }
}

function isFc2Url(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "fc2.com" || host.endsWith(".fc2.com");
  } catch {
    return false;
  }
}

function isSpotifyUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "spotify.com" || host.endsWith(".spotify.com") || host === "spotify.link";
  } catch {
    return false;
  }
}

function isHAnimeUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return host === "hanime1.me" || host.endsWith(".hanime1.me") || host.endsWith("hembed.com");
  } catch {
    return false;
  }
}

function isEpisodeAggregatorUrl(value: string): boolean {
  try {
    const host = new URL(value).hostname.toLowerCase();
    return (
      host === "dmttang.com" ||
      host.endsWith(".dmttang.com") ||
      host === "83dm.com" ||
      host.endsWith(".83dm.com") ||
      host.includes("yinghua")
    );
  } catch {
    return false;
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

function shouldTriggerPlayback(
  _url: string,
  _platformHint: string | undefined,
  outputs: string[] | undefined,
  _hasStaticPlayableCandidate = false,
): boolean {
  // Attempt playback for any job that wants audio/video. triggerPlayback is a
  // safe no-op when the page has no player, so this is broadly applicable — and
  // it is exactly what lets JS-resolved sites (StreetVoice, Douyin, Bilibili,
  // ...) reveal their real media request instead of only the page URL.
  return !outputs?.length || outputs.some((output) => output === "audio" || output === "video");
}

function shouldAvoidGenericPlaybackClick(
  finalUrl: string,
  originalUrl: string,
  hasStaticPlayableCandidate: boolean,
): boolean {
  return hasStaticPlayableCandidate && (isEpisodeAggregatorUrl(finalUrl) || isEpisodeAggregatorUrl(originalUrl));
}

const PLAY_SELECTORS = [
  // Generic ARIA / title
  "button[aria-label='Play']",
  "button[aria-label^='Play ']",
  "button[aria-label*='play' i]",
  "button[title='Play']",
  "button[title*='play' i]",
  "[aria-label*='播放']",
  "[title*='播放']",
  // Generic class / data hooks
  "button.playButton",
  ".playButton",
  ".play-button",
  ".play-btn",
  ".btn-play",
  "[data-testid='play-button']",
  // SoundCloud
  ".sc-button-play",
  // YouTube
  "button.ytp-large-play-button",
  "button.ytp-play-button",
  // StreetVoice
  ".player-control .play",
  ".sv-player .play",
  ".player .play",
  ".icon-play",
  // Bilibili / common CN HTML5 players
  ".bpx-player-ctrl-play",
  ".squirtle-video-start",
  ".xgplayer-play",
  ".xgplayer-start",
  ".xgplayer-start-button",
  ".iqp-btn-play",
  ".iqp-playbutton",
  ".kui-player-play",
  ".yk-player-play",
  ".youku-player-play",
  ".txp_btn_play",
  ".acfun-player-play",
  ".danmaku-player .play",
  ".play-icon",
  // Common adult/video players
  ".mgp_playIcon",
  ".mgp_play",
  ".mhp1138_playIcon",
  ".js-play",
  ".playButton",
  ".play_button",
  "[data-role='play']",
];

async function triggerPlayback(page: Page, warnings: string[]): Promise<boolean> {
  await dismissConsentPrompt(page);
  await noteAgeGate(page, warnings);
  await dismissPlaybackOverlays(page, warnings);

  let clicked = await clickFirstVisible(page, PLAY_SELECTORS);

  if (!clicked) {
    // Fall back to clicking the page's own media element.
    for (const selector of ["video", "audio"]) {
      const ok = await page
        .locator(selector)
        .first()
        .click({ timeout: 1_000 })
        .then(() => true)
        .catch(() => false);
      if (ok) {
        clicked = true;
        break;
      }
    }
  }

  if (!clicked) {
    // Last resort: ask any media element to play() directly (muted, to satisfy
    // autoplay policies).
    clicked = await page
      .evaluate(() => {
        const media = document.querySelector("video,audio") as HTMLMediaElement | null;
        if (!media) {
          return false;
        }
        media.muted = true;
        const result = media.play() as unknown as Promise<void> | undefined;
        if (result && typeof result.catch === "function") {
          result.catch(() => undefined);
        }
        return true;
      })
      .catch(() => false);
  }

  if (!clicked) {
    warnings.push("no play control found");
  }

  await page.waitForTimeout(1_500);
  await dismissPlaybackAds(page, warnings);
  await dismissPlaybackOverlays(page, warnings);
  await page.waitForTimeout(4_000);
  return clicked;
}

async function dismissPlaybackOverlays(page: Page, warnings: string[]): Promise<void> {
  let clicked = false;
  for (const selector of [
    "button:has-text('Accept All')",
    "button:has-text('I Agree')",
    "button:has-text('Agree')",
    "button:has-text('Continue')",
    "button:has-text('Enter')",
    "button:has-text('Close')",
    "button:has-text('No thanks')",
    "button:has-text('Not now')",
    "button[aria-label*='close' i]",
    ".modal-close",
    ".close",
    ".mgp_announce_close",
    ".mgp_skip",
    ".skipButton",
  ]) {
    const ok = await clickFirstVisible(page, [selector]);
    if (ok) {
      clicked = true;
      await page.waitForTimeout(400);
    }
  }
  if (clicked) {
    warnings.push("player overlay closed");
  }
}

async function dismissPlaybackAds(page: Page, warnings: string[]): Promise<void> {
  const deadline = Date.now() + 12_000;
  let clicked = false;

  while (Date.now() < deadline) {
    const skipped = await clickFirstVisible(page, [
      "button[aria-label*='Skip' i]",
      "button[title*='Skip' i]",
      "button:has-text('Skip Ad')",
      "button:has-text('Skip Ads')",
      "button:has-text('Skip')",
      "button:has-text('\\u8df3\\u8fc7\\u5e7f\\u544a')",
      "button:has-text('\\u8df3\\u8fc7')",
      "button:has-text('\\u8df3\\u904e\\u5ee3\\u544a')",
      "button:has-text('\\u8df3\\u904e')",
      ".ytp-ad-skip-button",
      ".ytp-ad-skip-button-modern",
    ]);
    if (skipped) {
      clicked = true;
      await page.waitForTimeout(1_000);
      continue;
    }

    const closedOverlay = await clickFirstVisible(page, [
      "button[aria-label*='Close' i]",
      "button[title*='Close' i]",
      "button:has-text('Close')",
      "button:has-text('\\u5173\\u95ed')",
      "button:has-text('\\u95dc\\u9589')",
      ".close",
      ".modal-close",
    ]);
    if (closedOverlay) {
      clicked = true;
      await page.waitForTimeout(1_000);
      continue;
    }

    await page.waitForTimeout(1_000);
  }

  if (clicked) {
    warnings.push("ad overlay skipped or closed");
  }
}

async function noteAgeGate(page: Page, warnings: string[]): Promise<void> {
  for (const name of [
    /i am over 18/i,
    /enter site/i,
    /^enter$/i,
    /agree and enter/i,
    /我已满\s*18/,
    /满\s*18/,
    /同意并进入/,
    /^同意$/,
    /continue/i,
  ]) {
    const button = page.getByRole("button", { name }).first();
    if (await isVisible(button)) {
      warnings.push("age confirmation visible; user profile or cookies may be required");
      return;
    }
  }
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

async function removeStaleChromiumSingletonLocks(profileDir: string): Promise<void> {
  if (process.platform !== "linux") {
    return;
  }

  const lockPath = path.join(profileDir, "SingletonLock");
  const target = await fs.readlink(lockPath).catch(() => undefined);
  const lock = parseChromiumSingletonLock(target);
  if (!lock) {
    return;
  }

  const currentHost = os.hostname();
  const sameHost = lock.host === currentHost;
  if (sameHost && isPidAlive(lock.pid)) {
    return;
  }
  if (!sameHost && !(await isContainerRuntime())) {
    return;
  }

  await Promise.all(
    ["SingletonLock", "SingletonSocket", "SingletonCookie"].map((name) =>
      fs.rm(path.join(profileDir, name), { force: true }),
    ),
  );
}

function parseChromiumSingletonLock(target: string | undefined): { host: string; pid: number } | undefined {
  if (!target) {
    return undefined;
  }
  const fileName = path.basename(target);
  const separator = fileName.lastIndexOf("-");
  if (separator <= 0 || separator === fileName.length - 1) {
    return undefined;
  }
  const pid = Number.parseInt(fileName.slice(separator + 1), 10);
  if (!Number.isInteger(pid) || pid <= 0) {
    return undefined;
  }
  return { host: fileName.slice(0, separator), pid };
}

function isPidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

async function isContainerRuntime(): Promise<boolean> {
  return await fs.stat("/.dockerenv").then(
    () => true,
    () => false,
  );
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

function normalizeLoginUrl(value: string): string {
  const trimmed = value.trim();
  const withProtocol = /^https?:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;
  const url = new URL(withProtocol);
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("login URL must be http or https");
  }
  return url.href;
}

async function assertSafeBrowserUrl(value: string, blockedHosts?: Set<string>): Promise<void> {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    return;
  }
  const host = url.hostname.toLowerCase();
  if (!host || host === "localhost" || host.endsWith(".localhost")) {
    throw new Error("browser URL resolves to blocked host");
  }
  if (blockedHosts?.has(host)) {
    throw new Error("browser URL resolves to blocked host");
  }
  if (isBlockedIpLiteral(host)) {
    blockedHosts?.add(host);
    throw new Error("browser URL resolves to blocked address");
  }
  const records = await dns.lookup(host, { all: true, verbatim: true }).catch((error) => {
    throw new Error(`browser DNS resolution failed: ${error instanceof Error ? error.message : String(error)}`);
  });
  if (records.length === 0 || records.some((record) => isBlockedIpLiteral(record.address))) {
    blockedHosts?.add(host);
    throw new Error("browser URL resolves to blocked address");
  }
}

function isBlockedIpLiteral(value: string): boolean {
  const normalized = value.replace(/^\[|\]$/g, "");
  if (isBlockedIpv4(normalized)) {
    return true;
  }
  return isBlockedIpv6(normalized);
}

function isBlockedIpv4(value: string): boolean {
  const parts = value.split(".");
  if (parts.length !== 4) {
    return false;
  }
  const octets = parts.map((part) => Number.parseInt(part, 10));
  if (octets.some((part) => !Number.isInteger(part) || part < 0 || part > 255)) {
    return false;
  }
  const [a, b, c, d] = octets as [number, number, number, number];
  return (
    a === 0 ||
    a === 10 ||
    a === 127 ||
    a >= 224 ||
    (a === 100 && b >= 64 && b <= 127) ||
    (a === 169 && b === 254) ||
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 168) ||
    (a === 192 && b === 0 && c === 0) ||
    (a === 192 && b === 0 && c === 2) ||
    (a === 198 && (b === 18 || b === 19)) ||
    (a === 198 && b === 51 && c === 100) ||
    (a === 203 && b === 0 && c === 113) ||
    (a === 255 && b === 255 && c === 255 && d === 255)
  );
}

function isBlockedIpv6(value: string): boolean {
  const lowered = value.toLowerCase();
  if (!lowered.includes(":")) {
    return false;
  }
  return (
    lowered === "::" ||
    lowered === "::1" ||
    lowered.startsWith("fc") ||
    lowered.startsWith("fd") ||
    lowered.startsWith("fe80:") ||
    lowered.startsWith("ff") ||
    lowered.startsWith("2001:db8:") ||
    lowered.startsWith("::ffff:127.") ||
    lowered.startsWith("::ffff:10.") ||
    lowered.startsWith("::ffff:192.168.")
  );
}

/// Normalize a URL for "is this the page itself" comparison: drop the fragment
/// and any trailing slash so the page URL and a candidate that echoes it match.
function normalizeUrlForCompare(value: string): string {
  try {
    const url = new URL(value);
    url.hash = "";
    return url.toString().replace(/\/$/, "");
  } catch {
    return value.replace(/\/$/, "");
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
