import { createRequire } from "node:module";
import { createInterface } from "node:readline/promises";
import { stdin as input, stdout as output } from "node:process";
import path from "node:path";

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.RK_LOGIN_PLAYWRIGHT);

const platformUrls = {
  bilibili: "https://www.bilibili.com/",
  youtube: "https://www.youtube.com/",
  douyin: "https://www.douyin.com/",
  kuaishou: "https://www.kuaishou.com/",
  pornhub: "https://www.pornhub.com/",
};

const baseUrl = process.env.RK_LOGIN_BASE_URL?.replace(/\/+$/, "");
const apiKey = process.env.RK_LOGIN_API_KEY;
const loginToken = process.env.RK_LOGIN_TOKEN;
const profileId = process.env.RK_LOGIN_PROFILE_ID;
const platform = process.env.RK_LOGIN_PLATFORM;
const userDataDir = process.env.RK_LOGIN_USER_DATA_DIR;
const dryRun = process.env.RK_LOGIN_DRY_RUN === "1";

if (!baseUrl || !profileId || !platform || !userDataDir) {
  throw new Error("Missing RK_LOGIN_* environment variables");
}
if (!platformUrls[platform]) {
  throw new Error(`Unsupported platform: ${platform}`);
}

if (dryRun) {
  console.log(JSON.stringify({
    ok: true,
    baseUrl,
    profileId,
    platform,
    userDataDir: path.normalize(userDataDir),
    authMode: loginToken ? "login-token" : apiKey ? "api-key" : "none",
  }, null, 2));
  process.exit(0);
}

if (!apiKey && !loginToken) {
  throw new Error("Missing admin API key or login token");
}

const context = await chromium.launchPersistentContext(userDataDir, {
  headless: false,
  viewport: { width: 1366, height: 860 },
  locale: "zh-CN",
});

try {
  const page = context.pages()[0] ?? await context.newPage();
  await page.goto(platformUrls[platform], { waitUntil: "domcontentloaded", timeout: 60000 });
  console.log("");
  console.log(`Opened ${platformUrls[platform]}`);
  console.log("Complete login in the opened browser, then return to this PowerShell window.");
  const rl = createInterface({ input, output });
  await rl.question("Press Enter after login to upload cookies...");
  rl.close();

  const cookies = await context.cookies();
  if (!cookies.length) {
    throw new Error("Browser profile has no cookies to upload");
  }

  const endpoint = loginToken
    ? `${baseUrl}/api/browser-login-tokens/cookies/import`
    : `${baseUrl}/api/admin/browser-profiles/${encodeURIComponent(profileId)}/cookies/import`;
  const headers = {
    "content-type": "application/json",
  };
  if (apiKey) {
    headers["x-api-key"] = apiKey;
  }

  const response = await fetch(endpoint, {
    method: "POST",
    headers,
    body: JSON.stringify({ profile_id: profileId, login_token: loginToken, cookies }),
  });

  const text = await response.text();
  if (!response.ok) {
    throw new Error(`Server import failed HTTP ${response.status}: ${text.slice(0, 300)}`);
  }

  let parsed = {};
  try {
    parsed = JSON.parse(text);
  } catch {
    parsed = { response: text };
  }
  console.log(JSON.stringify({
    ok: true,
    profileId,
    platform,
    cookieCount: cookies.length,
    server: parsed,
  }, null, 2));
} finally {
  await context.close();
}
