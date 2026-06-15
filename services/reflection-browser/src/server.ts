import cors from "@fastify/cors";
import Fastify from "fastify";
import { z } from "zod";
import { loadConfig } from "./config.js";
import { BrowserProbeService } from "./probe.js";

const probeSchema = z.object({
  url: z.string().url(),
  profileId: z.string().optional(),
  platformHint: z.string().optional(),
  outputs: z.array(z.string()).optional(),
  timeoutMs: z.number().int().positive().optional(),
  maxEvents: z.number().int().positive().optional(),
  maxCandidates: z.number().int().positive().optional(),
  headed: z.boolean().optional(),
});

const cookiesSchema = z.object({
  cookies: z.array(z.record(z.unknown())),
});

const loginSessionStartSchema = z.object({
  url: z.string().min(1),
});

const loginClickSchema = z.object({
  x: z.number().finite(),
  y: z.number().finite(),
  button: z.enum(["left", "right", "middle"]).optional(),
  clickCount: z.number().int().min(1).max(3).optional(),
});

const loginMoveSchema = z.object({
  x: z.number().finite(),
  y: z.number().finite(),
});

const loginMouseButtonSchema = z.object({
  x: z.number().finite(),
  y: z.number().finite(),
  button: z.enum(["left", "right", "middle"]).optional(),
});

const loginTypeSchema = z.object({
  text: z.string().max(2_000),
});

const loginInsertTextSchema = z.object({
  text: z.string().max(10_000),
});

const loginPressSchema = z.object({
  key: z.string().min(1).max(64),
});

const loginNavigateSchema = z.object({
  url: z.string().min(1),
});

const loginWheelSchema = z.object({
  deltaX: z.number().finite().optional(),
  deltaY: z.number().finite().optional(),
  x: z.number().finite().optional(),
  y: z.number().finite().optional(),
});

const loginResizeSchema = z.object({
  width: z.number().int().min(640).max(2560),
  height: z.number().int().min(360).max(1600),
});

const headersSchema = z.object({
  url: z.string().url(),
  profileId: z.string().optional(),
  referer: z.string().url().optional(),
});

const config = loadConfig();
if (!config.internalToken && !isLoopbackHost(config.host)) {
  throw new Error("RK_BROWSER_INTERNAL_TOKEN is required when RK_BROWSER_HOST is not loopback");
}
const probeService = new BrowserProbeService(config);
const app = Fastify({
  logger: true,
});

await app.register(cors, {
  origin: true,
});

app.addHook("preHandler", async (request, reply) => {
  if (!config.internalToken || request.url === "/health") {
    return;
  }
  const supplied = request.headers["x-reflection-browser-token"];
  if (supplied !== config.internalToken) {
    return reply.status(401).send({ error: "unauthorized browser sidecar request" });
  }
});

app.get("/health", async () => ({
  ok: true,
  service: "reflection-browser",
  profileRoot: config.profileRoot,
  defaultProfileId: config.defaultProfileId,
}));

app.post("/probe", async (request, reply) => {
  const parsed = probeSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.probe(parsed.data);
});

app.post("/profiles/:profileId/cookies/import", async (request, reply) => {
  const params = z.object({ profileId: z.string() }).parse(request.params);
  const parsed = cookiesSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.importCookies(params.profileId, parsed.data.cookies);
});

app.post("/profiles/:profileId/login-sessions", async (request, reply) => {
  const params = z.object({ profileId: z.string() }).parse(request.params);
  const parsed = loginSessionStartSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.startLoginSession(params.profileId, parsed.data.url);
});

app.get("/login-sessions/:sessionId/snapshot", async (request) => {
  const params = z.object({ sessionId: z.string() }).parse(request.params);
  return probeService.snapshotLoginSession(params.sessionId);
});

