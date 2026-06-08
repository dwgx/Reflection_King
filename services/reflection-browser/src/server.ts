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

const headersSchema = z.object({
  url: z.string().url(),
  profileId: z.string().optional(),
  referer: z.string().url().optional(),
});

const loginSessionSchema = z.object({
  headed: z.boolean().optional(),
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

app.post("/profiles/:profileId/headers-for-url", async (request, reply) => {
  const params = z.object({ profileId: z.string() }).parse(request.params);
  const parsed = headersSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }
  return probeService.headersForUrl(params.profileId, parsed.data.url, parsed.data.referer);
});

app.post("/profiles/:profileId/login-sessions", async (request, reply) => {
  const params = z.object({ profileId: z.string() }).parse(request.params);
  const parsed = loginSessionSchema.safeParse(request.body);
  if (!parsed.success) {
    return reply.status(400).send({ error: parsed.error.flatten() });
  }

  return {
    profileId: params.profileId,
    mode: parsed.data.headed ?? config.headed ? "headed" : "headless",
    message:
      "Persistent profile is ready. Use RK_BROWSER_HEADED=1 on a desktop host, or attach a noVNC wrapper in deployment.",
  };
});

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.on(signal, async () => {
    await probeService.close();
    await app.close();
    process.exit(0);
  });
}

await app.listen({ host: config.host, port: config.port });
