export type CandidateKind = "audio" | "video" | "image" | "manifest" | "html" | "unknown";

export interface ProbeRequest {
  url: string;
  profileId?: string;
  platformHint?: string;
  outputs?: string[];
  timeoutMs?: number;
  maxEvents?: number;
  maxCandidates?: number;
  headed?: boolean;
}

export interface BrowserCandidate {
  url: string;
  kind: CandidateKind;
  method: string;
  status?: number;
  contentType?: string;
  contentLength?: number;
  resourceType?: string;
  initiatorUrl?: string;
  qualityLabel?: string;
  score: number;
  requiresAuthorization: boolean;
  failureReason?: string;
  validationState?: string;
  metadata?: Record<string, unknown>;
}

export interface ProbeResponse {
  finalUrl: string;
  title?: string;
  platformHint?: string;
  candidates: BrowserCandidate[];
  warnings: string[];
  eventCount: number;
  timedOut: boolean;
  userAgent?: string;
  playbackTriggered?: boolean;
  consoleErrors?: string[];
}

export interface HeadersForUrlRequest {
  url: string;
  profileId?: string;
  referer?: string;
}

export interface HeadersForUrlResponse {
  headers: Record<string, string>;
}

export interface CookiesForUrlResponse {
  cookies: BrowserContextCookie[];
}

export interface BrowserContextCookie {
  name: string;
  value: string;
  domain: string;
  path: string;
  expires: number;
  httpOnly: boolean;
  secure: boolean;
  sameSite: "Strict" | "Lax" | "None";
}

export interface LoginSessionStartRequest {
  url: string;
  profileId?: string;
}

export interface LoginSessionView {
  id: string;
  profileId: string;
  url: string;
  title?: string;
  createdAt: string;
  lastActiveAt: string;
  expiresAt: string;
}

export interface LoginSessionSnapshot {
  session: LoginSessionView;
  image: string;
  url: string;
  title?: string;
  width: number;
  height: number;
}
