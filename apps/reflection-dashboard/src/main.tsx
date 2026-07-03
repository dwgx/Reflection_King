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
import "./styles/slider.css";

import type {
  ArchiveFileView,
  ArchiveTreeView,
  Artifact,
  BrowserLoginSnapshot,
  BrowserMouseButton,
  CacheCleanupView,
  CacheInventoryView,
  Candidate,
  Capabilities,
  ClearJobsResponse,
  ConfirmDialogState,
  CreatedUserKeyResponse,
  CreateJobPayload,
  DiscoveryMode,
  FilePreviewState,
  Health,
  HiddenJobBatchView,
  JobView,
  NotificationItem,
  OutputKind,
  OutputMode,
  PlatformHint,
  RestoreJobsResponse,
  RotatedAdminKeyResponse,
  RuntimeSettingsForm,
  RuntimeSettingsView,
  UserKeyView,
  ViewMode,
} from "./types";
import {
  EMPTY_RUNTIME_FORM,
  FALLBACK_PLATFORM_HINTS,
  LOGIN_TARGETS,
  OUTPUTS,
  PAGE_SIZE_OPTIONS,
  TERMINAL,
  WEB_ARCHIVE_PLATFORM_HINTS,
} from "./constants";
import {
  apiHeaders,
  archiveFileLabel,
  artifactLabel,
  artifactOpenLabel,
  authModeLabel,
  bestCandidate,
  bitrateLabel,
  cacheCategoryLabel,
  candidateAvailability,
  candidateDisplayList,
  candidateKindLabel,
  candidateMeta,
  candidateNeedsAudioCompanion,
  candidateSummary,
  capabilityStatus,
  clampPage,
  compactUrl,
  compareArchiveFiles,
  compareArtifacts,
  copy,
  defaultCandidatesForJob,
  discoveryLabel,
  errorMessage,
  extractorLabel,
  formatBytes,
  formatShortDate,
  friendlyError,
  isDownloadArtifact,
  isPageArchiveJob,
  isUsableCandidate,
  jobIssue,
  jobMediaContentType,
  normalizeSourceInput,
  outputLabel,
  outputModeLabel,
  outputsForMode,
  outputsLabel,
  paginate,
  parseOptionalPositiveInt,
  parsePositiveInt,
  platformHintForSourceUrl,
  platformLabel,
  protectionLabel,
  qualityAvailabilityLabel,
  roleLabel,
  routeLabel,
  runtimeSettingsToForm,
  safeUrl,
  sourceInputIssue,
  sourceTitle,
  summarizeJobs,
  validationLabel,
  viewModeLabel,
} from "./lib/format";
import {
  Card,
  Field,
  ControlGroup,
  Input,
  Button,
  ActionLink,
  Toggle,
  HelpCard,
  Badge,
  JobStatusBadge,
  Empty,
  StatusLine,
  MetaLine,
} from "./components/primitives";
import { StepSlider } from "./components/StepSlider";
import { Dropdown, SegmentedControl, Pager } from "./components/controls";
import { NotificationStack, FilePreviewModal, ConfirmDialog } from "./components/overlays";
import { CandidateRow } from "./components/CandidateRow";
import { Player } from "./components/Player";

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
  const jobLoadRequestRef = useRef(0);
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
  const normalizedSourceUrl = useMemo(() => normalizeSourceInput(form.url), [form.url]);
  const sourceUrlIssue = useMemo(
    () => sourceInputIssue(normalizedSourceUrl, health?.public_base_url),
    [normalizedSourceUrl, health?.public_base_url],
  );

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
    const requestId = ++jobLoadRequestRef.current;
    try {
      const job = await request<JobView>(`/api/jobs/${id}`);
      if (requestId !== jobLoadRequestRef.current) return;
      setSelectedJob(job);
      const [candidateData, artifactData] = await Promise.all([
        request<Candidate[]>(`/api/jobs/${id}/candidates`).catch(() => []),
        request<Artifact[]>(`/api/jobs/${id}/artifacts`).catch(() => []),
      ]);
      if (requestId !== jobLoadRequestRef.current) return;
      setCandidates(candidateData);
      setArtifacts(artifactData);
      if (job.outputs.includes("page_html")) {
        const tree = await request<ArchiveTreeView>(`/api/jobs/${id}/archive/tree`).catch(() => null);
        if (requestId !== jobLoadRequestRef.current) return;
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
        url: normalizedSourceUrl,
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

  function handleSourceUrlChange(value: string) {
    const platformHint = platformHintForSourceUrl(value);
    if (platformHint && WEB_ARCHIVE_PLATFORM_HINTS.has(platformHint)) {
      setOutputMode("page_html");
      setForm({
        ...form,
        url: value,
        platform_hint: platformHint,
        discovery: "browser",
        outputs: ["page_html"],
        auth_mode: "none",
      });
      return;
    }
    setForm({ ...form, url: value });
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
    jobLoadRequestRef.current += 1;
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
                type="text"
                inputMode="url"
                placeholder="youtube.com/watch?v=..."
                value={form.url}
                className={sourceUrlIssue ? "input-warning" : ""}
                onChange={(event) => handleSourceUrlChange(event.target.value)}
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
            <div className="control-group">
              <StepSlider
                label="清晰度"
                options={["360p", "480p", "720p", "1080p", "1440p", "2160p", "auto"]}
                value={form.bitrate}
                labelFor={bitrateLabel}
                showTicks={false}
                onChange={(value) => setForm({ ...form, bitrate: value })}
              />
            </div>
            <ControlGroup label="站点">
              <Dropdown
                value={form.platform_hint}
                options={capabilities?.supported_platform_hints?.length
                  ? capabilities.supported_platform_hints
                  : FALLBACK_PLATFORM_HINTS}
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
                {selectedJob.original_source_url && selectedJob.original_source_url !== selectedJob.source_url && (
                  <MetaLine label="原始输入" value={selectedJob.original_source_url} copyable />
                )}
                <MetaLine label="来源 URL" value={selectedJob.source_url} copyable href={selectedJob.source_url} />
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

createRoot(document.getElementById("root")!).render(<App />);
