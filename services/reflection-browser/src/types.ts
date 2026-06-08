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
}

export interface ProbeResponse {
  finalUrl: string;
  title?: string;
  platformHint?: string;
  candidates: BrowserCandidate[];
  warnings: string[];
  eventCount: number;
  timedOut: boolean;
}

export interface HeadersForUrlRequest {
  url: string;
  profileId?: string;
  referer?: string;
}

export interface HeadersForUrlResponse {
  headers: Record<string, string>;
}
