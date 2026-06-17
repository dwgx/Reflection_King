import React, { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  Activity,
  AlertTriangle,
  ChevronDown,
  ChevronUp,
  CheckCircle2,
  Clipboard,
  Database,
  ExternalLink,
  FileAudio,
  HelpCircle,
  KeyRound,
  ListRestart,
  Loader2,
  Play,
  RefreshCw,
  Search,
  Settings,
  Shield,
  UserCog,
  MonitorPlay,
  X,
} from "lucide-react";
import "./styles.css";

type DiscoveryMode = "direct" | "external" | "browser" | "auto";
type PlatformHint =
  | "auto"
  | "bilibili"
  | "youtube"
  | "soundcloud"
  | "douyin"
  | "kuaishou"
  | "pornhub"
  | "acfun"
  | "iqiyi"
  | "youku"
  | "tiktok"
  | "vimeo"
  | "live"
  | "generic";
type OutputKind = "audio" | "video" | "image" | "page_html";
type OutputMode = "auto" | "video" | "audio" | "image" | "page_html";
type ViewMode = "console" | "admin" | "help";
type BrowserMouseButton = "left" | "right" | "middle";

interface Health {
  ok: boolean;
  service: string;
  version: string;
  public_base_url: string;
  storage_dir: string;
  database_path: string;
  ffmpeg_path: string;
  max_download_bytes: number;
}

interface Capabilities {
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

interface RuntimeSettingsView {
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

interface RuntimeSettingsForm {
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

interface JobView {
  id: string;
  status: string;
  source_url: string;
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

interface Candidate {
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

interface CandidateMetadata {
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

interface Artifact {
  id: string;
  job_id: string;
  kind: OutputKind;
  media_url: string;
  content_type: string;
  bytes: number;
  created_at: string;
}

interface ArchiveFileView {
  path: string;
  name: string;
  content_type: string;
  bytes: number;
  media_url: string;
  previewable: boolean;
  modified_at: string | null;
}

interface ArchiveTreeView {
  job_id: string;
  base_url: string;
  files: ArchiveFileView[];
}

interface CacheCategoryView {
  name: string;
  path: string;
  bytes: number;
  files: number;
  directories: number;
  cleanup_allowed: boolean;
}

interface CacheInventoryView {
  storage_root: string;
  total_bytes: number;
  categories: CacheCategoryView[];
}

interface CacheCleanupEntryView {
  path: string;
  bytes: number;
  reason: string;
  deleted: boolean;
}

interface CacheCleanupView {
  dry_run: boolean;
  min_age_hours: number;
  total_bytes: number;
  deleted_bytes: number;
  entries: CacheCleanupEntryView[];
}

interface CreateJobPayload {
  url: string;
  bitrate: string;
  discovery: DiscoveryMode;
  platform_hint: PlatformHint;
  outputs: OutputKind[];
  profile_id: string;
  auth_mode: "auto" | "none" | "profile" | "cookies";
}

interface UserKeyView {
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

interface CreatedUserKeyResponse {
  key: string;
  record: UserKeyView;
}

interface RotatedAdminKeyResponse {
  key: string;
  record: UserKeyView;
}

interface HiddenJobBatchView {
  id: string;
  actor_key_id: string | null;
  actor_label: string | null;
  hidden_count: number;
  restored_count: number;
  created_at: string;
  restored_at: string | null;
}

interface ClearJobsResponse {
  batch_id: string | null;
  hidden: number;
  history_deleted: boolean;
}

interface RestoreJobsResponse {
  batch_id: string | null;
  restored: number;
  history_deleted: boolean;
}

interface BrowserLoginSessionView {
  id: string;
  profileId: string;
  url: string;
  title?: string;
  createdAt: string;
  lastActiveAt: string;
  expiresAt: string;
}

interface BrowserLoginSnapshot {
  session: BrowserLoginSessionView;
  image: string;
  url: string;
  title?: string;
  width: number;
  height: number;
}

interface NotificationItem {
  id: number;
  tone: "info" | "success" | "error" | "warn";
  text: string;
}

interface FilePreviewState {
  title: string;
  contentType: string;
  bytes: number | null;
  blobUrl: string;
  sourceUrl: string;
}

interface ConfirmDialogState {
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: () => Promise<void> | void;
}

const OUTPUTS: OutputKind[] = ["audio", "video", "image", "page_html"];
const TERMINAL = new Set(["ready", "error", "candidates_ready", "needs_profile"]);
const PAGE_SIZE_OPTIONS = [3, 5, 10, 20, 50];
const LOGIN_TARGETS = [
  { id: "douyin", label: "抖音", url: "https://www.douyin.com/" },
  { id: "kuaishou", label: "快手", url: "https://www.kuaishou.com/" },
  { id: "bilibili", label: "哔哩哔哩", url: "https://www.bilibili.com/" },
  { id: "youtube", label: "YouTube", url: "https://www.youtube.com/" },
  { id: "tiktok", label: "TikTok", url: "https://www.tiktok.com/" },
  { id: "youku", label: "优酷", url: "https://www.youku.com/" },
  { id: "iqiyi", label: "爱奇艺", url: "https://www.iqiyi.com/" },
  { id: "acfun", label: "AcFun", url: "https://www.acfun.cn/" },
];

const EMPTY_RUNTIME_FORM: RuntimeSettingsForm = {
  public_base_url: "",
  max_download_mb: "",
  download_timeout_seconds: "",
  yt_dlp_timeout_seconds: "",
  yt_dlp_max_json_mb: "",
  job_ttl_hours: "",
  page_archive_max_resources: "",
  page_archive_max_resource_mb: "",
  page_archive_max_total_mb: "",
  page_archive_capture_cdp_enabled: true,
  page_archive_save_mhtml_enabled: true,
  page_archive_save_har_enabled: true,
  page_archive_save_warc_enabled: true,
  page_archive_cdp_body_max_mb: "",
  page_archive_cdp_body_total_mb: "",
  cache_cleanup_min_age_hours: "",
};

function App() {
  const [apiKey, setApiKey] = useState(() => localStorage.getItem("reflection_api_key") ?? "");
  const [health, setHealth] = useState<Health | null>(null);
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null);
  const [jobs, setJobs] = useState<JobView[]>([]);
  const [selectedJobId, setSelectedJobId] = useState<string>("");
  const [selectedJob, setSelectedJob] = useState<JobView | null>(null);
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [artifacts, setArtifacts] = useState<Artifact[]>([]);
  const [archiveTree, setArchiveTree] = useState<ArchiveTreeView | null>(null);
  const [cacheInventory, setCacheInventory] = useState<CacheInventoryView | null>(null);
  const [cacheCleanupPreview, setCacheCleanupPreview] = useState<CacheCleanupView | null>(null);
  const [selectedCandidates, setSelectedCandidates] = useState<Set<string>>(new Set());
  const [showAllCandidates, setShowAllCandidates] = useState(false);
  const [jobPage, setJobPage] = useState(1);
  const [jobPageSize, setJobPageSize] = useState(3);
  const [candidatePage, setCandidatePage] = useState(1);
  const [candidatePageSize, setCandidatePageSize] = useState(3);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string>("空闲");
  const [viewMode, setViewMode] = useState<ViewMode>("console");
  const [notifications, setNotifications] = useState<NotificationItem[]>([]);
  const [confirmDialog, setConfirmDialog] = useState<ConfirmDialogState | null>(null);
  const [filePreview, setFilePreview] = useState<FilePreviewState | null>(null);
  const notificationIdRef = useRef(0);
  const [userKeys, setUserKeys] = useState<UserKeyView[]>([]);
  const [newUserKey, setNewUserKey] = useState("");
  const [newAdminKey, setNewAdminKey] = useState("");
  const [adminKeyForm, setAdminKeyForm] = useState("");
  const [runtimeSettings, setRuntimeSettings] = useState<RuntimeSettingsView | null>(null);
  const [settingsForm, setSettingsForm] = useState<RuntimeSettingsForm>(EMPTY_RUNTIME_FORM);
  const [keyForm, setKeyForm] = useState({
    label: "普通用户",
    key: "",
    max_download_mb: "",
    allow_browser_probe: true,
    allow_ytdlp: true,
    allow_external_adapters: true,
    allow_login_profile: false,
  });
  const [profileId, setProfileId] = useState("admin_default");
  const [cookieJson, setCookieJson] = useState("");
  const [loginUrl, setLoginUrl] = useState("https://www.bilibili.com/");
  const [loginText, setLoginText] = useState("");
  const [loginSnapshot, setLoginSnapshot] = useState<BrowserLoginSnapshot | null>(null);
  const [activeLoginJobId, setActiveLoginJobId] = useState<string | null>(null);
  const [loginZoom, setLoginZoom] = useState("1");
  const lastMouseMoveRef = useRef(0);
  const lastBrowserPointRef = useRef<{ x: number; y: number } | null>(null);
  const loginSnapshotRequestRef = useRef(0);
  const moveInFlightRef = useRef(false);
  const activePointerRef = useRef<{
    pointerId: number;
    button: BrowserMouseButton;
    point: { x: number; y: number };
  } | null>(null);
  const [hiddenBatches, setHiddenBatches] = useState<HiddenJobBatchView[]>([]);
  const [form, setForm] = useState<CreateJobPayload>({
    url: "",
    bitrate: "auto",
    discovery: "auto",
    platform_hint: "auto",
    outputs: ["video", "audio"],
    profile_id: "admin_default",
    auth_mode: "auto",
  });
  const [outputMode, setOutputMode] = useState<OutputMode>("auto");

  const headers = useMemo(() => apiHeaders(apiKey), [apiKey]);
  const isAdmin = capabilities?.auth?.role === "admin";

  useEffect(() => {
    localStorage.setItem("reflection_api_key", apiKey);
  }, [apiKey]);

  useEffect(() => {
    void refreshSystem();
    if (apiKey) {
      void refreshJobs();
    }
  }, [headers]);

  useEffect(() => {
    if (viewMode === "admin" && !isAdmin) {
      setViewMode("console");
    }
  }, [isAdmin, viewMode]);

  useEffect(() => {
    if (viewMode === "admin" && isAdmin) {
      void refreshAdminPanel();
    }
  }, [viewMode, isAdmin, headers]);

  useEffect(() => {
    if (viewMode === "admin" && apiKey) {
      void refreshHiddenBatches();
    }
  }, [viewMode, headers]);

  useEffect(() => {
    if (!selectedJobId) return;
    setSelectedCandidates(new Set());
    setShowAllCandidates(false);
    setCandidatePage(1);
    void loadJob(selectedJobId);
    const timer = window.setInterval(() => {
      void loadJob(selectedJobId, true);
    }, 3000);
    return () => window.clearInterval(timer);
  }, [selectedJobId, headers]);

  const visibleCandidates = useMemo(
    () => candidateDisplayList(candidates, selectedJob, showAllCandidates),
    [candidates, selectedJob, showAllCandidates],
  );
  const selectedJobIsPageArchive = isPageArchiveJob(selectedJob);

  const defaultCandidate = useMemo(
    () => bestCandidate(candidates, selectedJob),
    [candidates, selectedJob],
  );
  const defaultCandidateIds = useMemo(
    () =>
      defaultCandidatesForJob(candidates, selectedJob, defaultCandidate).map(
        (candidate) => candidate.id,
      ),
    [candidates, selectedJob, defaultCandidate],
  );

  const pagedJobs = useMemo(
    () => paginate(jobs, jobPage, jobPageSize),
    [jobs, jobPage, jobPageSize],
  );
  const taskStats = useMemo(() => summarizeJobs(jobs), [jobs]);
  const sourceUrlIssue = useMemo(() => sourceInputIssue(form.url, health?.public_base_url), [form.url, health?.public_base_url]);

  const pagedCandidates = useMemo(
    () => paginate(visibleCandidates, candidatePage, candidatePageSize),
    [visibleCandidates, candidatePage, candidatePageSize],
  );
  const orderedArtifacts = useMemo(
    () => artifacts.slice().sort(compareArtifacts),
    [artifacts],
  );
  const orderedArchiveFiles = useMemo(
    () => archiveTree?.files.slice().sort(compareArchiveFiles) ?? [],
    [archiveTree],
  );

  useEffect(() => {
    setJobPage((page) => clampPage(page, jobs.length, jobPageSize));
  }, [jobs.length, jobPageSize]);

  useEffect(() => {
    setCandidatePage((page) => clampPage(page, visibleCandidates.length, candidatePageSize));
  }, [visibleCandidates.length, candidatePageSize]);

  useEffect(() => {
    const onCopyResult = (event: Event) => {
      const detail = (event as CustomEvent<{ ok: boolean }>).detail;
      notify(detail?.ok ? "已复制到剪贴板" : "复制失败，请手动选择文本", detail?.ok ? "success" : "error");
    };
    window.addEventListener("reflection-copy-result", onCopyResult);
    return () => window.removeEventListener("reflection-copy-result", onCopyResult);
  }, []);

  async function request<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await fetch(path, {
      ...init,
      headers: {
        ...headers,
        ...(init?.headers ?? {}),
      },
    });
    const text = await response.text();
    if (!response.ok) {
      let error = text;
      try {
        error = JSON.parse(text).error ?? text;
      } catch {
        // Keep plain text.
      }
      throw new Error(error || `${response.status} ${response.statusText}`);
    }
    return text ? (JSON.parse(text) as T) : (undefined as T);
  }

  async function requestWithoutAuth<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await fetch(path, init);
    const text = await response.text();
    if (!response.ok) {
      let error = text;
      try {
        error = JSON.parse(text).error ?? text;
      } catch {
        // Keep plain text.
      }
      throw new Error(error || `${response.status} ${response.statusText}`);
    }
    return text ? (JSON.parse(text) as T) : (undefined as T);
  }

