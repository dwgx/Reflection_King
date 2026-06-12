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
});

const loginTypeSchema = z.object({
  text: z.string().max(2_000),
});

const loginPressSchema = z.object({
  key: z.string().min(1).max(64),
});

const loginNavigateSchema = z.object({
  url: z.string().min(1),
});

const headersSchema = z.object({
  url: z.string().url(),
  profileId: z.string().optional(),
  referer: z.string().url().optional(),
});

const config = loadConfig();
const probeService = new BrowserProbeService(config);
const app = Fastify({
  logger: true,
});

await app.register(cors, {
  origin: true,
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
  return probeService.loginClick(params.sessionId, parsed.data.x, parsed.data.y);
});

app.post("/login-sessions/:sessionId/type", async (request, reply) => {
  const params = z.object({ sessionId: z.string() }).parse(request.params);
  const parsed = loginTypeSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.loginType(params.sessionId, parsed.data.text);
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

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.on(signal, async () => {
    await probeService.close();
    await app.close();
    process.exit(0);
  });
}

await app.listen({ host: config.host, port: config.port });
