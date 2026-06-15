import path from "node:path";

export interface ViewportSize {
  width: number;
  height: number;
}

export interface RuntimeConfig {
  host: string;
  port: number;
  profileRoot: string;
  internalToken?: string;
  defaultProfileId: string;
  defaultTimeoutMs: number;
  maxTimeoutMs: number;
  defaultMaxEvents: number;
  defaultMaxCandidates: number;
  headed: boolean;
  browserChannel?: string;
  userAgent?: string;
  locale: string;
  timezoneId: string;
  viewport: ViewportSize;
  loginViewport: ViewportSize;
}

const BROWSER_CHANNELS = new Set([
  "chromium",
  "chrome",
  "chrome-beta",
  "chrome-dev",
  "chrome-canary",
  "msedge",
  "msedge-beta",
  "msedge-dev",
  "msedge-canary",
]);

function numberEnv(name: string, defaultValue: number): number {
  const value = process.env[name];
  if (!value) {
    return defaultValue;
  }

  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : defaultValue;
}

function optionalStringEnv(name: string): string | undefined {
  const value = process.env[name]?.trim();
  return value ? value : undefined;
}

function browserChannelEnv(): string | undefined {
  const value = optionalStringEnv("RK_BROWSER_CHANNEL") ?? "chromium";
  if (["bundled", "default", "none"].includes(value.toLowerCase())) {
    return undefined;
  }
  if (!BROWSER_CHANNELS.has(value)) {
    throw new Error(`invalid RK_BROWSER_CHANNEL: ${value}`);
  }
  return value;
}

function viewportEnv(name: string, defaultValue: ViewportSize): ViewportSize {
  const value = optionalStringEnv(name);
  if (!value) {
    return defaultValue;
  }
  const match = value.match(/^(\d{3,4})x(\d{3,4})$/i);
  if (!match) {
    throw new Error(`invalid ${name}: expected WIDTHxHEIGHT, got ${value}`);
  }
  const width = Number(match[1]);
  const height = Number(match[2]);
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 640 || height < 360 || width > 3840 || height > 2160) {
    throw new Error(`invalid ${name}: viewport must be between 640x360 and 3840x2160`);
  }
  return { width, height };
}

export function loadConfig(): RuntimeConfig {
  const profileRoot = process.env.RK_BROWSER_PROFILE_ROOT
    ? path.resolve(process.env.RK_BROWSER_PROFILE_ROOT)
    : path.resolve("storage", "browser-profiles");

  return {
    host: process.env.RK_BROWSER_HOST ?? "127.0.0.1",
    port: numberEnv("RK_BROWSER_PORT", 8791),
    profileRoot,
    internalToken: process.env.RK_BROWSER_INTERNAL_TOKEN || undefined,
    defaultProfileId: process.env.RK_BROWSER_DEFAULT_PROFILE ?? "admin_default",
    defaultTimeoutMs: numberEnv("RK_BROWSER_TIMEOUT_MS", 45_000),
    maxTimeoutMs: numberEnv("RK_BROWSER_MAX_TIMEOUT_MS", 120_000),
    defaultMaxEvents: numberEnv("RK_BROWSER_MAX_EVENTS", 500),
    defaultMaxCandidates: numberEnv("RK_BROWSER_MAX_CANDIDATES", 50),
    headed: process.env.RK_BROWSER_HEADED === "1",
    browserChannel: browserChannelEnv(),
    userAgent: optionalStringEnv("RK_BROWSER_USER_AGENT"),
    locale: optionalStringEnv("RK_BROWSER_LOCALE") ?? "zh-CN",
    timezoneId: optionalStringEnv("RK_BROWSER_TIMEZONE") ?? "Asia/Shanghai",
    viewport: viewportEnv("RK_BROWSER_VIEWPORT", { width: 1366, height: 768 }),
    loginViewport: viewportEnv("RK_BROWSER_LOGIN_VIEWPORT", { width: 1280, height: 720 }),
  };
}