  async function openAuthenticatedPreview(url: string, title: string) {
    try {
      const response = await fetch(url, { headers });
      if (!response.ok) {
        const text = await response.text();
        let error = text;
        try {
          error = JSON.parse(text).error ?? text;
        } catch {
          // Keep plain text.
        }
        throw new Error(error || `${response.status} ${response.statusText}`);
      }
      const blob = await response.blob();
      const blobUrl = URL.createObjectURL(blob);
      setFilePreview((current) => {
        if (current) URL.revokeObjectURL(current.blobUrl);
        return {
          title,
          contentType: response.headers.get("content-type") ?? blob.type,
          bytes: Number(response.headers.get("content-length")) || blob.size || null,
          blobUrl,
          sourceUrl: url,
        };
      });
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  }

  async function openArchiveFile(file: ArchiveFileView) {
    await openAuthenticatedPreview(file.media_url, archiveFileLabel(file));
  }

  function closeFilePreview() {
    setFilePreview((current) => {
      if (current) URL.revokeObjectURL(current.blobUrl);
      return null;
    });
  }

  function notify(text: string, tone: NotificationItem["tone"] = "info") {
    setMessage(text);
    const id = ++notificationIdRef.current;
    setNotifications((items) => [...items.slice(-3), { id, tone, text }]);
    window.setTimeout(() => {
      setNotifications((items) => items.filter((item) => item.id !== id));
    }, tone === "error" ? 7000 : 4200);
  }

  function askConfirm(dialog: ConfirmDialogState) {
    setConfirmDialog(dialog);
  }

  async function refreshSystem() {
    try {
      const healthData = await requestWithoutAuth<Health>("/api/health");
      setHealth(healthData);
    } catch (error) {
      notify(errorMessage(error), "error");
    }

    try {
      const capabilityData = await request<Capabilities>("/api/capabilities");
      setCapabilities(capabilityData);
    } catch (error) {
      setCapabilities(null);
      if (apiKey) {
        notify(errorMessage(error), "error");
      } else {
        setMessage("系统状态已加载；填写管理密钥后可查看解析能力和任务。");
      }
    }
  }

  async function refreshJobs() {
    try {
      const data = await request<JobView[]>("/api/jobs?limit=100");
      setJobs(data);
      setJobPage((page) => clampPage(page, data.length, jobPageSize));
      if (selectedJobId && !data.some((job) => job.id === selectedJobId)) {
        clearSelection();
      } else if (!selectedJobId && data[0]) {
        setSelectedJobId(data[0].id);
      }
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  }

  async function refreshHiddenBatches() {
    try {
      const data = await request<HiddenJobBatchView[]>("/api/jobs/hidden-batches?limit=100");
      setHiddenBatches(data);
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  }

  async function refreshRuntimeSettings() {
    try {
      const data = await request<RuntimeSettingsView>("/api/admin/settings");
      setRuntimeSettings(data);
      setSettingsForm(runtimeSettingsToForm(data));
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  }

  async function refreshCacheInventory() {
    try {
      const data = await request<CacheInventoryView>("/api/admin/cache");
      setCacheInventory(data);
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  }

  async function refreshAdminPanel() {
    if (!isAdmin) return;
    await Promise.all([
      refreshRuntimeSettings(),
      refreshCacheInventory(),
      refreshUserKeys(),
      refreshHiddenBatches(),
    ]);
  }

  async function saveRuntimeSettings(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    try {
      const payload = {
        public_base_url: settingsForm.public_base_url.trim(),
        max_download_mb: parsePositiveInt(settingsForm.max_download_mb, "单任务最大大小"),
        download_timeout_seconds: parsePositiveInt(settingsForm.download_timeout_seconds, "下载超时"),
        yt_dlp_timeout_seconds: parsePositiveInt(settingsForm.yt_dlp_timeout_seconds, "yt-dlp 超时"),
        yt_dlp_max_json_mb: parsePositiveInt(settingsForm.yt_dlp_max_json_mb, "yt-dlp JSON 上限"),
        job_ttl_hours: parsePositiveInt(settingsForm.job_ttl_hours, "任务保留小时"),
        page_archive_max_resources: parsePositiveInt(settingsForm.page_archive_max_resources, "网页资源数上限"),
        page_archive_max_resource_mb: parsePositiveInt(settingsForm.page_archive_max_resource_mb, "网页单资源上限"),
        page_archive_max_total_mb: parsePositiveInt(settingsForm.page_archive_max_total_mb, "网页资源包总上限"),
        page_archive_capture_cdp_enabled: settingsForm.page_archive_capture_cdp_enabled,
        page_archive_save_mhtml_enabled: settingsForm.page_archive_save_mhtml_enabled,
        page_archive_save_har_enabled: settingsForm.page_archive_save_har_enabled,
        page_archive_save_warc_enabled: settingsForm.page_archive_save_warc_enabled,
        page_archive_cdp_body_max_mb: parsePositiveInt(settingsForm.page_archive_cdp_body_max_mb, "CDP 单响应上限"),
        page_archive_cdp_body_total_mb: parsePositiveInt(settingsForm.page_archive_cdp_body_total_mb, "CDP 响应总上限"),
        cache_cleanup_min_age_hours: parsePositiveInt(settingsForm.cache_cleanup_min_age_hours, "缓存清理最小小时"),
      };
      const data = await request<RuntimeSettingsView>("/api/admin/settings", {
        method: "PATCH",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      });
      setRuntimeSettings(data);
      setSettingsForm(runtimeSettingsToForm(data));
      await refreshSystem();
      notify("高级运行设置已保存", "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function loadJob(id: string, quiet = false) {
    try {
      const job = await request<JobView>(`/api/jobs/${id}`);
      setSelectedJob(job);
      const [candidateData, artifactData] = await Promise.all([
        request<Candidate[]>(`/api/jobs/${id}/candidates`).catch(() => []),
        request<Artifact[]>(`/api/jobs/${id}/artifacts`).catch(() => []),
      ]);
      setCandidates(candidateData);
      setArtifacts(artifactData);
      if (job.outputs.includes("page_html")) {
        const tree = await request<ArchiveTreeView>(`/api/jobs/${id}/archive/tree`).catch(() => null);
        setArchiveTree(tree);
      } else {
        setArchiveTree(null);
      }
      if (!quiet) setMessage(`已载入任务 ${id}`);
      if (TERMINAL.has(job.status)) {
        await refreshJobs();
      }
    } catch (error) {
      if (!quiet) notify(errorMessage(error), "error");
    }
  }

  async function createJob(event: React.FormEvent) {
    event.preventDefault();
    if (sourceUrlIssue) {
      notify(sourceUrlIssue, "warn");
      return;
    }
    setBusy(true);
    setMessage("正在创建任务...");
    try {
      const payload: CreateJobPayload = {
        ...form,
        url: form.url.trim(),
        outputs: outputsForMode(outputMode),
        discovery: outputMode === "page_html" ? "browser" : form.discovery,
        auth_mode: outputMode === "page_html" ? "none" : form.auth_mode,
      };
      const job = await request<JobView>("/api/jobs", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      });
      setSelectedJobId(job.id);
      setSelectedCandidates(new Set());
      notify(`已创建任务 ${job.id}`, "success");
      await refreshJobs();
      await loadJob(job.id);
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  function clearForm() {
    setForm({ ...form, url: "" });
    notify("已清空来源 URL", "info");
  }

  async function clearVisibleJobs() {
    if (!jobs.length) {
      notify("任务列表已经为空", "info");
      return;
    }
    askConfirm({
      title: "清空任务列表",
      message: "清空只会隐藏当前密钥可见的任务列表，不会删除数据库历史。可以恢复上一批，也可以在隐藏历史页恢复更早批次。",
      confirmLabel: "清空列表",
      danger: true,
      onConfirm: async () => {
        setBusy(true);
        try {
          const response = await request<ClearJobsResponse>("/api/jobs/clear", {
            method: "POST",
          });
          setJobs([]);
          clearSelection();
          setJobPage(1);
          await refreshHiddenBatches();
          notify(`已隐藏 ${response.hidden} 个任务；数据库历史未删除`, "success");
        } catch (error) {
          notify(errorMessage(error), "error");
        } finally {
          setBusy(false);
        }
      },
    });
  }

  async function restoreHiddenJobs() {
    setBusy(true);
    try {
      const response = await request<RestoreJobsResponse>("/api/jobs/restore", {
        method: "POST",
      });
      await refreshJobs();
      await refreshHiddenBatches();
      notify(response.restored ? `已恢复上一批 ${response.restored} 个隐藏任务` : "没有可恢复的隐藏批次", response.restored ? "success" : "info");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function restoreHiddenBatch(id: string) {
    setBusy(true);
    try {
      const response = await request<RestoreJobsResponse>(`/api/jobs/hidden-batches/${id}/restore`, {
        method: "POST",
      });
      await refreshJobs();
      await refreshHiddenBatches();
      notify(response.restored ? `已恢复 ${response.restored} 个隐藏任务` : "这个批次已经恢复或不可见", response.restored ? "success" : "info");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  function clearSelection() {
    setSelectedJobId("");
    setSelectedJob(null);
    setCandidates([]);
    setArtifacts([]);
    setArchiveTree(null);
    setSelectedCandidates(new Set());
  }

  async function previewCacheCleanup() {
    setBusy(true);
    try {
      const data = await request<CacheCleanupView>("/api/admin/cache/cleanup-preview", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          min_age_hours: parsePositiveInt(settingsForm.cache_cleanup_min_age_hours, "缓存清理最小小时"),
        }),
      });
      setCacheCleanupPreview(data);
      notify(`预计可清理 ${formatBytes(data.total_bytes)}`, "info");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function runCacheCleanup() {
    const minAge = parsePositiveInt(settingsForm.cache_cleanup_min_age_hours, "缓存清理最小小时");
    askConfirm({
      title: "清理缓存",
      message: `将清理超过 ${minAge} 小时的临时任务目录和公开产物目录。浏览器 Profile 和 Cookie 不会被删除。`,
      confirmLabel: "清理缓存",
      danger: true,
      onConfirm: async () => {
        setBusy(true);
        try {
          const data = await request<CacheCleanupView>("/api/admin/cache/cleanup", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ confirm: true, min_age_hours: minAge }),
          });
          setCacheCleanupPreview(data);
          await refreshCacheInventory();
          notify(`已清理 ${formatBytes(data.deleted_bytes)}`, "success");
        } catch (error) {
          notify(errorMessage(error), "error");
        } finally {
          setBusy(false);
        }
      },
    });
  }

  async function selectCandidates() {
    if (!selectedJob) return;
    const candidateIds = selectedCandidates.size ? Array.from(selectedCandidates) : defaultCandidateIds;
    if (candidateIds.length === 0) return;
    setBusy(true);
    setMessage("正在开始转码资源...");
    try {
      await request<JobView>(`/api/jobs/${selectedJob.id}/select-candidates`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ candidate_ids: candidateIds }),
      });
      setSelectedCandidates(new Set());
      await loadJob(selectedJob.id);
      notify("已提交转码任务", "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  function setOutputModeAndPayload(mode: OutputMode) {
    setOutputMode(mode);
    setForm({
      ...form,
      outputs: outputsForMode(mode),
      discovery: mode === "page_html" ? "browser" : form.discovery,
      auth_mode: mode === "page_html" ? "none" : form.auth_mode === "none" ? "auto" : form.auth_mode,
    });
  }

  function toggleCandidate(id: string) {
    const candidate = candidates.find((item) => item.id === id);
    if (candidate && !isUsableCandidate(candidate)) {
      notify("该资源已标记为不可转换，请选择其他可用资源", "warn");
      return;
    }
    const next = new Set(selectedCandidates);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelectedCandidates(next);
  }

  async function refreshUserKeys() {
    try {
      setUserKeys(await request<UserKeyView[]>("/api/admin/user-keys"));
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  }

  async function createUserKey(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    setNewUserKey("");
    try {
      const payload = {
        label: keyForm.label.trim() || undefined,
        key: keyForm.key.trim() || undefined,
        max_download_mb: parseOptionalPositiveInt(keyForm.max_download_mb, "用户最大下载大小"),
        allow_browser_probe: keyForm.allow_browser_probe,
        allow_ytdlp: keyForm.allow_ytdlp,
        allow_external_adapters: keyForm.allow_external_adapters,
        allow_login_profile: keyForm.allow_login_profile,
      };
      const response = await request<CreatedUserKeyResponse>("/api/admin/user-keys", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      });
      setNewUserKey(response.key);
      setKeyForm({ ...keyForm, key: "", max_download_mb: "" });
      await refreshUserKeys();
      notify("已创建用户密钥，明文只显示这一次。", "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function revokeUserKey(id: string) {
    setBusy(true);
    try {
      await request<void>(`/api/admin/user-keys/${id}/revoke`, { method: "POST" });
      await refreshUserKeys();
      notify("已撤销用户密钥", "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function rotateAdminKey() {
    const customKey = adminKeyForm.trim();
    askConfirm({
      title: "轮换管理员密钥",
      message: customKey
        ? "旧管理员密钥会立即失效。将使用你输入的自定义新密钥，并自动填入当前页面。"
        : "旧管理员密钥会立即失效。新管理员密钥只显示一次，并会自动填入当前页面。",
      confirmLabel: "轮换密钥",
      danger: true,
      onConfirm: async () => {
        setBusy(true);
        setNewAdminKey("");
        try {
          const response = await request<RotatedAdminKeyResponse>("/api/admin/admin-key/rotate", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify(customKey ? { key: customKey } : {}),
          });
          setNewAdminKey(response.key);
          setAdminKeyForm("");
          setApiKey(response.key);
          await refreshUserKeys();
          notify("已轮换管理员密钥，明文只显示这一次。", "success");
        } catch (error) {
          notify(errorMessage(error), "error");
        } finally {
          setBusy(false);
        }
      },
    });
  }

  async function importProfileCookies(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    try {
      const parsed = JSON.parse(cookieJson);
      const cookies = Array.isArray(parsed) ? parsed : parsed.cookies;
      if (!Array.isArray(cookies)) {
        throw new Error("Cookie JSON 必须是数组，或包含 cookies 数组");
      }
      await request(`/api/admin/browser-profiles/${encodeURIComponent(profileId)}/cookies/import`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ cookies }),
      });
      setCookieJson("");
      notify("已导入浏览器 Profile Cookie", "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  function loginSessionEndpoint(action: string): string {
    const sessionId = encodeURIComponent(loginSnapshot?.session.id ?? "");
    if (activeLoginJobId) {
      return `/api/jobs/${encodeURIComponent(activeLoginJobId)}/browser-login-session/${sessionId}/${action}`;
    }
    return `/api/admin/browser-login-sessions/${sessionId}/${action}`;
  }

  async function requestLoginSnapshot(
    endpoint: string,
    options?: RequestInit,
  ): Promise<BrowserLoginSnapshot> {
    const requestId = ++loginSnapshotRequestRef.current;
    const snapshot = await request<BrowserLoginSnapshot>(endpoint, options);
    if (requestId === loginSnapshotRequestRef.current) {
      setLoginSnapshot(snapshot);
    }
    return snapshot;
  }

  function capturePointerSafely(event: React.PointerEvent<HTMLElement>) {
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // Pointer capture can fail if the browser already ended the pointer stream.
    }
  }

  function releasePointerSafely(event?: React.PointerEvent<HTMLElement>) {
    if (!event) return;
    try {
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
    } catch {
      // Losing capture is recoverable; the server-side mouse-up is the important step.
    }
  }

  async function startJobBrowserLoginSession(job: JobView) {
    setBusy(true);
    try {
      const snapshot = await request<BrowserLoginSnapshot>(
        `/api/jobs/${encodeURIComponent(job.id)}/browser-login-session`,
        { method: "POST" },
      );
      setActiveLoginJobId(job.id);
      setProfileId(snapshot.session.profileId);
      setLoginUrl(snapshot.url);
      setLoginSnapshot(snapshot);
      notify("已打开共享 Profile 验证浏览器。完成验证或登录后点击继续解析。", "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function resumeSelectedJobWithProfile() {
    if (!selectedJob) return;
    setBusy(true);
    try {
      const job = await request<JobView>(
        `/api/jobs/${encodeURIComponent(selectedJob.id)}/resume-with-profile`,
        { method: "POST" },
      );
      setSelectedJob(job);
      await refreshJobs();
      notify("已使用登录 Profile 继续解析任务。", "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function forceSelectedJobPageArchive() {
    if (!selectedJob) return;
    setBusy(true);
    try {
      const job = await request<JobView>(
        `/api/jobs/${encodeURIComponent(selectedJob.id)}/force-page-archive`,
        { method: "POST" },
      );
      setSelectedJob(job);
      await refreshJobs();
      notify("已强制按未登录网页包重新解析。", "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function startBrowserLoginSession() {
    setBusy(true);
    try {
      setActiveLoginJobId(null);
      const snapshot = await request<BrowserLoginSnapshot>(
        `/api/admin/browser-profiles/${encodeURIComponent(profileId)}/login-sessions`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ url: loginUrl }),
        },
      );
      setLoginSnapshot(snapshot);
      notify("已打开服务端浏览器会话。登录完成后直接关闭会话即可，Cookie 会留在 Profile。", "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function openBrowserLoginTarget(url: string) {
    setLoginUrl(url);
    setBusy(true);
    try {
      if (loginSnapshot) {
        await requestLoginSnapshot(
          loginSessionEndpoint("navigate"),
          {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ url }),
          },
        );
        notify("已切换服务端远程浏览器站点。", "success");
      } else {
        setActiveLoginJobId(null);
        const snapshot = await request<BrowserLoginSnapshot>(
          `/api/admin/browser-profiles/${encodeURIComponent(profileId)}/login-sessions`,
          {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ url }),
          },
        );
        setLoginSnapshot(snapshot);
        notify("已打开服务端远程浏览器。登录完成后关闭会话，Profile 会保留 Cookie。", "success");
      }
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function refreshBrowserLoginSession() {
    if (!loginSnapshot) return;
    setBusy(true);
    try {
      await requestLoginSnapshot(
        loginSessionEndpoint("snapshot"),
      );
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  function browserPointFromEvent(
    event: React.PointerEvent<HTMLElement> | React.WheelEvent<HTMLElement>,
  ) {
    if (!loginSnapshot) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const point = {
      x: ((event.clientX - rect.left) / rect.width) * loginSnapshot.width,
      y: ((event.clientY - rect.top) / rect.height) * loginSnapshot.height,
    };
    lastBrowserPointRef.current = point;
    return point;
  }

  function browserButtonFromPointer(event: React.PointerEvent<HTMLElement>): BrowserMouseButton {
    if (event.button === 1) return "middle";
    if (event.button === 2) return "right";
    return "left";
  }

  async function sendBrowserPointerAction(
    action: "move" | "mouse-down" | "mouse-up",
    point: { x: number; y: number },
    button?: BrowserMouseButton,
  ) {
    const payload = action === "move" ? point : { ...point, button: button ?? "left" };
    await requestLoginSnapshot(
      loginSessionEndpoint(action),
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      },
    );
  }

  async function moveBrowserLoginSession(event: React.PointerEvent<HTMLButtonElement>) {
    if (!loginSnapshot) return;
    const now = Date.now();
    if (now - lastMouseMoveRef.current < 120 || moveInFlightRef.current) return;
    lastMouseMoveRef.current = now;
    const point = browserPointFromEvent(event);
    if (!point) return;
    if (activePointerRef.current?.pointerId === event.pointerId) {
      activePointerRef.current.point = point;
    }
    moveInFlightRef.current = true;
    try {
      await sendBrowserPointerAction("move", point);
    } catch {
      // Mouse movement is best-effort; keep the current screenshot usable.
    } finally {
      moveInFlightRef.current = false;
    }
  }

  async function mouseDownBrowserLoginSession(event: React.PointerEvent<HTMLButtonElement>) {
    if (!loginSnapshot) return;
    event.preventDefault();
    const point = browserPointFromEvent(event);
    if (!point) return;
    capturePointerSafely(event);
    const button = browserButtonFromPointer(event);
    activePointerRef.current = { pointerId: event.pointerId, button, point };
    try {
      await sendBrowserPointerAction("mouse-down", point, button);
    } catch (error) {
      activePointerRef.current = null;
      notify(errorMessage(error), "error");
    }
  }

  async function mouseUpBrowserLoginSession(event?: React.PointerEvent<HTMLButtonElement>) {
    if (!loginSnapshot || !activePointerRef.current) return;
    event?.preventDefault();
    const activePointer = activePointerRef.current;
    const point = event ? browserPointFromEvent(event) : (lastBrowserPointRef.current ?? activePointer.point);
    if (!point) return;
    releasePointerSafely(event);
    try {
      await sendBrowserPointerAction("mouse-up", point, activePointer.button);
      if (activePointerRef.current?.pointerId === activePointer.pointerId) {
        activePointerRef.current = null;
      }
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  }

  async function cancelBrowserPointerSession(event: React.PointerEvent<HTMLButtonElement>) {
    if (activePointerRef.current?.pointerId !== event.pointerId) return;
    await mouseUpBrowserLoginSession(event);
  }

  async function wheelBrowserLoginSession(event: React.WheelEvent<HTMLButtonElement>) {
    if (!loginSnapshot) return;
    event.preventDefault();
    const point = browserPointFromEvent(event);
    try {
      await requestLoginSnapshot(
        loginSessionEndpoint("wheel"),
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            deltaX: event.deltaX,
            deltaY: event.deltaY,
            x: point?.x,
            y: point?.y,
          }),
        },
      );
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  }

  async function resizeBrowserLoginSession(width: number, height: number) {
    if (!loginSnapshot) return;
    setBusy(true);
    try {
      await requestLoginSnapshot(
        loginSessionEndpoint("resize"),
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ width, height }),
        },
      );
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function typeIntoBrowserLoginSession() {
    if (!loginSnapshot || !loginText) return;
    setBusy(true);
    try {
      await requestLoginSnapshot(
        loginSessionEndpoint("type"),
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ text: loginText }),
        },
      );
      setLoginText("");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function insertTextIntoBrowserLoginSession() {
    if (!loginSnapshot || !loginText) return;
    setBusy(true);
    try {
      await requestLoginSnapshot(
        loginSessionEndpoint("insert-text"),
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ text: loginText }),
        },
      );
      setLoginText("");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function pressBrowserLoginKey(key: string) {
    if (!loginSnapshot) return;
    setBusy(true);
    try {
      await requestLoginSnapshot(
        loginSessionEndpoint("press"),
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ key }),
        },
      );
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function navigateBrowserLoginSession() {
    if (!loginSnapshot) return;
    setBusy(true);
    try {
      await requestLoginSnapshot(
        loginSessionEndpoint("navigate"),
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ url: loginUrl }),
        },
      );
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function closeBrowserLoginSession() {
    if (!loginSnapshot) return;
    const closeEndpoint = loginSessionEndpoint("close");
    setLoginSnapshot(null);
    setActiveLoginJobId(null);
    activePointerRef.current = null;
    loginSnapshotRequestRef.current += 1;
    try {
      await request(closeEndpoint, {
        method: "POST",
      });
      notify("已关闭服务端浏览器会话，Profile 已保留登录态。", "success");
    } catch (error) {
      notify(`验证浏览器已从当前页面关闭；服务端清理失败：${errorMessage(error)}`, "error");
    }
  }

  const consoleView = (
    <div className="view-stack">
      {!apiKey && (
        <div className="notice-strip warn">
          填写管理密钥或用户密钥后可查看任务、创建解析和选择资源。管理功能只对管理密钥开放。
        </div>
      )}

      <section className="panel composer-panel">
        <div className="panel-header composer-header">
          <div>
            <h2><Play size={16} /> 新建解析任务</h2>
            <p>粘贴链接后自动选择可用的最高质量资源，必要时再手动调整参数。</p>
          </div>
          <div className="profile-pill">
            Profile <strong>{form.profile_id}</strong>
          </div>
        </div>

        <form className="composer-form" onSubmit={createJob}>
          <div className="url-command-row">
            <label className="url-field">
              <span>来源 URL</span>
              <Input
                required
                type="url"
                placeholder="https://example.com/watch/123"
                value={form.url}
                className={sourceUrlIssue ? "input-warning" : ""}
                onChange={(event) => setForm({ ...form, url: event.target.value })}
              />
              {sourceUrlIssue && <span className="field-hint warn">{sourceUrlIssue}</span>}
            </label>
            <Button type="button" variant="secondary" onClick={clearForm} disabled={busy}>
              <X size={16} /> 清空
            </Button>
            <Button className="create-button" type="submit" disabled={busy}>
              {busy ? <Loader2 className="animate-spin" size={16} /> : <Search size={16} />}
              创建任务
            </Button>
          </div>

          <div className="parameter-grid">
            <ControlGroup label="解析方式">
              <SegmentedControl
                value={form.discovery}
                options={["auto", "browser", "external", "direct"]}
                labelFor={discoveryLabel}
                onChange={(value) => setForm({ ...form, discovery: value as DiscoveryMode })}
              />
            </ControlGroup>
            <ControlGroup label="清晰度">
              <Dropdown
                value={form.bitrate}
                options={["auto", "2160p", "1440p", "1080p", "720p", "480p", "360p"]}
                labelFor={bitrateLabel}
                onChange={(value) => setForm({ ...form, bitrate: value })}
              />
            </ControlGroup>
            <ControlGroup label="站点">
              <Dropdown
                value={form.platform_hint}
                options={capabilities?.supported_platform_hints?.length
                  ? capabilities.supported_platform_hints
                  : ["auto", "bilibili", "youtube", "soundcloud", "douyin", "kuaishou", "pornhub", "acfun", "iqiyi", "youku", "tiktok", "vimeo", "live", "generic"]}
                labelFor={platformLabel}
                onChange={(value) => setForm({ ...form, platform_hint: value as PlatformHint })}
              />
            </ControlGroup>
            <ControlGroup label="输出">
              <SegmentedControl
                value={outputMode}
                options={["auto", "video", "audio", "image", "page_html"]}
                labelFor={outputModeLabel}
                onChange={(value) => setOutputModeAndPayload(value as OutputMode)}
              />
              {outputMode === "page_html" && (
                <span className="field-hint">保存入口 HTML、CSS、JS、图片和资源清单，并生成 archive.zip。</span>
              )}
            </ControlGroup>
            <ControlGroup label="授权">
              <Dropdown
                value={outputMode === "page_html" ? "none" : form.auth_mode}
                options={["auto", "none", "profile", "cookies"]}
                labelFor={authModeLabel}
                disabled={outputMode === "page_html"}
                onChange={(value) => setForm({ ...form, auth_mode: value as CreateJobPayload["auth_mode"] })}
              />
            </ControlGroup>
          </div>
        </form>
      </section>

      <section className="workbench-grid">
        <Card
          title="任务列表"
          icon={<Activity size={16} />}
          action={
            <div className="panel-actions">
              <Button variant="secondary" onClick={clearVisibleJobs} disabled={busy || !jobs.length}>
                <X size={16} /> 清空
              </Button>
              <Button variant="secondary" onClick={() => refreshJobs()}>
                <ListRestart size={16} /> 刷新
              </Button>
            </div>
          }
          className="dashboard-panel"
          bodyClassName="dashboard-panel-body"
        >
          <div className="panel-scroll-layout task-panel-layout">
            <div className="task-summary">
              <span><strong>{taskStats.total}</strong> 全部</span>
              <span className="ok"><strong>{taskStats.ready}</strong> 已完成</span>
              <span><strong>{taskStats.candidates}</strong> 待选择</span>
              <span><strong>{taskStats.running}</strong> 处理中</span>
              <span className={taskStats.error ? "bad" : ""}><strong>{taskStats.error}</strong> 失败</span>
              {taskStats.cookie > 0 && <span className="warn"><strong>{taskStats.cookie}</strong> 需授权</span>}
              {taskStats.dependency > 0 && <span className="warn"><strong>{taskStats.dependency}</strong> 缺依赖</span>}
              {taskStats.unsupported > 0 && <span className="info"><strong>{taskStats.unsupported}</strong> 待适配</span>}
            </div>
            <div className="task-list-head">
              <span>状态</span>
              <span>来源</span>
              <span>策略</span>
              <span>更新</span>
            </div>
            <div className="task-list">
              {pagedJobs.items.map((job) => (
                <button
                  key={job.id}
                  className={`task-row ${job.id === selectedJobId ? "selected" : ""}`}
                  type="button"
                  onClick={() => setSelectedJobId(job.id)}
                >
                  <span className="task-status"><JobStatusBadge job={job} /></span>
                  <span className="task-source">
                    <strong>{sourceTitle(job.source_url)}</strong>
                    <small>{job.error ? friendlyError(job.error, job) : job.source_url}</small>
                  </span>
                  <span className="task-tags">
                    <em>{discoveryLabel(job.discovery)}</em>
                    <em>{platformLabel(job.platform_hint)}</em>
                    <em>{outputsLabel(job.outputs)}</em>
                  </span>
                  <span className="task-time">{formatShortDate(job.updated_at)}</span>
                </button>
              ))}
              {!pagedJobs.items.length && <Empty label="当前没有任务" />}
            </div>
            <Pager
              page={pagedJobs.page}
              pageSize={jobPageSize}
              total={jobs.length}
              onPageChange={setJobPage}
              onPageSizeChange={(value) => {
                setJobPageSize(value);
                setJobPage(1);
              }}
            />
          </div>
        </Card>

        <Card
          title="任务详情"
          icon={<Settings size={16} />}
          className="dashboard-panel"
          bodyClassName="dashboard-panel-body"
          action={selectedJob && (jobIssue(selectedJob)?.kind === "profile" || isPageArchiveJob(selectedJob)) ? (
            <div className="profile-card-actions">
              {jobIssue(selectedJob)?.kind === "profile" && (
                <>
                  <Button type="button" variant="secondary" disabled={busy} onClick={() => startJobBrowserLoginSession(selectedJob)}>
                    <MonitorPlay size={16} /> 打开验证
                  </Button>
                  <Button type="button" disabled={busy} onClick={resumeSelectedJobWithProfile}>
                    <RefreshCw size={16} /> 继续
                  </Button>
                </>
              )}
              <Button type="button" variant="secondary" disabled={busy} onClick={forceSelectedJobPageArchive}>
                <RefreshCw size={16} /> 保存网页包
              </Button>
            </div>
          ) : undefined}
        >
          {selectedJob ? (
            <div className="detail-layout">
              <div className="detail-title-row">
                <JobStatusBadge job={selectedJob} />
                <div>
                  <strong>{sourceTitle(selectedJob.source_url)}</strong>
                  <span>{selectedJob.id}</span>
                </div>
              </div>
              <div className="meta-list">
                <MetaLine label="输出" value={outputsLabel(selectedJob.outputs)} />
                <MetaLine label="解析" value={`${discoveryLabel(selectedJob.discovery)} / ${platformLabel(selectedJob.platform_hint)}`} />
                <MetaLine label="清晰度" value={bitrateLabel(selectedJob.bitrate)} />
                <MetaLine label="授权" value={authModeLabel(selectedJob.auth_mode)} />
                <MetaLine label="更新时间" value={formatShortDate(selectedJob.updated_at)} />
                <MetaLine label={isPageArchiveJob(selectedJob) ? "网页包地址" : "播放地址"} value={selectedJob.media_url ?? "-"} copyable />
              </div>
              {selectedJob.error && (
                <div className={`error-line ${jobIssue(selectedJob)?.tone ?? "error"}`}>
                  <AlertTriangle size={16} />
                  <div>
                    <strong>{jobIssue(selectedJob)?.label ?? "失败原因"}</strong>
                    <span>{friendlyError(selectedJob.error, selectedJob)}</span>
                  </div>
                  {jobIssue(selectedJob)?.kind === "profile" && (
                    <div className="error-line-actions">
                      <Button type="button" variant="secondary" disabled={busy} onClick={() => startJobBrowserLoginSession(selectedJob)}>
                        <MonitorPlay size={16} /> 打开验证浏览器
                      </Button>
                      <Button type="button" disabled={busy} onClick={resumeSelectedJobWithProfile}>
                        <RefreshCw size={16} /> 继续解析
                      </Button>
                      <Button type="button" variant="secondary" disabled={busy} onClick={forceSelectedJobPageArchive}>
                        <RefreshCw size={16} /> 保存网页包
                      </Button>
                    </div>
                  )}
                </div>
              )}
              {isPageArchiveJob(selectedJob) && jobIssue(selectedJob)?.kind === "profile" && (
                <div className="screen-help">
                  只想保存未登录主页 UI 时，点“强制解析”。系统会忽略登录提示，重新保存当前可公开访问的 HTML/CSS/JS 和页面资源。
                </div>
              )}
              {jobIssue(selectedJob)?.kind === "profile" && (
                <div className="remote-login-card">
                  <div className="remote-login-head">
                    <div>
                      <strong>此任务需要网页登录或安全验证</strong>
                      <span>打开共享 Profile 服务端浏览器，完成登录、真人验证或站点确认后继续解析此任务。</span>
                    </div>
                    <div className="panel-actions">
                      <Button type="button" variant="secondary" disabled={busy} onClick={() => startJobBrowserLoginSession(selectedJob)}>
                        <MonitorPlay size={16} /> 打开登录
                      </Button>
                      <Button type="button" disabled={busy} onClick={resumeSelectedJobWithProfile}>
                        <RefreshCw size={16} /> 继续解析
                      </Button>
                      <Button type="button" variant="secondary" disabled={busy} onClick={forceSelectedJobPageArchive}>
                        <RefreshCw size={16} /> 保存网页包
                      </Button>
                    </div>
                  </div>
                  {activeLoginJobId === selectedJob.id && loginSnapshot && (
                    <div className="screen-help">
                      验证浏览器已在页面中央打开。当前控制器会把点击、输入和滚轮同步到服务端 Profile；复杂真人验证建议使用受信任的真实远程桌面浏览器完成后再继续。
                    </div>
                  )}
                </div>
              )}
              {selectedJob.media_url && <Player url={selectedJob.media_url} contentType={jobMediaContentType(selectedJob, artifacts)} />}
            </div>
          ) : (
            <Empty label="请选择一个任务" />
          )}
        </Card>
      </section>

      <section className="media-grid">
        <Card
          title="资源选择"
          icon={<FileAudio size={16} />}
          className="resource-panel"
          bodyClassName="resource-panel-body"
          action={!isPageArchiveJob(selectedJob) ? (
            <Button
              onClick={selectCandidates}
              disabled={!selectedJob || (!selectedCandidates.size && defaultCandidateIds.length === 0) || busy}
            >
              {busy ? <Loader2 className="animate-spin" size={16} /> : <Play size={16} />}
              {selectedCandidates.size
                ? "转换选中"
                : defaultCandidateIds.length > 1
                  ? `转换推荐 (${defaultCandidateIds.length})`
                  : "转换推荐"}
            </Button>
          ) : undefined}
        >
          {isPageArchiveJob(selectedJob) ? (
            <Empty label="当前任务是 HTML/CSS/JS 网页包；请在产物中下载网页前端包或打开入口 HTML，不会生成可转码媒体候选。" />
          ) : candidates.length ? (
            <div className="panel-scroll-layout">
              <div className="resource-toolbar">
                <span>
                  找到 {candidates.length} 个，显示 {pagedCandidates.items.length} 个。
                  {selectedJob && ` ${qualityAvailabilityLabel(candidates, selectedJob.bitrate)}`}
                </span>
                <Button type="button" variant="secondary" className="h-8" onClick={() => setShowAllCandidates(!showAllCandidates)}>
                  {showAllCandidates ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                  {showAllCandidates ? "只看推荐" : "显示全部"}
                </Button>
              </div>
              <div className="resource-list">
                {pagedCandidates.items.map((candidate, index) => (
                  <CandidateRow
                    key={candidate.id}
                    candidate={candidate}
                    recommended={!selectedCandidates.size && defaultCandidateIds.includes(candidate.id)}
                    index={pagedCandidates.start + index}
                    selected={selectedCandidates.has(candidate.id)}
                    disabled={!isUsableCandidate(candidate)}
                    onToggle={() => toggleCandidate(candidate.id)}
                  />
                ))}
              </div>
              <Pager
                page={pagedCandidates.page}
                pageSize={candidatePageSize}
                total={visibleCandidates.length}
                onPageChange={setCandidatePage}
                onPageSizeChange={(value) => {
                  setCandidatePageSize(value);
                  setCandidatePage(1);
                }}
              />
            </div>
          ) : (
            <Empty label={selectedJob?.status === "error" ? "解析失败，没有可用资源" : "还没有发现可用资源"} />
          )}
        </Card>

        <Card title="产物" icon={<ExternalLink size={16} />} className="resource-panel" bodyClassName="resource-panel-body">
          {orderedArtifacts.length ? (
            <div className="artifact-list">
              {orderedArtifacts.map((artifact) => (
                <div key={artifact.id} className="artifact-item">
                  <div className="artifact-head">
                    <div>
                      <strong>{artifactLabel(artifact)}</strong>
                      <span>{artifact.content_type} / {formatBytes(artifact.bytes)}</span>
                    </div>
                    <div className="panel-actions">
                      <Button variant="secondary" onClick={() => copy(artifact.media_url)}>
                        <Clipboard size={16} /> 复制
                      </Button>
                      <ActionLink href={artifact.media_url} variant="secondary">
                        <ExternalLink size={16} /> {artifactOpenLabel(artifact)}
                      </ActionLink>
                    </div>
                  </div>
                  <Player url={artifact.media_url} contentType={artifact.content_type} />
                </div>
              ))}
            </div>
          ) : (
            <Empty label="暂无产物" />
          )}
          {orderedArchiveFiles.length ? (
            <div className="archive-browser">
              <div className="archive-browser-head">
                <strong>归档资源</strong>
                <span>{orderedArchiveFiles.length} 个文件</span>
              </div>
              <div className="archive-file-list">
                {orderedArchiveFiles.slice(0, 80).map((file) => (
                  <div key={file.path} className="archive-file-row">
                    <div>
                      <strong>{archiveFileLabel(file)}</strong>
                      <span>{file.path} / {file.content_type} / {formatBytes(file.bytes)}</span>
                    </div>
                    <div className="panel-actions">
                      <Button
                        className="h-8"
                        variant="secondary"
                        onClick={async () => {
                          await copy(file.media_url);
                          notify("已复制归档接口地址；直接访问仍需要当前密钥", "info");
                        }}
                      >
                        <Clipboard size={14} /> 复制
                      </Button>
                      <Button className="h-8" variant="secondary" onClick={() => void openArchiveFile(file)}>
                        <ExternalLink size={14} /> 打开
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ) : null}
        </Card>
      </section>
    </div>
  );

  const adminView = (
    <div className="view-stack">
      <Card
        title="运行设置"
        icon={<Settings size={16} />}
        action={
          <div className="panel-actions">
            <Button variant="secondary" onClick={refreshAdminPanel} disabled={busy || !isAdmin}>
              <RefreshCw size={16} /> 刷新
            </Button>
            <Button form="runtime-settings-form" type="submit" disabled={busy || !isAdmin}>
              保存设置
            </Button>
          </div>
        }
      >
        <form id="runtime-settings-form" className="admin-form" onSubmit={saveRuntimeSettings}>
          <div className="settings-grid">
            <Field label="公网 Base URL">
              <Input
                value={settingsForm.public_base_url}
                placeholder={window.location.origin}
                onChange={(event) => setSettingsForm({ ...settingsForm, public_base_url: event.target.value })}
              />
            </Field>
            <Field label="单任务最大大小 MB">
              <Input
                inputMode="numeric"
                value={settingsForm.max_download_mb}
                onChange={(event) => setSettingsForm({ ...settingsForm, max_download_mb: event.target.value })}
              />
            </Field>
            <Field label="下载/转码超时 秒">
              <Input
                inputMode="numeric"
                value={settingsForm.download_timeout_seconds}
                onChange={(event) => setSettingsForm({ ...settingsForm, download_timeout_seconds: event.target.value })}
              />
            </Field>
            <Field label="yt-dlp 超时 秒">
              <Input
                inputMode="numeric"
                value={settingsForm.yt_dlp_timeout_seconds}
                onChange={(event) => setSettingsForm({ ...settingsForm, yt_dlp_timeout_seconds: event.target.value })}
              />
            </Field>
            <Field label="yt-dlp JSON 上限 MB">
              <Input
                inputMode="numeric"
                value={settingsForm.yt_dlp_max_json_mb}
                onChange={(event) => setSettingsForm({ ...settingsForm, yt_dlp_max_json_mb: event.target.value })}
              />
            </Field>
            <Field label="任务保留小时">
              <Input
                inputMode="numeric"
                value={settingsForm.job_ttl_hours}
                onChange={(event) => setSettingsForm({ ...settingsForm, job_ttl_hours: event.target.value })}
              />
            </Field>
            <Field label="网页资源数上限">
              <Input
                inputMode="numeric"
                value={settingsForm.page_archive_max_resources}
                onChange={(event) => setSettingsForm({ ...settingsForm, page_archive_max_resources: event.target.value })}
              />
            </Field>
            <Field label="网页单资源上限 MB">
              <Input
                inputMode="numeric"
                value={settingsForm.page_archive_max_resource_mb}
                onChange={(event) => setSettingsForm({ ...settingsForm, page_archive_max_resource_mb: event.target.value })}
              />
            </Field>
            <Field label="网页资源包总上限 MB">
              <Input
                inputMode="numeric"
                value={settingsForm.page_archive_max_total_mb}
                onChange={(event) => setSettingsForm({ ...settingsForm, page_archive_max_total_mb: event.target.value })}
              />
            </Field>
            <Field label="CDP 单响应上限 MB">
              <Input
                inputMode="numeric"
                value={settingsForm.page_archive_cdp_body_max_mb}
                onChange={(event) => setSettingsForm({ ...settingsForm, page_archive_cdp_body_max_mb: event.target.value })}
              />
            </Field>
            <Field label="CDP 响应总上限 MB">
              <Input
                inputMode="numeric"
                value={settingsForm.page_archive_cdp_body_total_mb}
                onChange={(event) => setSettingsForm({ ...settingsForm, page_archive_cdp_body_total_mb: event.target.value })}
              />
            </Field>
            <Field label="缓存清理最小小时">
              <Input
                inputMode="numeric"
                value={settingsForm.cache_cleanup_min_age_hours}
                onChange={(event) => setSettingsForm({ ...settingsForm, cache_cleanup_min_age_hours: event.target.value })}
              />
            </Field>
          </div>
          <div className="permission-grid">
            <Toggle
              checked={settingsForm.page_archive_capture_cdp_enabled}
              label="CDP 捕获"
              onChange={(checked) => setSettingsForm({ ...settingsForm, page_archive_capture_cdp_enabled: checked })}
            />
            <Toggle
              checked={settingsForm.page_archive_save_mhtml_enabled}
              label="保存 MHTML"
              onChange={(checked) => setSettingsForm({ ...settingsForm, page_archive_save_mhtml_enabled: checked })}
            />
            <Toggle
              checked={settingsForm.page_archive_save_har_enabled}
              label="保存 HAR"
              onChange={(checked) => setSettingsForm({ ...settingsForm, page_archive_save_har_enabled: checked })}
            />
            <Toggle
              checked={settingsForm.page_archive_save_warc_enabled}
              label="保存 WARC"
              onChange={(checked) => setSettingsForm({ ...settingsForm, page_archive_save_warc_enabled: checked })}
            />
          </div>
          <div className="settings-readonly-grid">
            <MetaLine label="当前全局大小" value={formatBytes(runtimeSettings?.max_download_bytes ?? health?.max_download_bytes)} />
            <MetaLine label="当前密钥大小" value={formatBytes(capabilities?.max_download_bytes)} />
            <MetaLine label="并发任务" value={runtimeSettings?.max_concurrent_jobs ?? capabilities?.max_concurrent_jobs ?? "-"} />
            <MetaLine label="浏览器超时" value={`${runtimeSettings?.browser_probe_timeout_seconds ?? capabilities?.browser_probe_timeout_seconds ?? "-"} 秒`} />
            <MetaLine label="外部解析超时" value={`${runtimeSettings?.external_probe_timeout_seconds ?? "-"} 秒`} />
            <MetaLine label="FFmpeg" value={runtimeSettings?.ffmpeg_path ?? capabilities?.ffmpeg_path ?? "-"} />
            <MetaLine label="Browser Probe" value={runtimeSettings?.browser_probe_url ?? "-"} />
            <MetaLine label="yt-dlp" value={runtimeSettings?.yt_dlp_path ?? capabilities?.yt_dlp_path ?? "-"} />
            <MetaLine label="you-get" value={runtimeSettings?.you_get_path ?? capabilities?.you_get_path ?? "-"} />
            <MetaLine label="lux" value={runtimeSettings?.lux_path ?? capabilities?.lux_path ?? "-"} />
            <MetaLine label="Streamlink" value={runtimeSettings?.streamlink_path ?? capabilities?.streamlink_path ?? "-"} />
            <MetaLine label="外部工具" value={capabilities?.external_tools?.length ? capabilities.external_tools.join(", ") : "-"} />
            <MetaLine label="网页资源数" value={runtimeSettings?.page_archive_max_resources ?? "-"} />
            <MetaLine label="网页单资源" value={formatBytes(runtimeSettings?.page_archive_max_resource_bytes)} />
            <MetaLine label="网页资源包" value={formatBytes(runtimeSettings?.page_archive_max_total_bytes)} />
            <MetaLine label="CDP 单响应" value={formatBytes(runtimeSettings?.page_archive_cdp_body_max_bytes)} />
            <MetaLine label="CDP 响应总量" value={formatBytes(runtimeSettings?.page_archive_cdp_body_total_bytes)} />
            <MetaLine label="缓存最小年龄" value={`${runtimeSettings?.cache_cleanup_min_age_hours ?? "-"} 小时`} />
          </div>
        </form>
      </Card>

      <Card
        title="缓存与归档"
        icon={<Database size={16} />}
        action={
          <div className="panel-actions">
            <Button variant="secondary" onClick={refreshCacheInventory} disabled={busy || !isAdmin}>
              <RefreshCw size={16} /> 刷新
            </Button>
            <Button variant="secondary" onClick={previewCacheCleanup} disabled={busy || !isAdmin}>
              预览清理
            </Button>
            <Button variant="secondary" onClick={runCacheCleanup} disabled={busy || !isAdmin}>
              清理缓存
            </Button>
          </div>
        }
      >
        <div className="cache-panel">
          <div className="settings-readonly-grid">
            <MetaLine label="缓存总量" value={formatBytes(cacheInventory?.total_bytes)} />
            <MetaLine label="存储根目录" value={cacheInventory?.storage_root ?? "-"} />
            <MetaLine label="清理年龄" value={`${settingsForm.cache_cleanup_min_age_hours || "-"} 小时`} />
          </div>
          <div className="cache-category-list">
            {cacheInventory?.categories.map((category) => (
              <div key={category.name} className="cache-category-row">
                <div>
                  <strong>{cacheCategoryLabel(category.name)}</strong>
                  <span>{category.path}</span>
                </div>
                <div className="key-flags">
                  <em>{formatBytes(category.bytes)}</em>
                  <em>{category.files} 文件</em>
                  <em>{category.directories} 目录</em>
                  <em>{category.cleanup_allowed ? "可清理" : "保留"}</em>
                </div>
              </div>
            ))}
            {!cacheInventory && <Empty label="尚未加载缓存统计" />}
          </div>
          {cacheCleanupPreview && (
            <div className="cache-cleanup-list">
              <div className="archive-browser-head">
                <strong>{cacheCleanupPreview.dry_run ? "清理预览" : "清理结果"}</strong>
                <span>{formatBytes(cacheCleanupPreview.dry_run ? cacheCleanupPreview.total_bytes : cacheCleanupPreview.deleted_bytes)}</span>
              </div>
              {cacheCleanupPreview.entries.slice(0, 50).map((entry) => (
                <div key={entry.path} className="cache-cleanup-row">
                  <div>
                    <strong>{formatBytes(entry.bytes)}</strong>
                    <span>{entry.path}</span>
                  </div>
                  <em>{entry.deleted ? "已删除" : entry.reason}</em>
                </div>
              ))}
              {!cacheCleanupPreview.entries.length && <Empty label="没有可清理缓存" />}
            </div>
          )}
        </div>
      </Card>

      <section className="admin-grid">
        <Card
          title="任务恢复"
          icon={<ListRestart size={16} />}
          action={
            <div className="panel-actions">
              <Button variant="secondary" onClick={restoreHiddenJobs} disabled={busy || !apiKey}>
                恢复上一批
              </Button>
              <Button variant="secondary" onClick={refreshHiddenBatches} disabled={!apiKey}>
                <RefreshCw size={16} /> 刷新
              </Button>
            </div>
          }
        >
          <div className="history-list">
            {hiddenBatches.map((batch) => (
              <div key={batch.id} className={`history-row ${batch.restored_at ? "restored" : ""}`}>
                <div>
                  <strong>{batch.actor_label ?? "未知密钥"}</strong>
                  <span>{batch.id}</span>
                </div>
                <div className="history-stats">
                  <em>隐藏 {batch.hidden_count}</em>
                  <em>已恢复 {batch.restored_count}</em>
                  <em>{formatShortDate(batch.created_at)}</em>
                  {batch.restored_at && <em>恢复于 {formatShortDate(batch.restored_at)}</em>}
                </div>
                <Button
                  className="h-8"
                  variant="secondary"
                  disabled={busy || Boolean(batch.restored_at)}
                  onClick={() => restoreHiddenBatch(batch.id)}
                >
                  恢复此批
                </Button>
              </div>
            ))}
            {!hiddenBatches.length && <Empty label={apiKey ? "暂无隐藏批次" : "需要先填写密钥"} />}
          </div>
        </Card>

        <Card
          title="管理员密钥"
          icon={<Shield size={16} />}
          action={
            <Button variant="secondary" onClick={rotateAdminKey} disabled={busy || !isAdmin}>
              <RefreshCw size={16} /> 轮换
            </Button>
          }
        >
          <div className="admin-key-panel">
            <div>
              <strong>当前管理权限</strong>
              <span>{capabilities?.auth ? `${roleLabel(capabilities.auth.role)} / ${capabilities.auth.label}` : "未确认"}</span>
            </div>
            <Field label="自定义新管理员密钥">
              <Input
                type="password"
                value={adminKeyForm}
                placeholder="留空则自动生成"
                onChange={(event) => setAdminKeyForm(event.target.value)}
              />
            </Field>
            <p>轮换后旧管理员密钥立即失效。自定义密钥长度 16-256，不能包含空白或控制字符。</p>
            {newAdminKey && (
              <div className="key-result">
                <strong>新管理员密钥，只显示一次</strong>
                <code>{newAdminKey}</code>
                <Button className="h-8" variant="secondary" onClick={() => copy(newAdminKey)}>
                  <Clipboard size={14} /> 复制
                </Button>
              </div>
            )}
          </div>
        </Card>

        <Card
          title="用户密钥"
          icon={<KeyRound size={16} />}
          action={<Button variant="secondary" onClick={refreshUserKeys}><RefreshCw size={16} /> 刷新</Button>}
        >
          <form className="admin-form" onSubmit={createUserKey}>
            <div className="settings-grid">
              <Field label="名称">
                <Input value={keyForm.label} onChange={(event) => setKeyForm({ ...keyForm, label: event.target.value })} />
              </Field>
              <Field label="自定义用户密钥">
                <Input
                  type="password"
                  value={keyForm.key}
                  placeholder="留空则自动生成"
                  onChange={(event) => setKeyForm({ ...keyForm, key: event.target.value })}
                />
              </Field>
              <Field label="下载上限 MB">
                <Input
                  inputMode="numeric"
                  value={keyForm.max_download_mb}
                  placeholder="留空跟随全局"
                  onChange={(event) => setKeyForm({ ...keyForm, max_download_mb: event.target.value })}
                />
              </Field>
            </div>
            <div className="permission-grid">
              <Toggle
                checked={keyForm.allow_browser_probe}
                label="允许浏览器探测"
                onChange={(checked) => setKeyForm({ ...keyForm, allow_browser_probe: checked })}
              />
              <Toggle
                checked={keyForm.allow_ytdlp}
                label="允许 yt-dlp"
                onChange={(checked) => setKeyForm({ ...keyForm, allow_ytdlp: checked })}
              />
              <Toggle
                checked={keyForm.allow_external_adapters}
                label="允许外部适配器"
                onChange={(checked) => setKeyForm({ ...keyForm, allow_external_adapters: checked })}
              />
              <Toggle
                checked={keyForm.allow_login_profile}
                label="允许登录 Profile"
                onChange={(checked) => setKeyForm({ ...keyForm, allow_login_profile: checked })}
              />
            </div>
            <Button type="submit" disabled={busy}>创建用户密钥</Button>
          </form>
          {newUserKey && (
            <div className="key-result">
              <strong>明文密钥只显示一次</strong>
              <code>{newUserKey}</code>
              <Button className="h-8" variant="secondary" onClick={() => copy(newUserKey)}>
                <Clipboard size={14} /> 复制
              </Button>
            </div>
          )}
          <div className="key-table">
            {userKeys.map((key) => (
              <div key={key.id} className="key-row">
                <div>
                  <strong>{key.label}</strong>
                  <span>{key.key_prefix}... / {roleLabel(key.role)}</span>
                </div>
                <div className="key-flags">
                  <em>大小：{key.max_download_bytes ? formatBytes(key.max_download_bytes) : "跟随全局"}</em>
                  <em>浏览器：{key.allow_browser_probe ? "允许" : "禁止"}</em>
                  <em>yt-dlp：{key.allow_ytdlp ? "允许" : "禁止"}</em>
                  <em>外部：{key.allow_external_adapters ? "允许" : "禁止"}</em>
                  <em>Profile：{key.allow_login_profile ? "允许" : "禁止"}</em>
                  {key.revoked_at && <em className="danger">已撤销</em>}
                </div>
                <Button className="h-8" variant="secondary" disabled={Boolean(key.revoked_at) || busy} onClick={() => revokeUserKey(key.id)}>
                  撤销
                </Button>
              </div>
            ))}
            {!userKeys.length && <Empty label="暂无用户密钥，点击刷新或创建一个" />}
          </div>
        </Card>

        <Card title="浏览器账号配置" icon={<UserCog size={16} />}>
          <div className="admin-form">
            <Field label="Profile ID">
              <Input value={profileId} onChange={(event) => setProfileId(event.target.value)} />
            </Field>
            <div className="remote-login-card">
              <div className="remote-login-head">
                <div>
                  <strong>服务端远程浏览器</strong>
                  <span>在服务器浏览器里登录站点。解析时自动使用当前 Profile 的 Cookie/Header，不返回 Cookie 明文。</span>
                </div>
                <Button type="button" disabled={busy} onClick={startBrowserLoginSession}>
                  <MonitorPlay size={16} /> 打开会话
                </Button>
              </div>
              <div className="login-target-grid">
                {LOGIN_TARGETS.map((target) => (
                  <Button
                    key={target.id}
                    type="button"
                    variant={target.id === "douyin" ? "primary" : "secondary"}
                    disabled={busy}
                    onClick={() => openBrowserLoginTarget(target.url)}
                  >
                    <MonitorPlay size={14} /> 登录 {target.label}
                  </Button>
                ))}
              </div>
              <div className="admin-note">
                抖音完整视频页常见 `fresh cookies`。先点“登录 抖音”，用手机扫码或账号登录；之后创建任务保持 Profile 为 <strong>{profileId}</strong>、授权为“自动”或“浏览器配置”。
              </div>
              <div className="remote-login-controls">
                <Input value={loginUrl} onChange={(event) => setLoginUrl(event.target.value)} />
                <Button type="button" variant="secondary" disabled={busy || !loginSnapshot} onClick={navigateBrowserLoginSession}>
                  跳转
                </Button>
                <Button type="button" variant="secondary" disabled={busy || !loginSnapshot} onClick={refreshBrowserLoginSession}>
                  <RefreshCw size={16} /> 刷新截图
                </Button>
                <Button type="button" variant="secondary" disabled={busy || !loginSnapshot} onClick={closeBrowserLoginSession}>
                  关闭
                </Button>
              </div>
              {loginSnapshot ? (
                <div className="remote-browser">
                  <div className="remote-browser-meta">
                    <span>{loginSnapshot.title || "未命名页面"}</span>
                    <em>{loginSnapshot.url}</em>
                  </div>
                  <div className="remote-browser-toolbar">
                    <Dropdown
                      value={loginZoom}
                      options={["0.75", "1", "1.25", "1.5", "2"]}
                      labelFor={(value) => `缩放 ${Math.round(Number(value) * 100)}%`}
                      onChange={setLoginZoom}
                    />
                    <Button
                      type="button"
                      variant="secondary"
                      disabled={busy}
                      onClick={() => resizeBrowserLoginSession(1280, 720)}
                    >
                      720p
                    </Button>
                    <Button
                      type="button"
                      variant="secondary"
                      disabled={busy}
                      onClick={() => resizeBrowserLoginSession(1920, 1080)}
                    >
                      1080p
                    </Button>
                  </div>
                  <div className="remote-browser-viewport">
                    <button
                      className="remote-browser-screen"
                      type="button"
                      style={{ width: `${Number(loginZoom) * 100}%` }}
                      onPointerMove={moveBrowserLoginSession}
                      onPointerDown={mouseDownBrowserLoginSession}
                      onPointerUp={mouseUpBrowserLoginSession}
                      onPointerCancel={cancelBrowserPointerSession}
                      onLostPointerCapture={cancelBrowserPointerSession}
                      onWheel={wheelBrowserLoginSession}
                      onContextMenu={(event) => {
                        event.preventDefault();
                      }}
                    >
                      <img src={loginSnapshot.image} alt="服务端浏览器截图" draggable={false} />
                    </button>
                  </div>
                  <div className="screen-help">
                    <span>像远程桌面一样操作截图：移动、左键、右键、双击、按住拖动和滚轮都会投射到服务端浏览器。</span>
                  </div>
                  <div className="remote-input-controls">
                    <Input
                      value={loginText}
                      placeholder="输入要发送到当前焦点的文本"
                      onChange={(event) => setLoginText(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") {
                          event.preventDefault();
                          void insertTextIntoBrowserLoginSession();
                        }
                      }}
                    />
                    <Button type="button" variant="secondary" disabled={busy || !loginText} onClick={typeIntoBrowserLoginSession}>
                      输入
                    </Button>
                    <Button type="button" variant="secondary" disabled={busy || !loginText} onClick={insertTextIntoBrowserLoginSession}>
                      插入
                    </Button>
                  </div>
                  <div className="remote-key-strip">
                    <Button type="button" variant="secondary" disabled={busy} onClick={() => pressBrowserLoginKey("Enter")}>
                      Enter
                    </Button>
                    <Button type="button" variant="secondary" disabled={busy} onClick={() => pressBrowserLoginKey("Tab")}>
                      Tab
                    </Button>
                    <Button type="button" variant="secondary" disabled={busy} onClick={() => pressBrowserLoginKey("Space")}>
                      Space
                    </Button>
                    <Button type="button" variant="secondary" disabled={busy} onClick={() => pressBrowserLoginKey("Control+A")}>
                      Ctrl+A
                    </Button>
                    <Button type="button" variant="secondary" disabled={busy} onClick={() => pressBrowserLoginKey("Backspace")}>
                      Backspace
                    </Button>
                    <Button type="button" variant="secondary" disabled={busy} onClick={() => pressBrowserLoginKey("Escape")}>
                      Esc
                    </Button>
                  </div>
                </div>
              ) : (
                <div className="admin-note">
                  选择站点地址后打开会话；在截图上点击输入框，再用下方输入栏发送文本。二维码登录可直接用手机扫码。
                </div>
              )}
            </div>
            <form className="admin-form" onSubmit={importProfileCookies}>
              <label className="field">
                <span>Cookie JSON</span>
                <textarea
                  className="input cookie-input"
                  placeholder='[{"name":"SESSDATA","value":"...","domain":".bilibili.com","path":"/"}]'
                  value={cookieJson}
                  onChange={(event) => setCookieJson(event.target.value)}
                />
              </label>
              <Button type="submit" disabled={busy || !cookieJson.trim()}>导入 Cookie 到 Profile</Button>
            </form>
            <div className="admin-note">
              备用方式：`scripts/cookies/import_browser_cookies.py` 可以从本机已登录浏览器导入指定站点 Cookie。Edge/Chrome 正在运行或 Windows 加密限制时，需要关闭浏览器或使用管理员终端。
            </div>
          </div>
        </Card>
      </section>
    </div>
  );

  const helpView = (
    <div className="view-stack">
      <section className="help-grid">
        <HelpCard title="密钥逻辑" lines={[
          "首次管理密钥来自服务器 RK_API_KEY，并会写入数据库；之后可以在管理页轮换。",
          "轮换管理员密钥后，旧管理员密钥会立即失效，新密钥只显示一次。",
          "用户密钥保存在数据库中，只存 SHA-256 摘要；明文只在创建时显示一次。",
          "用户密钥可以被限制是否允许浏览器探测和 yt-dlp。",
        ]} />
        <HelpCard title="解析方式" lines={[
          "自动解析会先走直链，再按当前密钥权限尝试 yt-dlp 和浏览器探测。",
          "浏览器探测适合页面脚本生成、签名 URL、需要播放后才出现资源的站点。",
          "yt-dlp 适合成熟站点规则；如果站点规则过期，需要更新 yt-dlp 或改用浏览器探测。",
        ]} />
        <HelpCard title="授权模式" lines={[
          "自动会按任务和候选资源决定是否复用浏览器 Profile 的 Cookie/Header。",
          "Profile/Cookie 适合哔哩哔哩、YouTube、抖音、快手等登录后清晰度或资源更完整的页面。",
          "后台账号配置不会绕过验证码、DRM、年龄确认或登录风控。",
        ]} />
        <HelpCard title="剪贴板" lines={[
          "HTTPS 或 localhost 页面通常可以在点击按钮后读取系统剪贴板。",
          "当前公网 IP 使用 HTTP 时，Edge/Chrome 可能按安全策略拦截自动读取。",
          "被拦截时按钮会自动聚焦来源 URL，按 Ctrl+V 会自动填入。稳定一键读取需要给服务配置域名和 HTTPS。",
        ]} />
      </section>
    </div>
  );

  const navItems: Array<{ mode: ViewMode; icon: React.ReactNode }> = [
    { mode: "console", icon: <Activity size={16} /> },
    ...(isAdmin ? [{ mode: "admin" as ViewMode, icon: <Settings size={16} /> }] : []),
    { mode: "help", icon: <HelpCircle size={16} /> },
  ];

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand-block">
          <div className="brand-mark">RK</div>
          <div>
            <h1>Reflection King</h1>
            <p>媒体抓取与转码</p>
          </div>
        </div>

        <nav className="side-nav">
          {navItems.map((item) => (
            <button
              key={item.mode}
              className={viewMode === item.mode ? "active" : ""}
              type="button"
              onClick={() => {
                setViewMode(item.mode);
                if (item.mode === "admin" && isAdmin) void refreshAdminPanel();
              }}
            >
              {item.icon}
              {viewModeLabel(item.mode)}
            </button>
          ))}
        </nav>

        <div className="side-section">
          <span className="side-label">系统状态</span>
          <StatusLine label="API" ok={health?.ok} value={health?.service ? "正常" : "连接中"} />
          <StatusLine
            label="浏览器"
            ok={capabilities?.browser_probe_configured}
            value={capabilityStatus(capabilities?.browser_probe_configured, apiKey)}
          />
          <StatusLine
            label="yt-dlp"
            ok={capabilities?.yt_dlp_configured}
            value={capabilityStatus(capabilities?.yt_dlp_configured, apiKey)}
          />
          <StatusLine
            label="外部适配器"
            ok={capabilities?.external_adapters_configured}
            value={capabilityStatus(capabilities?.external_adapters_configured, apiKey)}
          />
        </div>

        <div className="side-section">
          <span className="side-label">当前密钥</span>
          <div className="identity-box">
            <Shield size={16} />
            <span>{capabilities?.auth ? `${roleLabel(capabilities.auth.role)} / ${capabilities.auth.label}` : apiKey ? "读取中" : "未填写"}</span>
          </div>
          <div className="identity-box">
            <Database size={16} />
            <span>{formatBytes(capabilities?.max_download_bytes ?? health?.max_download_bytes)}</span>
          </div>
        </div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div className="topbar-title">
            <strong>{viewModeLabel(viewMode)}</strong>
            <span>{message}</span>
          </div>
          <div className="topbar-actions">
            <Input
              className="key-input"
              type="password"
              placeholder="输入管理密钥或用户密钥"
              value={apiKey}
              onChange={(event) => setApiKey(event.target.value)}
            />
            <Button onClick={refreshSystem} variant="secondary">
              <RefreshCw size={16} /> 刷新
            </Button>
          </div>
        </header>

        <div className="workspace-body">
          {viewMode === "console" && consoleView}
          {viewMode === "admin" && isAdmin && adminView}
          {viewMode === "help" && helpView}
        </div>

        <footer className="app-footer">
          <span>Reflection King {health?.version ? `v${health.version}` : ""}</span>
          <span>{health?.public_base_url ?? window.location.origin}</span>
          <span>{viewModeLabel(viewMode)}</span>
          <span>任务清空只隐藏列表，数据库历史保留</span>
        </footer>
      </section>
      <NotificationStack items={notifications} onClose={(id) => setNotifications((items) => items.filter((item) => item.id !== id))} />
      {filePreview && (
        <FilePreviewModal
          preview={filePreview}
          onClose={closeFilePreview}
          onCopy={() => copy(filePreview.sourceUrl)}
        />
      )}
      {activeLoginJobId && loginSnapshot && (
        <div className="remote-browser-overlay" role="dialog" aria-modal="true" aria-label="任务验证浏览器">
          <section className="remote-browser-modal">
            <div className="remote-browser-modal-head">
              <div>
                <strong>任务验证浏览器</strong>
                <span>{loginSnapshot.title || loginSnapshot.url}</span>
              </div>
              <div className="profile-card-actions">
                <Button type="button" variant="secondary" disabled={busy} onClick={refreshBrowserLoginSession}>
                  <RefreshCw size={16} /> 刷新
                </Button>
                <Button
                  type="button"
                  disabled={busy}
                  onClick={selectedJobIsPageArchive ? forceSelectedJobPageArchive : resumeSelectedJobWithProfile}
                >
                  <RefreshCw size={16} /> {selectedJobIsPageArchive ? "保存网页包" : "继续解析"}
                </Button>
                <Button type="button" variant="secondary" disabled={busy} onClick={closeBrowserLoginSession}>
                  关闭
                </Button>
              </div>
            </div>
            <div className="remote-browser-modal-body">
              <div className="remote-browser">
                <div className="remote-browser-meta">
                  <span>{loginSnapshot.title || "未命名页面"}</span>
                  <em>{loginSnapshot.url}</em>
                </div>
                <div className="remote-browser-toolbar">
                  <Dropdown
                    value={loginZoom}
                    options={["0.75", "1", "1.25", "1.5", "2"]}
                    labelFor={(value) => `缩放 ${Math.round(Number(value) * 100)}%`}
                    onChange={setLoginZoom}
                  />
                  <Button type="button" variant="secondary" disabled={busy} onClick={() => resizeBrowserLoginSession(1280, 720)}>
                    720p
                  </Button>
                  <Button type="button" variant="secondary" disabled={busy} onClick={() => resizeBrowserLoginSession(1920, 1080)}>
                    1080p
                  </Button>
                </div>
                <div className="remote-browser-viewport floating">
                  <button
                    className="remote-browser-screen"
                    type="button"
                    style={{ width: `${Number(loginZoom) * 100}%` }}
                    onPointerMove={moveBrowserLoginSession}
                    onPointerDown={mouseDownBrowserLoginSession}
                    onPointerUp={mouseUpBrowserLoginSession}
                    onPointerCancel={cancelBrowserPointerSession}
                    onLostPointerCapture={cancelBrowserPointerSession}
                    onWheel={wheelBrowserLoginSession}
                    onContextMenu={(event) => {
                      event.preventDefault();
                    }}
                  >
                    <img src={loginSnapshot.image} alt="任务验证浏览器截图" draggable={false} />
                  </button>
                </div>
                <div className="screen-help">
                  <span>像远程桌面一样操作截图：移动、左键、右键、双击、按住拖动和滚轮都会投射到服务端浏览器。页面已可见但只想保存公开 UI 时，请创建 HTML/CSS/JS 网页包任务；继续解析用于带 Profile 再抓媒体候选。</span>
                </div>
                <div className="remote-input-controls">
                  <Input
                    value={loginText}
                    placeholder="输入要发送到当前焦点的文本"
                    onChange={(event) => setLoginText(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.preventDefault();
                        void insertTextIntoBrowserLoginSession();
                      }
                    }}
                  />
                  <Button type="button" variant="secondary" disabled={busy || !loginText} onClick={typeIntoBrowserLoginSession}>
                    输入
                  </Button>
                  <Button type="button" variant="secondary" disabled={busy || !loginText} onClick={insertTextIntoBrowserLoginSession}>
                    插入
                  </Button>
                </div>
                <div className="remote-key-strip">
                  <Button type="button" variant="secondary" disabled={busy} onClick={() => pressBrowserLoginKey("Enter")}>
                    Enter
                  </Button>
                  <Button type="button" variant="secondary" disabled={busy} onClick={() => pressBrowserLoginKey("Tab")}>
                    Tab
                  </Button>
                  <Button type="button" variant="secondary" disabled={busy} onClick={() => pressBrowserLoginKey("Space")}>
                    Space
                  </Button>
                  <Button type="button" variant="secondary" disabled={busy} onClick={() => pressBrowserLoginKey("Control+A")}>
                    Ctrl+A
                  </Button>
                  <Button type="button" variant="secondary" disabled={busy} onClick={() => pressBrowserLoginKey("Backspace")}>
                    Backspace
                  </Button>
                  <Button type="button" variant="secondary" disabled={busy} onClick={() => pressBrowserLoginKey("Escape")}>
                    Esc
                  </Button>
                </div>
              </div>
            </div>
          </section>
        </div>
      )}
      <ConfirmDialog state={confirmDialog} busy={busy} onClose={() => setConfirmDialog(null)} />
    </main>
  );
}

function Card(props: {
  title: string;
  icon: React.ReactNode;
  action?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
  bodyClassName?: string;
}) {
  return (
    <section className={`panel ${props.className ?? ""}`}>
      <div className="panel-header">
        <h2>{props.icon}{props.title}</h2>
        {props.action && <div className="panel-header-action">{props.action}</div>}
      </div>
      <div className={`panel-body ${props.bodyClassName ?? ""}`}>{props.children}</div>
    </section>
  );
}

function Field(props: { label: string; children: React.ReactNode }) {
  return (
    <label className="field">
      <span>{props.label}</span>
      {props.children}
    </label>
  );
}

function ControlGroup(props: { label: string; children: React.ReactNode }) {
  return (
    <div className="control-group">
      <div>{props.label}</div>
      {props.children}
    </div>
  );
}

const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(function Input(props, ref) {
  const { className = "", ...rest } = props;
  return <input ref={ref} className={`input ${className}`} {...rest} />;
});

function Button(props: React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "secondary" }) {
  const { className = "", variant = "primary", type = "button", ...rest } = props;
  return <button type={type} className={`button ${variant} ${className}`} {...rest} />;
}

function ActionLink(props: React.AnchorHTMLAttributes<HTMLAnchorElement> & { variant?: "primary" | "secondary" }) {
  const { className = "", variant = "primary", target = "_blank", rel = "noopener noreferrer", ...rest } = props;
  return <a target={target} rel={rel} className={`button ${variant} ${className}`} {...rest} />;
}

function Dropdown(props: {
  value: string;
  options: string[];
  labelFor?: (value: string) => string;
  onChange: (value: string) => void;
  className?: string;
  disabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
      }
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const label = props.labelFor?.(props.value) ?? props.value;

  return (
    <div ref={rootRef} className={`custom-select ${open ? "open" : ""} ${props.className ?? ""}`}>
      <button
        className="custom-select-trigger"
        type="button"
        disabled={props.disabled}
        onClick={() => {
          if (!props.disabled) setOpen(!open);
        }}
      >
        <span>{label}</span>
        <ChevronDown size={15} />
      </button>
      {open && (
        <div className="custom-select-menu" role="listbox">
          {props.options.map((option) => (
            <button
              key={option}
              className={props.value === option ? "active" : ""}
              type="button"
              role="option"
              aria-selected={props.value === option}
              onClick={() => {
                props.onChange(option);
                setOpen(false);
              }}
            >
              {props.labelFor?.(option) ?? option}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function SegmentedControl(props: {
  value: string;
  options: string[];
  labelFor?: (value: string) => string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="segmented">
      {props.options.map((option) => (
        <button
          key={option}
          className={props.value === option ? "active" : ""}
          type="button"
          onClick={() => props.onChange(option)}
        >
          {props.labelFor?.(option) ?? option}
        </button>
      ))}
    </div>
  );
}

function Toggle(props: { checked: boolean; label: string; onChange: (checked: boolean) => void }) {
  return (
    <label className="toggle">
      <span>{props.label}</span>
      <input
        type="checkbox"
        checked={props.checked}
        onChange={(event) => props.onChange(event.target.checked)}
      />
    </label>
  );
}

function HelpCard(props: { title: string; lines: string[] }) {
  return (
    <Card title={props.title} icon={<Settings size={16} />}>
      <div className="help-copy">
        {props.lines.map((line) => (
          <p key={line}>{line}</p>
        ))}
      </div>
    </Card>
  );
}

function NotificationStack(props: { items: NotificationItem[]; onClose: (id: number) => void }) {
  return (
    <div className="notification-stack" aria-live="polite">
      {props.items.map((item) => (
        <div key={item.id} className={`notification ${item.tone}`}>
          <span>{item.text}</span>
          <button type="button" onClick={() => props.onClose(item.id)} aria-label="关闭通知">
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}

function FilePreviewModal(props: { preview: FilePreviewState; onClose: () => void; onCopy: () => void }) {
  const type = props.preview.contentType.toLowerCase();
  const isFrame = type.includes("html")
    || type.includes("json")
    || type.includes("text/")
    || type.includes("javascript")
    || type.includes("css")
    || type.includes("xml");
  const isImage = type.startsWith("image/");

  return (
    <div className="file-preview-overlay" role="dialog" aria-modal="true" aria-label="文件预览">
      <section className="file-preview-modal">
        <div className="file-preview-head">
          <div>
            <strong>{props.preview.title}</strong>
            <span>{props.preview.contentType || "application/octet-stream"} / {formatBytes(props.preview.bytes)}</span>
          </div>
          <div className="panel-actions">
            <Button type="button" variant="secondary" onClick={props.onCopy}>
              <Clipboard size={16} /> 复制接口
            </Button>
            <ActionLink href={props.preview.blobUrl} variant="secondary" download={props.preview.title}>
              <ExternalLink size={16} /> 下载
            </ActionLink>
            <Button type="button" variant="secondary" onClick={props.onClose}>
              <X size={16} /> 关闭
            </Button>
          </div>
        </div>
        <div className="file-preview-body">
          {isImage ? (
            <img src={props.preview.blobUrl} alt="" />
          ) : isFrame ? (
            <iframe title={props.preview.title} src={props.preview.blobUrl} />
          ) : (
            <div className="file-preview-empty">
              <strong>此文件类型不适合内嵌预览</strong>
              <span>使用下载按钮保存后用本地工具查看。</span>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

function ConfirmDialog(props: {
  state: ConfirmDialogState | null;
  busy: boolean;
  onClose: () => void;
}) {
  if (!props.state) return null;
  return (
    <div className="modal-backdrop" role="presentation">
      <section className="confirm-modal" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
        <div>
          <h2 id="confirm-title">{props.state.title}</h2>
          <p>{props.state.message}</p>
        </div>
        <div className="modal-actions">
          <Button type="button" variant="secondary" onClick={props.onClose} disabled={props.busy}>
            取消
          </Button>
          <Button
            type="button"
            className={props.state.danger ? "danger-button" : ""}
            disabled={props.busy}
            onClick={async () => {
              await props.state?.onConfirm();
              props.onClose();
            }}
          >
            {props.busy ? <Loader2 className="animate-spin" size={16} /> : null}
            {props.state.confirmLabel}
          </Button>
        </div>
      </section>
    </div>
  );
}

function CandidateRow(props: {
  candidate: Candidate;
  index: number;
  selected: boolean;
  recommended: boolean;
  disabled: boolean;
  onToggle: () => void;
}) {
  const summary = candidateSummary(props.candidate);
  const validation = props.candidate.validation_state ?? props.candidate.validation_status;
  const availability = candidateAvailability(props.candidate);
  const isBad = Boolean(
    validation?.startsWith("failed") ||
      ["drm", "expired", "region_blocked"].includes(validation ?? ""),
  );
  return (
    <label
      className={`candidate-row ${
        isBad
          ? "bad"
          : props.selected || props.recommended
          ? "selected"
          : ""
      } ${props.disabled ? "disabled" : ""}`}
    >
      <input
        type="checkbox"
        checked={props.selected}
        disabled={props.disabled}
        onChange={props.onToggle}
      />
      <div className="candidate-main">
        <div className="candidate-title">
          <strong>{props.recommended ? "推荐 " : ""}{candidateKindLabel(props.candidate.kind)}</strong>
          <span>{summary.quality}</span>
          <span>{summary.source}</span>
        </div>
        <div className="candidate-meta">
          <span>类型：{summary.kindDetail}</span>
          <span>来源：{extractorLabel(props.candidate.extractor)}</span>
          <span>路线：{routeLabel(props.candidate.route ?? props.candidate.extractor)}</span>
          <span>评分：{props.candidate.score}</span>
          <span>大小：{formatBytes(props.candidate.content_length)}</span>
        </div>
        <div className="candidate-flags">
          {props.recommended && <em>自动推荐</em>}
          {props.candidate.kind === "manifest" && <em>清单流</em>}
          {(props.candidate.requires_authorization || props.candidate.requires_profile) && <em className="warn">需要页面授权</em>}
          {availability.higherQualityRequiresProfile && (
            <em className="warn">高质量需 Profile{availability.highestAdvertisedHeight ? `（最高 ${availability.highestAdvertisedHeight}p）` : ""}</em>
          )}
          {props.candidate.protection && props.candidate.protection !== "none" && (
            <em className={props.candidate.protection === "drm" ? "danger" : "warn"}>{protectionLabel(props.candidate.protection)}</em>
          )}
          {(props.candidate.evidence_count ?? 1) > 1 && <em>{props.candidate.evidence_count} 路证据</em>}
          {summary.adRisk && <em className="danger">广告/跟踪嫌疑</em>}
          {validation && validation !== "untested" && <em className={isBad ? "danger" : "ok"}>{validationLabel(validation)}</em>}
          {props.candidate.failure_reason && <em className="danger">{friendlyError(props.candidate.failure_reason)}</em>}
          {props.disabled && !props.candidate.failure_reason && <em className="danger">不可转换</em>}
        </div>
        <div className="candidate-url">第 {props.index + 1} 项 · {compactUrl(props.candidate.url)}</div>
      </div>
    </label>
  );
}

function Pager(props: {
  page: number;
  pageSize: number;
  total: number;
  onPageChange: (page: number) => void;
  onPageSizeChange: (pageSize: number) => void;
}) {
  const pageCount = Math.max(1, Math.ceil(props.total / props.pageSize));
  return (
    <div className="pager">
      <span>第 {props.page} / {pageCount} 页，共 {props.total} 条</span>
      <div className="pager-actions">
        <Dropdown
          className="pager-select"
          value={String(props.pageSize)}
          options={PAGE_SIZE_OPTIONS.map(String)}
          labelFor={(value) => `每页 ${value}`}
          onChange={(value) => props.onPageSizeChange(Number(value))}
        />
        <Button
          type="button"
          variant="secondary"
          className="h-8"
          disabled={props.page <= 1}
          onClick={() => props.onPageChange(props.page - 1)}
        >
          上一页
        </Button>
        <Button
          type="button"
          variant="secondary"
          className="h-8"
          disabled={props.page >= pageCount}
          onClick={() => props.onPageChange(props.page + 1)}
        >
          下一页
        </Button>
      </div>
    </div>
  );
}

function MetaLine(props: {
  label: string;
  value: string | number | null | undefined;
  copyable?: boolean;
}) {
  const value = String(props.value ?? "-");
  return (
    <div className="meta-line">
      <span>{props.label}</span>
      <strong>
        {value}
        {props.copyable && value !== "-" && (
          <button onClick={() => copy(value)} type="button">复制</button>
        )}
      </strong>
    </div>
  );
}

function StatusLine(props: { label: string; ok?: boolean; value: string }) {
  return (
    <div className="system-row">
      <span>{props.label}</span>
      <strong>
        {props.ok ? <CheckCircle2 className="text-emerald-400" size={16} /> : <AlertTriangle className="text-yellow-400" size={16} />}
        {props.value}
      </strong>
    </div>
  );
}

function Badge(props: { status: string }) {
  const tone = props.status === "ready"
    ? "ready"
    : props.status === "error"
      ? "error"
      : props.status === "candidates_ready"
        ? "candidates"
        : "progress";
  return <span className={`badge ${tone}`}>{statusLabel(props.status)}</span>;
}

function JobStatusBadge(props: { job: JobView }) {
  const issue = jobIssue(props.job);
  if (!issue) {
    return <Badge status={props.job.status} />;
  }
  return <span className={`badge ${issue.tone}`}>{issue.label}</span>;
}

function Empty(props: { label: string }) {
  return <div className="empty-state">{props.label}</div>;
}

function Player(props: { url: string; contentType?: string | null }) {
  const contentType = props.contentType?.toLowerCase() ?? "";
  const lower = props.url.toLowerCase();
  if (contentType.startsWith("video/") || /\.(mp4|webm|mov|m4v)(?:$|\?)/i.test(lower)) {
    return <video className="player" controls src={props.url} />;
  }
  if (contentType.startsWith("image/") || /\.(png|jpe?g|webp|gif|avif)(?:$|\?)/i.test(lower)) {
    return <img className="player" src={props.url} alt="" />;
  }
  if (contentType.startsWith("audio/") || /\.(mp3|m4a|aac|ogg|opus|wav|flac)(?:$|\?)/i.test(lower)) {
    return <audio className="player" controls src={props.url} />;
  }
  return null;
}

function errorMessage(error: unknown): string {
  return friendlyError(error instanceof Error ? error.message : String(error));
}

function apiHeaders(apiKey: string): Record<string, string> {
  return apiKey ? { "x-api-key": apiKey } : {};
}

function formatShortDate(value: string): string {
  return new Date(value).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatBytes(value: number | null | undefined): string {
  if (!value) return "-";
  const units = ["B", "KB", "MB", "GB"];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

function runtimeSettingsToForm(settings: RuntimeSettingsView): RuntimeSettingsForm {
  return {
    public_base_url: settings.public_base_url,
    max_download_mb: String(bytesToMib(settings.max_download_bytes)),
    download_timeout_seconds: String(settings.download_timeout_seconds),
    yt_dlp_timeout_seconds: String(settings.yt_dlp_timeout_seconds),
    yt_dlp_max_json_mb: String(bytesToMib(settings.yt_dlp_max_json_bytes)),
    job_ttl_hours: String(settings.job_ttl_hours),
    page_archive_max_resources: String(settings.page_archive_max_resources ?? 200),
    page_archive_max_resource_mb: String(bytesToMib(settings.page_archive_max_resource_bytes ?? 16 * 1024 * 1024)),
    page_archive_max_total_mb: String(bytesToMib(settings.page_archive_max_total_bytes ?? 200 * 1024 * 1024)),
    page_archive_capture_cdp_enabled: settings.page_archive_capture_cdp_enabled ?? true,
    page_archive_save_mhtml_enabled: settings.page_archive_save_mhtml_enabled ?? true,
    page_archive_save_har_enabled: settings.page_archive_save_har_enabled ?? true,
    page_archive_save_warc_enabled: settings.page_archive_save_warc_enabled ?? true,
    page_archive_cdp_body_max_mb: String(bytesToMib(settings.page_archive_cdp_body_max_bytes ?? 2 * 1024 * 1024)),
    page_archive_cdp_body_total_mb: String(bytesToMib(settings.page_archive_cdp_body_total_bytes ?? 64 * 1024 * 1024)),
    cache_cleanup_min_age_hours: String(settings.cache_cleanup_min_age_hours ?? 24),
  };
}

function bytesToMib(value: number): number {
  return Math.max(1, Math.round(value / 1024 / 1024));
}

function parsePositiveInt(value: string, label: string): number {
  const trimmed = value.trim();
  const parsed = Number(trimmed);
  if (!trimmed || !Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(`${label} 必须是正整数`);
  }
  return parsed;
}

function parseOptionalPositiveInt(value: string, label: string): number | undefined {
  if (!value.trim()) return undefined;
  return parsePositiveInt(value, label);
}

function paginate<T>(items: T[], page: number, pageSize: number): { items: T[]; page: number; start: number } {
  const safePage = clampPage(page, items.length, pageSize);
  const start = (safePage - 1) * pageSize;
  return {
    items: items.slice(start, start + pageSize),
    page: safePage,
    start,
  };
}

function clampPage(page: number, total: number, pageSize: number): number {
  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  return Math.min(Math.max(1, page), pageCount);
}

function capabilityStatus(value: boolean | undefined, apiKey: string): string {
  if (value === true) return "已配置";
  if (value === false) return "未配置";
  return apiKey ? "读取中" : "需要管理密钥";
}

function viewModeLabel(value: ViewMode): string {
  return ({
    console: "控制台",
    admin: "高级设置",
    help: "帮助",
  } as Record<ViewMode, string>)[value];
}

function roleLabel(value: string): string {
  return ({
    admin: "管理密钥",
    user: "用户密钥",
  } as Record<string, string>)[value] ?? value;
}

function statusLabel(value: string): string {
  return ({
    queued: "排队中",
    resolving: "解析中",
    candidates_ready: "待选择",
    candidate_selected: "已提交",
    downloading: "下载中",
    capturing: "捕获中",
    probing: "探测中",
    transcoding: "转码中",
    remuxing: "封装中",
    needs_profile: "需授权",
    ready: "已完成",
    error: "失败",
  } as Record<string, string>)[value] ?? value;
}

function discoveryLabel(value: string): string {
  return ({
    direct: "直链",
    external: "外部解析",
    browser: "浏览器探测",
    auto: "自动解析",
  } as Record<string, string>)[value] ?? value;
}

function platformLabel(value: string): string {
  return ({
    auto: "自动",
    bilibili: "哔哩哔哩",
    youtube: "YouTube",
    soundcloud: "SoundCloud",
    douyin: "抖音",
    kuaishou: "快手",
    pornhub: "Pornhub",
    acfun: "AcFun",
    iqiyi: "爱奇艺",
    youku: "优酷",
    tiktok: "TikTok",
    vimeo: "Vimeo",
    live: "直播/清单",
    generic: "通用",
  } as Record<string, string>)[value] ?? value;
}

function authModeLabel(value: string): string {
  return ({
    auto: "自动",
    none: "无",
    profile: "浏览器配置",
    cookies: "Cookie",
  } as Record<string, string>)[value] ?? value;
}

function outputLabel(value: string): string {
  return ({
    audio: "音频",
    video: "视频",
    image: "图片",
    page_html: "网页包",
  } as Record<string, string>)[value] ?? value;
}

function outputModeLabel(value: string): string {
  return ({
    auto: "自动（媒体）",
    video: "视频",
    audio: "音频",
    image: "图片",
    page_html: "HTML/CSS/JS",
  } as Record<string, string>)[value] ?? value;
}

function cacheCategoryLabel(value: string): string {
  return ({
    public_artifacts: "公开产物",
    temporary_jobs: "临时任务",
    browser_profiles: "浏览器 Profile",
  } as Record<string, string>)[value] ?? value;
}

function artifactLabel(artifact: Artifact): string {
  const url = artifact.media_url.toLowerCase();
  const contentType = artifact.content_type.toLowerCase();
  if (artifact.kind === "page_html") {
    if (url.endsWith("/archive.zip") || contentType.includes("zip")) return "网页前端包";
    if (url.endsWith("/archive.har")) return "HAR 归档";
    if (url.endsWith("/archive.mhtml")) return "MHTML 快照";
    if (url.endsWith("/archive.warc")) return "WARC 归档";
    if (url.endsWith("/index.html")) return "原始入口 HTML";
    if (url.endsWith("/page.html") || contentType.includes("html")) return "入口 HTML";
    if (url.endsWith("/resources.json") || contentType.includes("json")) return "资源清单";
    if (url.endsWith("/page.txt") || contentType.includes("text/plain")) return "页面文本";
    if (url.endsWith("/screenshot.png") || contentType.startsWith("image/")) return "页面截图";
  }
  return outputLabel(artifact.kind);
}

function artifactOpenLabel(artifact: Artifact): string {
  if (isDownloadArtifact(artifact)) return "下载";
  return "打开";
}

function archiveFileLabel(file: ArchiveFileView): string {
  const normalized = file.path.toLowerCase();
  if (normalized === "index.html") return "网页入口";
  if (normalized === "index.inline.html") return "内联预览";
  if (normalized === "page.html") return "原始页面";
  if (normalized === "page.txt") return "页面文本";
  if (normalized === "preview/screenshot.png" || normalized === "screenshot.png") return "页面截图";
  if (normalized === "metadata/resources.json" || normalized === "resources.json") return "资源清单";
  if (normalized === "metadata/archive.har" || normalized === "archive.har") return "HAR 归档";
  if (normalized === "metadata/archive.mhtml" || normalized === "archive.mhtml") return "MHTML 快照";
  if (normalized === "metadata/archive.warc" || normalized === "archive.warc") return "WARC 归档";
  return file.name;
}

function compareArtifacts(left: Artifact, right: Artifact): number {
  return artifactRank(left) - artifactRank(right)
    || left.media_url.localeCompare(right.media_url);
}

function artifactRank(artifact: Artifact): number {
  const url = artifact.media_url.toLowerCase();
  const contentType = artifact.content_type.toLowerCase();
  if (artifact.kind !== "page_html") return contentType.startsWith("video/") ? 10 : contentType.startsWith("audio/") ? 20 : 30;
  if (url.endsWith("/archive.zip") || contentType.includes("zip")) return 0;
  if (url.endsWith("/index.html")) return 1;
  if (url.endsWith("/page.html") || contentType.includes("html")) return 2;
  if (url.endsWith("/screenshot.png") || contentType.startsWith("image/")) return 3;
  if (url.endsWith("/resources.json")) return 4;
  if (url.endsWith("/page.txt") || contentType.includes("text/plain")) return 5;
  if (url.endsWith("/archive.mhtml")) return 20;
  if (url.endsWith("/archive.har")) return 21;
  if (url.endsWith("/archive.warc")) return 22;
  return 40;
}

function compareArchiveFiles(left: ArchiveFileView, right: ArchiveFileView): number {
  return archiveFileRank(left) - archiveFileRank(right)
    || left.path.localeCompare(right.path);
}

function archiveFileRank(file: ArchiveFileView): number {
  const path = file.path.toLowerCase();
  const contentType = file.content_type.toLowerCase();
  if (path === "index.html") return 0;
  if (path === "index.inline.html") return 1;
  if (path === "page.html") return 2;
  if (path === "preview/screenshot.png" || path === "screenshot.png") return 3;
  if (path === "metadata/resources.json" || path === "resources.json") return 4;
  if (path === "page.txt") return 5;
  if (path.startsWith("assets/") && contentType.startsWith("text/css")) return 20;
  if (path.startsWith("assets/") && (contentType.includes("javascript") || path.endsWith(".js"))) return 21;
  if (path.startsWith("assets/") && contentType.startsWith("image/")) return 30;
  if (path.startsWith("assets/") && contentType.startsWith("font/")) return 40;
  if (path.startsWith("metadata/")) return 80;
  return 60;
}

function isDownloadArtifact(artifact: Artifact): boolean {
  const url = artifact.media_url.toLowerCase();
  const contentType = artifact.content_type.toLowerCase();
  return contentType.includes("zip")
    || contentType.includes("warc")
    || contentType.includes("multipart")
    || url.endsWith(".zip")
    || url.endsWith(".warc")
    || url.endsWith(".mhtml")
    || url.endsWith(".har");
}

function jobMediaContentType(job: JobView, artifacts: Artifact[]): string | null {
  if (!job.media_url) return null;
  const matching = artifacts.find((artifact) => artifact.media_url === job.media_url);
  if (matching) return matching.content_type;
  if (job.outputs.includes("page_html")) return "application/zip";
  return null;
}

function outputsLabel(outputs: OutputKind[]): string {
  if (outputs.includes("video") && outputs.includes("audio")) {
    return "媒体";
  }
  return outputs.map(outputLabel).join(", ");
}

function outputsForMode(mode: OutputMode): OutputKind[] {
  if (mode === "auto") {
    return ["video", "audio"];
  }
  return [mode];
}

function isPageArchiveJob(job: JobView | null): boolean {
  return job?.outputs.includes("page_html") ?? false;
}

function bitrateLabel(value: string): string {
  return ({
    auto: "自动（最高可用）",
    "2160p": "4K / 2160p",
    "1440p": "2K / 1440p",
    "1080p": "1080p",
    "720p": "720p",
    "480p": "480p",
    "360p": "360p",
  } as Record<string, string>)[value] ?? value;
}

function candidateDisplayList(candidates: Candidate[], job: JobView | null, showAll: boolean): Candidate[] {
  const ranked = [...candidates].sort((left, right) => candidateRank(right, job) - candidateRank(left, job));
  if (showAll) return ranked;

  const wantedKinds = preferredCandidateKinds(job);
  const primary = ranked.filter((candidate) => wantedKinds.has(candidate.kind) && isUsableCandidate(candidate));
  if (primary.length) return primary.slice(0, 8);
  const fallback = ranked.filter((candidate) => candidate.kind !== "image" && candidate.kind !== "html");
  return fallback.slice(0, 8);
}

function bestCandidate(candidates: Candidate[], job: JobView | null): Candidate | null {
  return candidateDisplayList(candidates, job, false).find(isUsableCandidate) ?? null;
}

function defaultCandidatesForJob(
  candidates: Candidate[],
  job: JobView | null,
  fallback: Candidate | null,
): Candidate[] {
  const outputs = new Set(job?.outputs ?? ["audio"]);
  if (outputs.has("video")) {
    const mediaCandidates = candidateDisplayList(candidates, job, false)
      .filter((candidate) => candidate.kind === "video" || candidate.kind === "manifest")
      .filter(isUsableCandidate);
    if (!mediaCandidates.length) {
      const imageCandidates = candidates.filter((candidate) => candidate.kind === "image");
      if (imageCandidates.length) return imageCandidates;
    }
    const videoCandidate = mediaCandidates[0];
    if (!videoCandidate) return [];
    if (candidateNeedsAudioCompanion(videoCandidate)) {
      const audioCandidate = bestAudioCompanion(videoCandidate, candidates, job);
      if (audioCandidate) {
        return [videoCandidate, audioCandidate];
      }
    }
    return [videoCandidate];
  }
  if (outputs.has("audio") && !outputs.has("video")) {
    const audioCandidate = candidateDisplayList(candidates, job, false)
      .find((candidate) => candidate.kind === "audio" && isUsableCandidate(candidate));
    if (audioCandidate) {
      return [audioCandidate];
    }
  }
  return fallback && isUsableCandidate(fallback) ? [fallback] : [];
}

function candidateNeedsAudioCompanion(candidate: Candidate): boolean {
  if (candidate.kind !== "video") return false;
  const acodec = metadataString(candidate, "acodec");
  const vcodec = metadataString(candidate, "vcodec");
  if (codecPresent(vcodec) && !codecPresent(acodec)) {
    return true;
  }
  const value = `${candidate.url} ${candidate.resource_type ?? ""} ${candidate.quality_label ?? ""}`.toLowerCase();
  return (
    value.includes("bilibili") ||
    value.includes(".m4s") ||
    value.includes("dash") ||
    value.includes("video-only")
  );
}

function metadataString(candidate: Candidate, key: string): string {
  const metadata = candidate.metadata_json as Record<string, unknown> | undefined;
  const nested = metadata?.candidate as Record<string, unknown> | undefined;
  const value = metadata?.[key] ?? nested?.[key];
  return typeof value === "string" ? value.toLowerCase() : "";
}

function codecPresent(value: string): boolean {
  return Boolean(value && !["none", "null", "unknown"].includes(value));
}

function bestAudioCompanion(
  videoCandidate: Candidate,
  candidates: Candidate[],
  job: JobView | null,
): Candidate | null {
  const videoFamily = candidateFamily(videoCandidate);
  const audioCandidates = candidates
    .filter((candidate) => candidate.kind === "audio")
    .filter(isUsableCandidate)
    .filter((candidate) => candidateFamily(candidate) === videoFamily || isBilibiliFamily(videoCandidate, candidate));

  if (!audioCandidates.length) return null;
  return audioCandidates.sort((left, right) => candidateRank(right, job) - candidateRank(left, job))[0] ?? null;
}

function candidateFamily(candidate: Candidate): string {
  const url = safeUrl(candidate.url);
  if (candidate.resource_type === "bilibili_playinfo" || candidate.resource_type === "bilibili_api") {
    return `${candidate.extractor}:bilibili`;
  }
  return `${candidate.extractor}:${candidate.initiator_url ?? url?.hostname ?? candidate.platform ?? "unknown"}`;
}

function isBilibiliFamily(left: Candidate, right: Candidate): boolean {
  const leftValue = `${left.url} ${left.resource_type ?? ""} ${left.quality_label ?? ""}`.toLowerCase();
  const rightValue = `${right.url} ${right.resource_type ?? ""} ${right.quality_label ?? ""}`.toLowerCase();
  return leftValue.includes("bilibili") && rightValue.includes("bilibili") && left.extractor === right.extractor;
}

function candidateRank(candidate: Candidate, job: JobView | null): number {
  let rank = candidate.score;
  const outputs = new Set(job?.outputs ?? ["audio"]);

  if (outputs.has("audio")) {
    if (candidate.kind === "audio") rank += 1000;
    if (candidate.kind === "manifest") rank += 650;
    if (candidate.kind === "video") rank += 450;
    if (candidate.kind === "image" || candidate.kind === "html") rank -= 1000;
  }

  if (outputs.has("video")) {
    if (candidate.kind === "video" || candidate.kind === "manifest") rank += 1000;
    if (candidate.kind === "image") rank += 700;
    if (candidate.kind === "audio") rank += 50;
    rank += mp4CompatibilityRank(candidate);
  }

  if (outputs.has("image") && candidate.kind === "image") rank += 900;
  if (candidate.requires_authorization) rank -= 50;
  const height = qualityFromUrl(candidate.url) ?? candidate.quality_label;
  const parsedHeight = height?.match(/([1-9]\d{2,3})p/i)?.[1];
  if (parsedHeight) rank += Math.min(Number(parsedHeight), 2160) / 2;
  if (isLikelyAdCandidate(candidate)) rank -= 5000;
  if (candidate.validation_state === "usable") rank += 500;
  if (candidate.protection === "drm" || candidate.validation_state === "drm") rank -= 8000;
  if (candidate.validation_state === "expired" || candidate.validation_state === "region_blocked") rank -= 6000;
  if ((candidate.evidence_count ?? 1) > 1) rank += Math.min((candidate.evidence_count ?? 1) * 25, 150);
  if (candidate.validation_status?.startsWith("failed")) rank -= 4000;
  return rank;
}

function mp4CompatibilityRank(candidate: Candidate): number {
  const metadata = candidate.metadata_json as Record<string, unknown> | undefined;
  const nested = metadata?.candidate as Record<string, unknown> | undefined;
  const value = [
    candidate.content_type,
    candidate.url,
    metadata?.ext,
    metadata?.vcodec,
    metadata?.acodec,
    nested?.ext,
    nested?.vcodec,
    nested?.acodec,
  ].filter(Boolean).join(" ").toLowerCase();
  let rank = 0;
  if (value.includes("video/mp4") || value.includes(".mp4") || value.includes(" mp4 ")) rank += 300;
  if (value.includes("avc1") || value.includes("h264")) rank += 300;
  if (value.includes("mp4a") || value.includes("aac")) rank += 120;
  if (value.includes("video/webm") || value.includes(".webm") || value.includes("vp9") || value.includes("vp09") || value.includes("av01")) rank -= 700;
  if (value.includes("opus") || value.includes("vorbis")) rank -= 180;
  return rank;
}

function isUsableCandidate(candidate: Candidate): boolean {
  if (isLikelyAdCandidate(candidate)) return false;
  if (candidate.validation_status?.startsWith("failed")) return false;
  if (candidate.failure_reason) return false;
  if (["drm", "expired", "failed", "region_blocked", "suspect_ad"].includes(candidate.validation_state ?? "")) return false;
  if (["drm", "region_blocked"].includes(candidate.protection ?? "")) return false;
  return true;
}

function preferredCandidateKinds(job: JobView | null): Set<string> {
  const outputs = new Set(job?.outputs ?? ["audio"]);
  if (outputs.has("audio") && !outputs.has("video")) {
    return new Set(["audio", "manifest", "video"]);
  }
  if (outputs.has("video")) {
    return new Set(["video", "manifest", "image", "audio"]);
  }
  if (outputs.has("image")) {
    return new Set(["image"]);
  }
  return new Set(["audio", "video", "manifest"]);
}

function candidateMeta(candidate: Candidate): string {
  return [
    extractorLabel(candidate.extractor),
    candidate.quality_label,
    candidate.content_type,
    candidate.resource_type,
  ].filter(Boolean).join(" / ") || "媒体资源";
}

function candidateSummary(candidate: Candidate): {
  quality: string;
  source: string;
  kindDetail: string;
  adRisk: boolean;
} {
  const url = safeUrl(candidate.url);
  const path = url?.pathname.toLowerCase() ?? candidate.url.toLowerCase();
  const host = url?.hostname ?? "-";
  const quality = candidate.quality_label ?? qualityFromUrl(candidate.url) ?? "未知质量";
  const source = candidate.resource_type?.replace(/^inline_/, "页面脚本/")
    .replace(/^performance_/, "页面请求/")
    .replace(/^dom_/, "页面元素/")
    ?? host;
  const format = candidate.content_type ?? extensionFromPath(path) ?? "未知格式";
  const kindDetail = candidate.kind === "manifest"
    ? `流媒体清单 / ${format}`
    : `${candidateKindLabel(candidate.kind)} / ${format}`;
  return {
    quality,
    source,
    kindDetail,
    adRisk: isLikelyAdCandidate(candidate),
  };
}

function safeUrl(value: string): URL | null {
  try {
    return new URL(value);
  } catch {
    return null;
  }
}

function sourceInputIssue(value: string, publicBaseUrl?: string): string | null {
  const parsed = safeUrl(value.trim());
  if (!parsed) return null;
  const path = parsed.pathname.toLowerCase();
  const base = publicBaseUrl ? safeUrl(publicBaseUrl) : null;
  const isSameService = parsed.origin === window.location.origin || parsed.origin === base?.origin;
  if (isSameService && (path.startsWith("/media/") || path.startsWith("/api/jobs/"))) {
    return "这是 Reflection King 已生成的产物/API 地址，不是要解析的源网页。请下载或打开产物，或粘贴原始公网页面 URL。";
  }
  if (isSameService) {
    return "这是当前 Reflection King 服务地址，不是源网页。请粘贴 Steam、视频站或普通网页的原始公网 URL。";
  }
  return null;
}

function qualityFromUrl(value: string): string | null {
  const match = value.match(/(?:^|[^\d])([1-9]\d{2,3})p(?:[^\d]|$)/i);
  return match ? `${match[1]}p` : null;
}

function extensionFromPath(path: string): string | null {
  const match = path.match(/\.([a-z0-9]{2,5})$/i);
  return match ? match[1].toUpperCase() : null;
}

function isLikelyAdCandidate(candidate: Candidate): boolean {
  if (candidate.ad_risk) return true;
  const value = `${candidate.url} ${candidate.resource_type ?? ""}`.toLowerCase();
  return [
    "trafficjunky",
    "doubleclick",
    "googlesyndication",
    "adservice",
    "preroll",
    "pre-roll",
    "vast",
    "vpaid",
    "tracking",
    "tracker",
    "pixel",
  ].some((needle) => value.includes(needle));
}

function validationLabel(value: string): string {
  if (value === "usable") return "已验证可用";
  if (value === "needs_profile") return "需要授权";
  if (value === "suspect_ad") return "疑似广告";
  if (value === "expired") return "已过期";
  if (value === "drm") return "DRM/受保护";
  if (value === "region_blocked") return "地区限制";
  if (value === "untested") return "未验证";
  if (value === "ok") return "已验证可用";
  if (value.startsWith("failed:")) return `不可用：${friendlyError(value.slice("failed:".length).trim())}`;
  return value;
}

function protectionLabel(value: string): string {
  return ({
    needs_profile: "需要 Profile",
    signed_url: "签名链接",
    drm: "DRM",
    region_blocked: "地区限制",
    unknown: "保护未知",
  } as Record<string, string>)[value] ?? value;
}

function routeLabel(value: string): string {
  return value
    .replace(/^external:/, "外部/")
    .replace("browser_probe", "浏览器")
    .replace("yt_dlp", "yt-dlp")
    .replace("you_get", "you-get")
    .replace("streamlink", "Streamlink")
    .replace("direct", "直链");
}

function qualityAvailabilityLabel(candidates: Candidate[], preference: string): string {
  const qualities = Array.from(
    new Set(
      candidates
        .map((candidate) => candidate.quality_label ?? qualityFromUrl(candidate.url))
        .filter((value): value is string => Boolean(value && /^\d{3,4}p$/i.test(value))),
    ),
  ).sort((left, right) => qualityNumber(right) - qualityNumber(left));

  if (!qualities.length) {
    return "未识别清晰度，将按可用媒体排序。";
  }
  const advertised = Math.max(0, ...candidates.map((candidate) => candidateAvailability(candidate).highestAdvertisedHeight ?? 0));
  const highestVisible = qualityNumber(qualities[0]);
  if (advertised > highestVisible) {
    return `当前可用最高 ${qualities[0]}；站点提示最高 ${advertised}p，通常需要登录 Profile。`;
  }
  if (preference === "auto") {
    return `自动选择最高可用：${qualities[0]}。`;
  }
  if (qualities.includes(preference)) {
    return `目标清晰度可用：${preference}。`;
  }
  return `目标 ${preference} 不可用，可用：${qualities.join("、")}。`;
}

function qualityNumber(value: string): number {
  return Number(value.match(/(\d{3,4})p/i)?.[1] ?? 0);
}

function candidateAvailability(candidate: Candidate): {
  higherQualityRequiresProfile: boolean;
  highestAdvertisedHeight?: number;
} {
  const metadata = candidate.metadata_json as CandidateMetadata | undefined;
  const raw = metadata?.candidate;
  const highestAdvertisedHeight = Number(raw?.highestAdvertisedHeight ?? 0);
  return {
    higherQualityRequiresProfile: Boolean(raw?.higherQualityRequiresProfile),
    highestAdvertisedHeight: Number.isFinite(highestAdvertisedHeight) && highestAdvertisedHeight > 0
      ? highestAdvertisedHeight
      : undefined,
  };
}

function extractorLabel(value: string): string {
  return ({
    browser_probe: "浏览器抓取",
    yt_dlp: "站点解析",
  } as Record<string, string>)[value] ?? value;
}

function candidateKindLabel(value: string): string {
  return ({
    audio: "音频",
    video: "视频",
    image: "图片",
    manifest: "清单",
    html: "HTML",
    unknown: "未知",
  } as Record<string, string>)[value] ?? value;
}

function sourceTitle(value: string): string {
  try {
    const url = new URL(value);
    return `${url.hostname}${url.pathname === "/" ? "" : url.pathname}`;
  } catch {
    return value;
  }
}

function compactUrl(value: string): string {
  try {
    const url = new URL(value);
    return `${url.hostname}${url.pathname}${url.search ? "?" : ""}`;
  } catch {
    return value;
  }
}

type JobIssueKind = "cookie" | "dependency" | "unsupported" | "profile" | "resolver" | "timeout" | "policy" | "error";

interface JobIssue {
  kind: JobIssueKind;
  label: string;
  tone: "error" | "warn" | "info";
}

interface JobStats {
  total: number;
  ready: number;
  candidates: number;
  running: number;
  error: number;
  cookie: number;
  dependency: number;
  unsupported: number;
}

function summarizeJobs(items: JobView[]): JobStats {
  const stats: JobStats = {
    total: items.length,
    ready: 0,
    candidates: 0,
    running: 0,
    error: 0,
    cookie: 0,
    dependency: 0,
    unsupported: 0,
  };
  for (const item of items) {
    if (item.status === "ready") {
      stats.ready += 1;
    } else if (item.status === "candidates_ready") {
      stats.candidates += 1;
    } else if (item.status === "needs_profile" || item.status === "error") {
      stats.error += 1;
      const issue = jobIssue(item);
      if (issue?.kind === "cookie" || issue?.kind === "profile") stats.cookie += 1;
      if (issue?.kind === "dependency") stats.dependency += 1;
      if (issue?.kind === "unsupported") stats.unsupported += 1;
    } else {
      stats.running += 1;
    }
  }
  return stats;
}

function jobIssue(job: JobView | null): JobIssue | null {
  if (job?.issue_kind && job.issue_kind !== "none") {
    const map: Record<NonNullable<JobView["issue_kind"]>, JobIssue | null> = {
      none: null,
      failed: { kind: "error", label: "失败", tone: "error" },
      needs_profile: { kind: "profile", label: "需授权", tone: "warn" },
      unsupported: { kind: "unsupported", label: "待适配", tone: "info" },
      too_large: { kind: "error", label: "过大", tone: "warn" },
      timeout: { kind: "timeout", label: "超时", tone: "warn" },
      policy_blocked: { kind: "policy", label: "策略阻止", tone: "warn" },
    };
    return map[job.issue_kind];
  }
  if (!job?.error) return null;
  const lower = `${job.source_url} ${job.error}`.toLowerCase();
  if (lower.includes("fresh cookies") || lower.includes("sign in") || lower.includes("login required")) {
    return { kind: "cookie", label: "需 Cookie", tone: "warn" };
  }
  if (lower.includes("phantomjs")) {
    return { kind: "dependency", label: "缺依赖", tone: "warn" };
  }
  if (
    lower.includes("unsupported url") ||
    lower.includes("kuaishou") ||
    lower.includes("no media candidates from chain")
  ) {
    return { kind: "unsupported", label: "待适配", tone: "info" };
  }
  if (
    lower.includes("requires headers") ||
    lower.includes("requires authorization") ||
    lower.includes("profile")
  ) {
    return { kind: "profile", label: "需授权", tone: "warn" };
  }
  if (lower.includes("timed out")) {
    return { kind: "timeout", label: "超时", tone: "warn" };
  }
  if (lower.includes("url policy denied request")) {
    return { kind: "policy", label: "策略阻止", tone: "warn" };
  }
  if (lower.includes("yt-dlp probe exited") || lower.includes("external resolver")) {
    return { kind: "resolver", label: "解析器失败", tone: "error" };
  }
  return { kind: "error", label: "失败", tone: "error" };
}

function friendlyError(value: string, job?: JobView | null): string {
  if (!value) return "-";
  const lower = `${job?.source_url ?? ""} ${value}`.toLowerCase();

  if (lower.includes("url policy denied request") && lower.includes("blocked address")) {
    return "URL 安全策略阻止了这个地址：目标解析到内网、localhost、保留地址或链路本地地址。请提交原始公网 URL，不要把本服务的 192.168/127.0.0.1 产物地址再作为解析来源。";
  }
  if (lower.includes("steam") && lower.includes("no media candidates")) {
    return "Steam 商店页通常是网页内容，不是直连视频/音频媒体页。请选择 HTML/CSS/JS 输出保存网页前端包；如果要下载商店视频，需要页面里真实公开的媒体 URL 或后续专门适配。";
  }
  if (lower.includes("fresh cookies")) {
    return "该链接需要 fresh cookies。请导入对应站点 Cookie/Profile 后重试；这不是媒体管线损坏。";
  }
  if (
    lower.includes("cloudflare") ||
    lower.includes("turnstile") ||
    lower.includes("captcha") ||
    lower.includes("human verification") ||
    lower.includes("security challenge") ||
    lower.includes("security verification")
  ) {
    if (isPageArchiveJob(job ?? null)) {
      return "站点提示登录或安全验证；如果只需要未登录主页 UI 和公开资源，请点“强制解析”。";
    }
    return "站点正在进行安全验证。请打开验证浏览器，像远程桌面一样完成真人验证或登录确认，然后继续解析。";
  }
  if (lower.includes("phantomjs")) {
    return "爱奇艺/iQ.com 当前解析器需要 PhantomJS 兼容依赖；这是待补依赖/适配项。";
  }
  if (lower.includes("kuaishou") && lower.includes("unsupported url")) {
    return "快手当前样本还没有可用自动适配器；yt-dlp 不支持，浏览器探测也未抓到媒体。";
  }
  if (lower.includes("kuaishou") && lower.includes("no media candidates")) {
    return "快手当前样本所有解析链路都未发现可用媒体候选，属于待适配站点。";
  }
  if (lower.includes("no media candidates from chain")) {
    return "所有解析链路都没有找到可用媒体候选。可尝试登录 Profile、切换浏览器探测，或把该站点列入新适配。";
  }
  if (lower.includes("browser probe did not find media candidates")) {
    return "浏览器探测没有发现可用媒体资源";
  }
  if (lower.includes("deprecated feature")) {
    return "yt-dlp 站点规则过期，需要更新 yt-dlp 或更换解析方式";
  }
  if (lower.includes("[bilibili]") && (lower.includes("412") || lower.includes("error"))) {
    return "哔哩哔哩拒绝外部解析请求，建议改用浏览器探测";
  }
  if (lower.includes("raw candidate failed") && lower.includes("delegated")) {
    return "原始媒体地址被站点拒绝，系统已尝试 yt-dlp 代理下载 fallback 但仍失败。";
  }
  if (lower.includes("yt-dlp probe exited")) {
    return "外部解析失败，建议更新 yt-dlp 或改用浏览器探测";
  }
  if (lower.includes("timed out")) {
    return "解析超时";
  }
  if (lower.includes("requires headers") || lower.includes("requires authorization") || lower.includes("profile")) {
    if (isPageArchiveJob(job ?? null)) {
      return "当前任务是网页包；如果只需要未登录页面，请点“强制解析”重新保存 HTML/CSS/JS。";
    }
    return "资源需要登录态、页面授权或安全验证。请打开验证浏览器，使用共享 Profile 处理后继续解析。";
  }
  return value.length > 140 ? `${value.slice(0, 140)}...` : value;
}

async function copy(value: string): Promise<boolean> {
  let ok = false;
  try {
    await navigator.clipboard.writeText(value);
    ok = true;
  } catch {
    const textarea = document.createElement("textarea");
    textarea.value = value;
    textarea.setAttribute("readonly", "true");
    textarea.style.position = "fixed";
    textarea.style.left = "-9999px";
    textarea.style.top = "0";
    document.body.appendChild(textarea);
    textarea.select();
    textarea.setSelectionRange(0, textarea.value.length);
    try {
      ok = document.execCommand("copy");
    } finally {
      textarea.remove();
    }
  }
  window.dispatchEvent(new CustomEvent("reflection-copy-result", { detail: { ok } }));
  return ok;
}

createRoot(document.getElementById("root")!).render(<App />);