app.post("/login-sessions/:sessionId/click", async (request, reply) => {
  const params = z.object({ sessionId: z.string() }).parse(request.params);
  const parsed = loginClickSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.loginClick(
    params.sessionId,
    parsed.data.x,
    parsed.data.y,
    parsed.data.button ?? "left",
    parsed.data.clickCount ?? 1,
  );
});

app.post("/login-sessions/:sessionId/move", async (request, reply) => {
  const params = z.object({ sessionId: z.string() }).parse(request.params);
  const parsed = loginMoveSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.loginMove(params.sessionId, parsed.data.x, parsed.data.y);
});

app.post("/login-sessions/:sessionId/mouse-down", async (request, reply) => {
  const params = z.object({ sessionId: z.string() }).parse(request.params);
  const parsed = loginMouseButtonSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.loginMouseDown(
    params.sessionId,
    parsed.data.x,
    parsed.data.y,
    parsed.data.button ?? "left",
  );
});

app.post("/login-sessions/:sessionId/mouse-up", async (request, reply) => {
  const params = z.object({ sessionId: z.string() }).parse(request.params);
  const parsed = loginMouseButtonSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.loginMouseUp(
    params.sessionId,
    parsed.data.x,
    parsed.data.y,
    parsed.data.button ?? "left",
  );
});

app.post("/login-sessions/:sessionId/type", async (request, reply) => {
  const params = z.object({ sessionId: z.string() }).parse(request.params);
  const parsed = loginTypeSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.loginType(params.sessionId, parsed.data.text);
});

app.post("/login-sessions/:sessionId/insert-text", async (request, reply) => {
  const params = z.object({ sessionId: z.string() }).parse(request.params);
  const parsed = loginInsertTextSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.loginInsertText(params.sessionId, parsed.data.text);
});

app.post("/login-sessions/:sessionId/press", async (request, reply) => {
  const params = z.object({ sessionId: z.string() }).parse(request.params);
  const parsed = loginPressSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.loginPress(params.sessionId, parsed.data.key);
});

app.post("/login-sessions/:sessionId/navigate", async (request, reply) => {
  const params = z.object({ sessionId: z.string() }).parse(request.params);
  const parsed = loginNavigateSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.loginNavigate(params.sessionId, parsed.data.url);
});

app.post("/login-sessions/:sessionId/wheel", async (request, reply) => {
  const params = z.object({ sessionId: z.string() }).parse(request.params);
  const parsed = loginWheelSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.loginWheel(
    params.sessionId,
    parsed.data.deltaX ?? 0,
    parsed.data.deltaY ?? 0,
    parsed.data.x,
    parsed.data.y,
  );
});

app.post("/login-sessions/:sessionId/resize", async (request, reply) => {
  const params = z.object({ sessionId: z.string() }).parse(request.params);
  const parsed = loginResizeSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.loginResize(params.sessionId, parsed.data.width, parsed.data.height);
});

app.post("/login-sessions/:sessionId/close", async (request) => {
  const params = z.object({ sessionId: z.string() }).parse(request.params);
  return probeService.loginClose(params.sessionId);
});

app.post("/profiles/:profileId/headers-for-url", async (request, reply) => {
  const params = z.object({ profileId: z.string() }).parse(request.params);
  const parsed = headersSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.headersForUrl(params.profileId, parsed.data.url, parsed.data.referer);
});

app.post("/profiles/:profileId/cookies-for-url", async (request, reply) => {
  const params = z.object({ profileId: z.string() }).parse(request.params);
  const parsed = headersSchema.pick({ url: true }).safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.cookiesForUrl(params.profileId, parsed.data.url);
});

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.on(signal, async () => {
    await probeService.close();
    await app.close();
    process.exit(0);
  });
}

await app.listen({ host: config.host, port: config.port });

function isLoopbackHost(host: string): boolean {
  const normalized = host.trim().toLowerCase();
  return normalized === "localhost" || normalized === "127.0.0.1" || normalized === "::1";
}
