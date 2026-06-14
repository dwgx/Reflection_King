import path from "node:path";

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
}

function numberEnv(name: string, defaultValue: number): number {
  const value = process.env[name];
  if (!value) {
    return defaultValue;
  }

  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : defaultValue;
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
  };
}
