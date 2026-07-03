export type DiscoveryMode = "direct" | "external" | "browser" | "auto";
export type PlatformHint =
  | "auto"
  | "bilibili"
  | "youtube"
  | "soundcloud"
  | "ximalaya"
  | "douyin"
  | "kuaishou"
  | "pornhub"
  | "acfun"
  | "iqiyi"
  | "youku"
  | "tiktok"
  | "vimeo"
  | "weibo"
  | "dailymotion"
  | "rumble"
  | "peertube"
  | "archive_org"
  | "wayback"
  | "archive_it"
  | "perma_cc"
  | "archive_today"
  | "ghostarchive"
  | "webcitation"
  | "memento"
  | "wikimedia"
  | "twitch"
  | "twitter"
  | "reddit"
  | "instagram"
  | "facebook"
  | "pinterest"
  | "imgur"
  | "flickr"
  | "bandcamp"
  | "mixcloud"
  | "niconico"
  | "fc2"
  | "spotify"
  | "live"
  | "generic";
export type OutputKind = "audio" | "video" | "image" | "page_html";
export type OutputMode = "auto" | "video" | "audio" | "image" | "page_html";
export type ViewMode = "console" | "admin" | "help";
export type BrowserMouseButton = "left" | "right" | "middle";

export interface Health {
  ok: boolean;
  service: string;
  version: string;
  public_base_url: string;
  storage_dir: string;
  database_path: string;
  ffmpeg_path: string;
  max_download_bytes: number;
}

export interface Capabilities {
  service: string;
  version: string;
  browser_probe_configured: boolean;
  yt_dlp_configured: boolean;
  external_adapters_configured?: boolean;
  external_tools?: string[];
  ffmpeg_path: string;
  yt_dlp_path: string | null;
  you_get_path?: string | null;
  lux_path?: string | null;
  streamlink_path?: string | null;
  public_base_url: string;
  max_download_bytes: number;
  max_concurrent_jobs: number;
  browser_probe_timeout_seconds: number;
  yt_dlp_timeout_seconds: number;
  yt_dlp_max_json_bytes: number;
  download_timeout_seconds: number;
  job_ttl_hours?: number;
  supported_discovery: DiscoveryMode[];
  supported_platform_hints: PlatformHint[];
  supported_outputs: OutputKind[];
  auth?: {
    role: "admin" | "user";
    label: string;
    allow_browser_probe: boolean;
    allow_ytdlp: boolean;
    allow_external_adapters?: boolean;
    allow_login_profile?: boolean;
    max_download_bytes?: number | null;
  };
}

export interface RuntimeSettingsView {
  public_base_url: string;
  max_download_bytes: number;
  max_concurrent_jobs: number;
  download_timeout_seconds: number;
  browser_probe_timeout_seconds: number;
  yt_dlp_timeout_seconds: number;
  yt_dlp_max_json_bytes: number;
  job_ttl_hours: number;
  page_archive_max_resources?: number;
  page_archive_max_resource_bytes?: number;
  page_archive_max_total_bytes?: number;
  page_archive_capture_cdp_enabled?: boolean;
  page_archive_save_mhtml_enabled?: boolean;
  page_archive_save_har_enabled?: boolean;
  page_archive_save_warc_enabled?: boolean;
  page_archive_cdp_body_max_bytes?: number;
  page_archive_cdp_body_total_bytes?: number;
  cache_cleanup_min_age_hours?: number;
  ffmpeg_path: string;
  browser_probe_url: string | null;
  yt_dlp_path: string | null;
  you_get_path: string | null;
  lux_path: string | null;
  streamlink_path: string | null;
  external_probe_timeout_seconds: number;
}

export interface RuntimeSettingsForm {
  public_base_url: string;
  max_download_mb: string;
  download_timeout_seconds: string;
  yt_dlp_timeout_seconds: string;
  yt_dlp_max_json_mb: string;
  job_ttl_hours: string;
  page_archive_max_resources: string;
  page_archive_max_resource_mb: string;
  page_archive_max_total_mb: string;
  page_archive_capture_cdp_enabled: boolean;
  page_archive_save_mhtml_enabled: boolean;
  page_archive_save_har_enabled: boolean;
  page_archive_save_warc_enabled: boolean;
  page_archive_cdp_body_max_mb: string;
  page_archive_cdp_body_total_mb: string;
  cache_cleanup_min_age_hours: string;
}

export interface JobView {
  id: string;
  status: string;
  source_url: string;
  original_source_url?: string | null;
  bitrate: string;
  created_at: string;
  updated_at: string;
  status_url: string;
  media_url: string | null;
  artifacts_url: string;
  candidates_url: string;
  error: string | null;
  discovery: DiscoveryMode;
  platform_hint: PlatformHint;
  outputs: OutputKind[];
  profile_id: string;
  auth_mode: string;
  issue_kind?: "none" | "failed" | "needs_profile" | "unsupported" | "too_large" | "timeout" | "policy_blocked";
  issue_label?: string;
  issue_detail?: string | null;
  profile_action_url?: string | null;
}

