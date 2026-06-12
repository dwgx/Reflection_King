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
  Eye,
  ExternalLink,
  FileAudio,
  History,
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
type ViewMode = "console" | "history" | "admin" | "help";
type LoginClickMode = "left" | "right" | "double";

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
  };
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
  candidate?: {
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

interface ConfirmDialogState {
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: () => Promise<void> | void;
}

const OUTPUTS: OutputKind[] = ["audio", "video", "image", "page_html"];
const TERMINAL = new Set(["ready", "error", "candidates_ready"]);
const PAGE_SIZE_OPTIONS = [3, 5, 10, 20, 50];

function App() {
  const [apiKey, setApiKey] = useState(() => localStorage.getItem("reflection_api_key") ?? "");
  const [health, setHealth] = useState<Health | null>(null);
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null);
  const [jobs, setJobs] = useState<JobView[]>([]);
  const [selectedJobId, setSelectedJobId] = useState<string>("");
  const [selectedJob, setSelectedJob] = useState<JobView | null>(null);
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [artifacts, setArtifacts] = useState<Artifact[]>([]);
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
  const notificationIdRef = useRef(0);
  const [userKeys, setUserKeys] = useState<UserKeyView[]>([]);
  const [newUserKey, setNewUserKey] = useState("");
  const [newAdminKey, setNewAdminKey] = useState("");
  const [keyForm, setKeyForm] = useState({
    label: "普通用户",
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
  const [loginClickMode, setLoginClickMode] = useState<LoginClickMode>("left");
  const [loginZoom, setLoginZoom] = useState("1");
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
    if (viewMode === "history" && apiKey) {
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

  const pagedCandidates = useMemo(
    () => paginate(visibleCandidates, candidatePage, candidatePageSize),
    [visibleCandidates, candidatePage, candidatePageSize],
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
    setBusy(true);
    setMessage("正在创建任务...");
    try {
      const payload = { ...form, url: form.url.trim() };
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
    setSelectedCandidates(new Set());
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
    setForm({ ...form, outputs: outputsForMode(mode) });
  }

  function toggleCandidate(id: string) {
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
      const response = await request<CreatedUserKeyResponse>("/api/admin/user-keys", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(keyForm),
      });
      setNewUserKey(response.key);
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
    askConfirm({
      title: "轮换管理员密钥",
      message: "旧管理员密钥会立即失效。新管理员密钥只显示一次，并会自动填入当前页面。",
      confirmLabel: "轮换密钥",
      danger: true,
      onConfirm: async () => {
        setBusy(true);
        setNewAdminKey("");
        try {
          const response = await request<RotatedAdminKeyResponse>("/api/admin/admin-key/rotate", {
            method: "POST",
          });
          setNewAdminKey(response.key);
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

  async function startBrowserLoginSession() {
    setBusy(true);
    try {
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

  async function refreshBrowserLoginSession() {
    if (!loginSnapshot) return;
    setBusy(true);
    try {
      setLoginSnapshot(await request<BrowserLoginSnapshot>(
        `/api/admin/browser-login-sessions/${encodeURIComponent(loginSnapshot.session.id)}/snapshot`,
      ));
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  function browserPointFromEvent(event: React.MouseEvent<HTMLElement> | React.WheelEvent<HTMLElement>) {
    if (!loginSnapshot) return;
    const rect = event.currentTarget.getBoundingClientRect();
    return {
      x: ((event.clientX - rect.left) / rect.width) * loginSnapshot.width,
      y: ((event.clientY - rect.top) / rect.height) * loginSnapshot.height,
    };
  }

  async function clickBrowserLoginSession(event: React.MouseEvent<HTMLButtonElement>, mode = loginClickMode) {
    if (!loginSnapshot) return;
    const point = browserPointFromEvent(event);
    if (!point) return;
    const button = mode === "right" ? "right" : "left";
    const clickCount = mode === "double" ? 2 : 1;
    try {
      setLoginSnapshot(await request<BrowserLoginSnapshot>(
        `/api/admin/browser-login-sessions/${encodeURIComponent(loginSnapshot.session.id)}/click`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ ...point, button, click_count: clickCount }),
        },
      ));
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  }

  async function wheelBrowserLoginSession(event: React.WheelEvent<HTMLButtonElement>) {
    if (!loginSnapshot) return;
    event.preventDefault();
    const point = browserPointFromEvent(event);
    try {
      setLoginSnapshot(await request<BrowserLoginSnapshot>(
        `/api/admin/browser-login-sessions/${encodeURIComponent(loginSnapshot.session.id)}/wheel`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            delta_x: event.deltaX,
            delta_y: event.deltaY,
            x: point?.x,
            y: point?.y,
          }),
        },
      ));
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  }

  async function resizeBrowserLoginSession(width: number, height: number) {
    if (!loginSnapshot) return;
    setBusy(true);
    try {
      setLoginSnapshot(await request<BrowserLoginSnapshot>(
        `/api/admin/browser-login-sessions/${encodeURIComponent(loginSnapshot.session.id)}/resize`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ width, height }),
        },
      ));
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
      setLoginSnapshot(await request<BrowserLoginSnapshot>(
        `/api/admin/browser-login-sessions/${encodeURIComponent(loginSnapshot.session.id)}/type`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ text: loginText }),
        },
      ));
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
      setLoginSnapshot(await request<BrowserLoginSnapshot>(
        `/api/admin/browser-login-sessions/${encodeURIComponent(loginSnapshot.session.id)}/press`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ key }),
        },
      ));
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
      setLoginSnapshot(await request<BrowserLoginSnapshot>(
        `/api/admin/browser-login-sessions/${encodeURIComponent(loginSnapshot.session.id)}/navigate`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ url: loginUrl }),
        },
      ));
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
    }
  }

  async function closeBrowserLoginSession() {
    if (!loginSnapshot) return;
    setBusy(true);
    try {
      await request(`/api/admin/browser-login-sessions/${encodeURIComponent(loginSnapshot.session.id)}/close`, {
        method: "POST",
      });
      setLoginSnapshot(null);
      notify("已关闭服务端浏览器会话，Profile 已保留登录态。", "success");
    } catch (error) {
      notify(errorMessage(error), "error");
    } finally {
      setBusy(false);
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
                onChange={(event) => setForm({ ...form, url: event.target.value })}
              />
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
            </ControlGroup>
            <ControlGroup label="授权">
              <Dropdown
                value={form.auth_mode}
                options={["auto", "none", "profile", "cookies"]}
                labelFor={authModeLabel}
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
              <Button variant="secondary" onClick={restoreHiddenJobs} disabled={busy}>
                <Eye size={16} /> 恢复
              </Button>
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
          <div className="panel-scroll-layout">
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
                  <span className="task-status"><Badge status={job.status} /></span>
                  <span className="task-source">
                    <strong>{sourceTitle(job.source_url)}</strong>
                    <small>{job.error ? friendlyError(job.error) : job.source_url}</small>
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

        <Card title="任务详情" icon={<Settings size={16} />} className="dashboard-panel" bodyClassName="dashboard-panel-body">
          {selectedJob ? (
            <div className="detail-layout">
              <div className="detail-title-row">
                <Badge status={selectedJob.status} />
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
                <MetaLine label="播放地址" value={selectedJob.media_url ?? "-"} copyable />
              </div>
              {selectedJob.error && (
                <div className="error-line">
                  <AlertTriangle size={16} />
                  <span>{friendlyError(selectedJob.error)}</span>
                </div>
              )}
              {selectedJob.media_url && <Player url={selectedJob.media_url} />}
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
          action={
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
          }
        >
          {candidates.length ? (
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
          {artifacts.length ? (
            <div className="artifact-list">
              {artifacts.map((artifact) => (
                <div key={artifact.id} className="artifact-item">
                  <div className="artifact-head">
                    <div>
                      <strong>{outputLabel(artifact.kind)}</strong>
                      <span>{artifact.content_type} / {formatBytes(artifact.bytes)}</span>
                    </div>
                    <div className="panel-actions">
                      <Button variant="secondary" onClick={() => copy(artifact.media_url)}>
                        <Clipboard size={16} /> 复制
                      </Button>
                      <a className="button secondary" href={artifact.media_url} target="_blank" rel="noreferrer">
                        <ExternalLink size={16} /> 打开
                      </a>
                    </div>
                  </div>
                  <Player url={artifact.media_url} />
                </div>
              ))}
            </div>
          ) : (
            <Empty label="暂无产物" />
          )}
        </Card>
      </section>
    </div>
  );

  const adminView = (
    <div className="view-stack">
      <Card
        title="管理员密钥"
        icon={<Shield size={16} />}
        action={
          <Button variant="secondary" onClick={rotateAdminKey} disabled={busy || !isAdmin}>
            <RefreshCw size={16} /> 轮换管理员密钥
          </Button>
        }
      >
        <div className="admin-key-panel">
          <div>
            <strong>当前管理权限</strong>
            <span>{capabilities?.auth ? `${roleLabel(capabilities.auth.role)} / ${capabilities.auth.label}` : "未确认"}</span>
          </div>
          <p>轮换后旧管理员密钥立即失效。新密钥只在这里显示一次，并会自动填入顶部密钥输入框。</p>
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

      <section className="admin-grid">
        <Card
          title="用户密钥"
          icon={<KeyRound size={16} />}
          action={<Button variant="secondary" onClick={refreshUserKeys}><RefreshCw size={16} /> 刷新</Button>}
        >
          <form className="admin-form" onSubmit={createUserKey}>
            <Field label="名称">
              <Input value={keyForm.label} onChange={(event) => setKeyForm({ ...keyForm, label: event.target.value })} />
            </Field>
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
                  <span>点击截图操作服务器上的持久 Profile。登录态只保存在服务器 Profile，不返回 Cookie 明文。</span>
                </div>
                <Button type="button" disabled={busy} onClick={startBrowserLoginSession}>
                  <MonitorPlay size={16} /> 打开会话
                </Button>
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
                    <SegmentedControl
                      value={loginClickMode}
                      options={["left", "right", "double"]}
                      labelFor={loginClickModeLabel}
                      onChange={(value) => setLoginClickMode(value as LoginClickMode)}
                    />
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
                      onClick={clickBrowserLoginSession}
                      onWheel={wheelBrowserLoginSession}
                      onContextMenu={(event) => {
                        event.preventDefault();
                        setLoginClickMode("right");
                        void clickBrowserLoginSession(event, "right");
                      }}
                    >
                      <img src={loginSnapshot.image} alt="服务端浏览器截图" draggable={false} />
                    </button>
                  </div>
                  <div className="screen-help">
                    <span>点击截图按当前模式操作；滚轮会发送到服务端页面；右键截图会直接发送右键点击。</span>
                  </div>
                  <div className="remote-login-controls">
                    <Input
                      value={loginText}
                      placeholder="输入要发送到当前焦点的文本"
                      onChange={(event) => setLoginText(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") {
                          event.preventDefault();
                          void typeIntoBrowserLoginSession();
                        }
                      }}
                    />
                    <Button type="button" variant="secondary" disabled={busy || !loginText} onClick={typeIntoBrowserLoginSession}>
                      输入
                    </Button>
                    <Button type="button" variant="secondary" disabled={busy} onClick={() => pressBrowserLoginKey("Enter")}>
                      Enter
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

  const historyView = (
    <div className="view-stack">
      {!apiKey && (
        <div className="notice-strip warn">
          填写管理密钥或用户密钥后可以查看当前密钥可见的隐藏批次。
        </div>
      )}
      <Card
        title="隐藏历史"
        icon={<History size={16} />}
        action={
          <div className="panel-actions">
            <Button variant="secondary" onClick={restoreHiddenJobs} disabled={busy}>
              <Eye size={16} /> 恢复上一批
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
    { mode: "history", icon: <History size={16} /> },
    ...(isAdmin ? [{ mode: "admin" as ViewMode, icon: <Shield size={16} /> }] : []),
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
                if (item.mode === "admin" && isAdmin) void refreshUserKeys();
                if (item.mode === "history" && apiKey) void refreshHiddenBatches();
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
            <span>{formatBytes(health?.max_download_bytes)}</span>
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
          {viewMode === "history" && historyView}
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
        {props.action}
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
  const { className = "", variant = "primary", ...rest } = props;
  return <button className={`button ${variant} ${className}`} {...rest} />;
}

function Dropdown(props: {
  value: string;
  options: string[];
  labelFor?: (value: string) => string;
  onChange: (value: string) => void;
  className?: string;
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
      <button className="custom-select-trigger" type="button" onClick={() => setOpen(!open)}>
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
      }`}
    >
      <input
        type="checkbox"
        checked={props.selected}
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

function Empty(props: { label: string }) {
  return <div className="empty-state">{props.label}</div>;
}

function Player(props: { url: string }) {
  const lower = props.url.toLowerCase();
  if (lower.endsWith(".mp4")) {
    return <video className="player" controls src={props.url} />;
  }
  return <audio className="player" controls src={props.url} />;
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
    history: "隐藏历史",
    admin: "管理",
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

function loginClickModeLabel(value: string): string {
  return ({
    left: "左键",
    right: "右键",
    double: "双击",
  } as Record<string, string>)[value] ?? value;
}

function outputLabel(value: string): string {
  return ({
    audio: "音频",
    video: "视频",
    image: "图片",
    page_html: "页面 HTML",
  } as Record<string, string>)[value] ?? value;
}

function outputModeLabel(value: string): string {
  return ({
    auto: "自动（媒体）",
    video: "视频",
    audio: "音频",
    image: "图片",
    page_html: "页面 HTML",
  } as Record<string, string>)[value] ?? value;
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
  const primary = ranked.filter((candidate) => wantedKinds.has(candidate.kind));
  const usable = primary.length ? primary : ranked.filter((candidate) => candidate.kind !== "image" && candidate.kind !== "html");
  return usable.slice(0, 8);
}

function bestCandidate(candidates: Candidate[], job: JobView | null): Candidate | null {
  return candidateDisplayList(candidates, job, false)[0] ?? null;
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
  return fallback ? [fallback] : [];
}

function candidateNeedsAudioCompanion(candidate: Candidate): boolean {
  if (candidate.kind !== "video") return false;
  const value = `${candidate.url} ${candidate.resource_type ?? ""} ${candidate.quality_label ?? ""}`.toLowerCase();
  return (
    value.includes("bilibili") ||
    value.includes(".m4s") ||
    value.includes("dash") ||
    value.includes("video-only")
  );
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

function isUsableCandidate(candidate: Candidate): boolean {
  if (isLikelyAdCandidate(candidate)) return false;
  if (candidate.validation_status?.startsWith("failed")) return false;
  if (candidate.failure_reason) return false;
  if (["drm", "expired", "region_blocked"].includes(candidate.validation_state ?? "")) return false;
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

function friendlyError(value: string): string {
  if (!value) return "-";
  const lower = value.toLowerCase();

  if (lower.includes("browser probe did not find media candidates")) {
    return "浏览器探测没有发现可用媒体资源";
  }
  if (lower.includes("deprecated feature")) {
    return "yt-dlp 站点规则过期，需要更新 yt-dlp 或更换解析方式";
  }
  if (lower.includes("[bilibili]") && (lower.includes("412") || lower.includes("error"))) {
    return "哔哩哔哩拒绝外部解析请求，建议改用浏览器探测";
  }
  if (lower.includes("yt-dlp probe exited")) {
    return "外部解析失败，建议更新 yt-dlp 或改用浏览器探测";
  }
  if (lower.includes("timed out")) {
    return "解析超时";
  }
  if (lower.includes("requires headers") || lower.includes("requires authorization")) {
    return "资源需要登录态或页面授权";
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