export interface Candidate {
  id: string;
  job_id: string;
  url: string;
  kind: string;
  extractor: string;
  method: string;
  status?: number;
  content_type?: string;
  content_length?: number;
  resource_type?: string;
  initiator_url?: string;
  quality_label?: string;
  score: number;
  requires_authorization: boolean;
  platform?: PlatformHint | null;
  route?: string | null;
  extractor_confidence?: number | null;
  protection?: string | null;
  requires_profile?: boolean;
  ttl_hint_seconds?: number | null;
  ad_risk?: boolean;
  evidence_count?: number;
  paired_candidate_ids?: string[];
  failure_reason?: string | null;
  validation_state?: string | null;
  metadata_json?: unknown;
  selected?: boolean;
  selection_reason?: string | null;
  validation_status?: string | null;
}

export interface CandidateMetadata {
  acodec?: string;
  vcodec?: string;
  candidate?: {
    acodec?: string;
    vcodec?: string;
    higherQualityRequiresProfile?: boolean;
    highestAdvertisedHeight?: number;
    acceptDescription?: string[];
  };
}

export interface Artifact {
  id: string;
  job_id: string;
  kind: OutputKind;
  media_url: string;
  content_type: string;
  bytes: number;
  created_at: string;
}

export interface ArchiveFileView {
  path: string;
  name: string;
  content_type: string;
  bytes: number;
  media_url: string;
  previewable: boolean;
  modified_at: string | null;
}

export interface ArchiveTreeView {
  job_id: string;
  base_url: string;
  files: ArchiveFileView[];
}

export interface CacheCategoryView {
  name: string;
  path: string;
  bytes: number;
  files: number;
  directories: number;
  cleanup_allowed: boolean;
}

export interface CacheInventoryView {
  storage_root: string;
  total_bytes: number;
  categories: CacheCategoryView[];
}

export interface CacheCleanupEntryView {
  path: string;
  bytes: number;
  reason: string;
  deleted: boolean;
}

export interface CacheCleanupView {
  dry_run: boolean;
  min_age_hours: number;
  total_bytes: number;
  deleted_bytes: number;
  entries: CacheCleanupEntryView[];
}

export interface CreateJobPayload {
  url: string;
  bitrate: string;
  discovery: DiscoveryMode;
  platform_hint: PlatformHint;
  outputs: OutputKind[];
  profile_id: string;
  auth_mode: "auto" | "none" | "profile" | "cookies";
}

export interface UserKeyView {
  id: string;
  label: string;
  key_prefix: string;
  role: "admin" | "user";
  max_download_bytes: number | null;
  allow_browser_probe: boolean;
  allow_ytdlp: boolean;
  allow_external_adapters?: boolean;
  allow_login_profile?: boolean;
  created_at: string;
  revoked_at: string | null;
}

export interface CreatedUserKeyResponse {
  key: string;
  record: UserKeyView;
}

export interface RotatedAdminKeyResponse {
  key: string;
  record: UserKeyView;
}

export interface HiddenJobBatchView {
  id: string;
  actor_key_id: string | null;
  actor_label: string | null;
  hidden_count: number;
  restored_count: number;
  created_at: string;
  restored_at: string | null;
}

export interface ClearJobsResponse {
  batch_id: string | null;
  hidden: number;
  history_deleted: boolean;
}

export interface RestoreJobsResponse {
  batch_id: string | null;
  restored: number;
  history_deleted: boolean;
}

export interface BrowserLoginSessionView {
  id: string;
  profileId: string;
  url: string;
  title?: string;
  createdAt: string;
  lastActiveAt: string;
  expiresAt: string;
}

export interface BrowserLoginSnapshot {
  session: BrowserLoginSessionView;
  image: string;
  url: string;
  title?: string;
  width: number;
  height: number;
}

export interface NotificationItem {
  id: number;
  tone: "info" | "success" | "error" | "warn";
  text: string;
}

export interface FilePreviewState {
  title: string;
  contentType: string;
  bytes: number | null;
  blobUrl: string;
  sourceUrl: string;
}

export interface ConfirmDialogState {
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: () => Promise<void> | void;
}

export type JobIssueKind = "cookie" | "dependency" | "unsupported" | "profile" | "resolver" | "timeout" | "policy" | "error";

export interface JobIssue {
  kind: JobIssueKind;
  label: string;
  tone: "error" | "warn" | "info";
}

export interface JobStats {
  total: number;
  ready: number;
  candidates: number;
  running: number;
  error: number;
  cookie: number;
  dependency: number;
  unsupported: number;
}
