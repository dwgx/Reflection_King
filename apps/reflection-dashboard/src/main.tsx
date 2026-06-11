import React, { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  Activity,
  AlertTriangle,
  ChevronDown,
  ChevronUp,
  CheckCircle2,
  Clipboard,
  ClipboardPaste,
  Eye,
  ExternalLink,
  FileAudio,
  ListRestart,
  Loader2,
  Play,
  RefreshCw,
  Search,
  Server,
  Settings,
  X,
} from "lucide-react";
import "./styles.css";

type DiscoveryMode = "direct" | "external" | "browser" | "auto";
type PlatformHint = "auto" | "bilibili" | "youtube" | "soundcloud";
type OutputKind = "audio" | "video" | "image" | "page_html";
type OutputMode = "auto" | "video" | "audio" | "image" | "page_html";
type ViewMode = "console" | "admin" | "help";

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
  ffmpeg_path: string;
  yt_dlp_path: string | null;
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
  metadata_json?: unknown;
  selected?: boolean;
  selection_reason?: string | null;
  validation_status?: string | null;
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
  created_at: string;
  revoked_at: string | null;
}

interface CreatedUserKeyResponse {
  key: string;
  record: UserKeyView;
}

const OUTPUTS: OutputKind[] = ["audio", "video", "image", "page_html"];
const TERMINAL = new Set(["ready", "error", "candidates_ready"]);
const PAGE_SIZE_OPTIONS = [3, 5, 10, 20, 50];
const HIDDEN_JOBS_KEY = "reflection_hidden_job_ids";

function App() {
  const [apiKey, setApiKey] = useState(() => localStorage.getItem("reflection_api_key") ?? "");
  const [health, setHealth] = useState<Health | null>(null);
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null);
  const [jobs, setJobs] = useState<JobView[]>([]);
  const [hiddenJobIds, setHiddenJobIds] = useState<Set<string>>(() => loadHiddenJobIds());
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
  const [pasteOpen, setPasteOpen] = useState(false);
  const [pasteText, setPasteText] = useState("");
  const [waitingForPaste, setWaitingForPaste] = useState(false);
  const pasteInputRef = useRef<HTMLInputElement | null>(null);
  const sourceInputRef = useRef<HTMLInputElement | null>(null);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [userKeys, setUserKeys] = useState<UserKeyView[]>([]);
  const [newUserKey, setNewUserKey] = useState("");
  const [keyForm, setKeyForm] = useState({
    label: "普通用户",
    allow_browser_probe: true,
    allow_ytdlp: true,
  });
  const [profileId, setProfileId] = useState("admin_default");
  const [cookieJson, setCookieJson] = useState("");
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

  useEffect(() => {
    localStorage.setItem("reflection_api_key", apiKey);
  }, [apiKey]);

  useEffect(() => {
    localStorage.setItem(HIDDEN_JOBS_KEY, JSON.stringify(Array.from(hiddenJobIds)));
  }, [hiddenJobIds]);

  useEffect(() => {
    void refreshSystem();
    if (apiKey) {
      void refreshJobs();
    }
  }, [headers]);

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
    if (!pasteOpen) return;
    window.setTimeout(() => pasteInputRef.current?.focus(), 0);
  }, [pasteOpen]);

  useEffect(() => {
    if (!waitingForPaste) return;
    const onPaste = (event: ClipboardEvent) => {
      const text = event.clipboardData?.getData("text/plain") ?? "";
      if (!text.trim()) return;
      event.preventDefault();
      applyPastedUrl(text);
      setWaitingForPaste(false);
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [waitingForPaste, form]);

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

  async function refreshSystem() {
    try {
      const healthData = await requestWithoutAuth<Health>("/api/health");
      setHealth(healthData);
    } catch (error) {
      setMessage(errorMessage(error));
    }

    try {
      const capabilityData = await request<Capabilities>("/api/capabilities");
      setCapabilities(capabilityData);
    } catch (error) {
      setCapabilities(null);
      if (apiKey) {
        setMessage(errorMessage(error));
      } else {
        setMessage("系统状态已加载；填写管理密钥后可查看解析能力和任务。");
      }
    }
  }

  async function refreshJobs(hiddenOverride = hiddenJobIds) {
    try {
      const data = await request<JobView[]>("/api/jobs?limit=100");
      const visible = data.filter((job) => !hiddenOverride.has(job.id));
      setJobs(visible);
      setJobPage((page) => clampPage(page, visible.length, jobPageSize));
      if (selectedJobId && hiddenJobIds.has(selectedJobId)) {
        clearSelection();
      } else if (!selectedJobId && visible[0]) {
        setSelectedJobId(visible[0].id);
      }
    } catch (error) {
      setMessage(errorMessage(error));
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
      if (!quiet) setMessage(errorMessage(error));
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
      setMessage(`已创建任务 ${job.id}`);
      await refreshJobs();
      await loadJob(job.id);
    } catch (error) {
      setMessage(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function pasteFromClipboard() {
    try {
      if (!navigator.clipboard?.readText) {
        focusForManualPaste("浏览器不允许直接读取剪贴板，已聚焦来源 URL；按 Ctrl+V 会自动填入。");
        return;
      }
      const text = await navigator.clipboard.readText();
      if (!text.trim()) {
        setMessage("剪贴板为空");
        focusForManualPaste("剪贴板为空，已聚焦来源 URL；按 Ctrl+V 会自动填入。");
        return;
      }
      applyPastedUrl(text);
    } catch {
      focusForManualPaste("浏览器拦截剪贴板读取，已聚焦来源 URL；按 Ctrl+V 会自动填入。");
    }
  }

  function openPasteBox(reason: string) {
    setPasteText("");
    setPasteOpen(true);
    setMessage(reason);
  }

  function focusForManualPaste(reason: string) {
    setWaitingForPaste(true);
    setPasteOpen(true);
    setPasteText("");
    setMessage(reason);
    window.setTimeout(() => sourceInputRef.current?.focus(), 0);
  }

  function applyPastedUrl(text = pasteText) {
    const value = text.trim();
    if (!value) {
      setMessage("未粘贴内容");
      return;
    }
    setForm({ ...form, url: value });
    setPasteText("");
    setPasteOpen(false);
    setWaitingForPaste(false);
    setMessage("已填入粘贴内容");
  }

  function clearForm() {
    setForm({ ...form, url: "" });
    setMessage("已清空来源 URL");
  }

  function clearVisibleJobs() {
    if (!jobs.length) {
      setMessage("任务列表已经为空");
      return;
    }
    const next = new Set(hiddenJobIds);
    for (const job of jobs) {
      next.add(job.id);
    }
    setHiddenJobIds(next);
    setJobs([]);
    clearSelection();
    setJobPage(1);
    setMessage("已清空当前任务列表；数据库历史未删除");
  }

  async function restoreHiddenJobs() {
    const empty = new Set<string>();
    setHiddenJobIds(empty);
    setMessage("已恢复本机隐藏的历史任务");
    await refreshJobs(empty);
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
      setMessage("已提交转码任务");
    } catch (error) {
      setMessage(errorMessage(error));
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
      setMessage(errorMessage(error));
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
      setMessage("已创建用户密钥，明文只显示这一次。");
    } catch (error) {
      setMessage(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function revokeUserKey(id: string) {
    setBusy(true);
    try {
      await request<void>(`/api/admin/user-keys/${id}/revoke`, { method: "POST" });
      await refreshUserKeys();
      setMessage("已撤销用户密钥");
    } catch (error) {
      setMessage(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function startProfileLogin(platform: string) {
    setBusy(true);
    try {
      const response = await request<{ message?: string; mode?: string }>(
        `/api/admin/browser-profiles/${encodeURIComponent(profileId)}/login-session`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ headed: true, platform }),
        },
      );
      setMessage(response.message ?? "已准备浏览器登录会话");
    } catch (error) {
      setMessage(errorMessage(error));
    } finally {
      setBusy(false);
    }
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
      setMessage("已导入浏览器 Profile Cookie");
    } catch (error) {
      setMessage(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  const consoleView = (
    <>
      {!apiKey && (
        <div className="rounded-md border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-100">
          填写管理密钥或用户密钥后可查看任务、创建解析和选择资源。管理功能只对管理密钥开放。
        </div>
      )}

      <section className="mx-auto w-full max-w-5xl">
        <Card title="创建任务" icon={<Play size={16} />}>
          <form className="space-y-4" onSubmit={createJob}>
            <div className="grid gap-3 lg:grid-cols-[1fr_auto_auto]">
              <Field label="来源 URL">
                <Input
                  ref={sourceInputRef}
                  required
                  type="url"
                  placeholder="https://example.com/watch/123"
                  value={form.url}
                  onChange={(event) => setForm({ ...form, url: event.target.value })}
                />
              </Field>
              <div className="flex items-end gap-2">
                <Button type="button" variant="secondary" onClick={pasteFromClipboard} disabled={busy}>
                  <ClipboardPaste size={16} /> 粘贴剪贴板
                </Button>
                <Button type="button" variant="secondary" onClick={clearForm} disabled={busy}>
                  <X size={16} /> 清空
                </Button>
              </div>
              <div className="flex items-end">
                <Button className="w-full min-w-40" type="submit" disabled={busy}>
                  {busy ? <Loader2 className="animate-spin" size={16} /> : <Search size={16} />}
                  创建
                </Button>
              </div>
            </div>

            {pasteOpen && (
              <div className="rounded-md border border-cyan-500/40 bg-cyan-500/10 p-3">
                <div className="flex items-center justify-between gap-2">
                  <span className="text-sm font-medium text-cyan-100">
                    {waitingForPaste ? "等待粘贴" : "粘贴 URL"}
                  </span>
                  <button
                    className="rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100"
                    type="button"
                    onClick={() => {
                      setPasteOpen(false);
                      setWaitingForPaste(false);
                    }}
                  >
                    <X size={16} />
                  </button>
                </div>
                <div className="mt-3 grid gap-2 sm:grid-cols-[1fr_auto]">
                  <Input
                    ref={pasteInputRef}
                    value={pasteText}
                    placeholder={waitingForPaste ? "已聚焦来源 URL，也可在这里 Ctrl+V" : "在这里按 Ctrl+V"}
                    onPaste={(event) => {
                      const text = event.clipboardData.getData("text/plain");
                      if (text.trim()) {
                        event.preventDefault();
                        applyPastedUrl(text);
                      }
                    }}
                    onChange={(event) => setPasteText(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.preventDefault();
                        applyPastedUrl();
                      }
                    }}
                  />
                  <Button type="button" onClick={() => applyPastedUrl()}>
                    填入
                  </Button>
                </div>
              </div>
            )}

            <div className="grid gap-4">
              <ControlGroup label="解析方式">
                <SegmentedControl
                  value={form.discovery}
                  options={["auto", "browser", "external", "direct"]}
                  labelFor={discoveryLabel}
                  onChange={(value) => setForm({ ...form, discovery: value as DiscoveryMode })}
                />
              </ControlGroup>
              <ControlGroup label="码率">
                <SegmentedControl
                  value={form.bitrate}
                  options={["auto", "2160p", "1440p", "1080p", "720p", "480p", "360p"]}
                  labelFor={bitrateLabel}
                  onChange={(value) => setForm({ ...form, bitrate: value })}
                />
              </ControlGroup>
              <div className="grid gap-4 lg:grid-cols-2">
                <ControlGroup label="平台">
                  <SegmentedControl
                    value={form.platform_hint}
                    options={["auto", "bilibili", "youtube", "soundcloud"]}
                    labelFor={platformLabel}
                    onChange={(value) => setForm({ ...form, platform_hint: value as PlatformHint })}
                  />
                </ControlGroup>
                <ControlGroup label="输出类型">
                  <SegmentedControl
                    value={outputMode}
                    options={["auto", "video", "audio", "image", "page_html"]}
                    labelFor={outputModeLabel}
                    onChange={(value) => setOutputModeAndPayload(value as OutputMode)}
                  />
                </ControlGroup>
              </div>
            </div>

            <div className="rounded-md border border-zinc-800 bg-zinc-950/70">
              <button
                className="flex w-full items-center justify-between px-3 py-2 text-left text-sm text-zinc-300"
                type="button"
                onClick={() => setAdvancedOpen(!advancedOpen)}
              >
                高级设置
                {advancedOpen ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
              </button>
              {advancedOpen && (
                <div className="grid gap-3 border-t border-zinc-800 p-3 lg:grid-cols-2">
                  <Field label="授权模式">
                    <Select
                      value={form.auth_mode}
                      onChange={(event) => setForm({ ...form, auth_mode: event.target.value as CreateJobPayload["auth_mode"] })}
                      options={["auto", "none", "profile", "cookies"]}
                      labelFor={authModeLabel}
                    />
                  </Field>
                  <Info label="浏览器 Profile" value={form.profile_id} />
                </div>
              )}
            </div>
          </form>
        </Card>
      </section>

      <section className="grid items-stretch gap-4 xl:grid-cols-2">
        <Card
          title="任务列表"
          icon={<Activity size={16} />}
          action={
            <div className="flex flex-wrap gap-2">
              <Button variant="secondary" onClick={restoreHiddenJobs} disabled={!hiddenJobIds.size}>
                <Eye size={16} /> 恢复
              </Button>
              <Button variant="secondary" onClick={clearVisibleJobs} disabled={!jobs.length}>
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
          <div className="flex h-full min-h-0 flex-col gap-3">
            <div className="grid min-h-0 flex-1 auto-rows-max gap-2 overflow-y-auto pr-1">
              {pagedJobs.items.map((job) => (
                <button
                  key={job.id}
                  className={`grid w-full gap-2 rounded-md border p-3 text-left transition-colors ${
                    job.id === selectedJobId
                      ? "border-cyan-500/50 bg-cyan-500/10"
                      : "border-zinc-800 bg-zinc-950 hover:border-zinc-700"
                  }`}
                  type="button"
                  onClick={() => setSelectedJobId(job.id)}
                >
                  <div className="flex items-center justify-between gap-3">
                    <Badge status={job.status} />
                    <span className="text-xs text-zinc-500">{formatShortDate(job.updated_at)}</span>
                  </div>
                  <div className="min-w-0">
                    <div className="truncate text-sm font-medium text-zinc-100">{sourceTitle(job.source_url)}</div>
                    <div className="mt-1 line-clamp-2 break-all text-xs text-zinc-500">
                      {job.error ? friendlyError(job.error) : job.source_url}
                    </div>
                  </div>
                  <div className="flex flex-wrap items-center gap-2 text-xs text-zinc-400">
                    <span className="rounded bg-zinc-800 px-1.5 py-0.5">{discoveryLabel(job.discovery)}</span>
                    <span className="rounded bg-zinc-800 px-1.5 py-0.5">{platformLabel(job.platform_hint)}</span>
                    <span className="rounded bg-zinc-800 px-1.5 py-0.5">{outputsLabel(job.outputs)}</span>
                  </div>
                </button>
              ))}
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

        <Card title="任务详情" icon={<Settings size={16} />} className="dashboard-panel" bodyClassName="dashboard-panel-body overflow-auto">
          {selectedJob ? (
            <div className="grid gap-3 text-sm">
              <Info label="ID" value={selectedJob.id} />
              <Info label="状态" value={statusLabel(selectedJob.status)} tone={selectedJob.status === "error" ? "danger" : "normal"} />
              <Info label="输出类型" value={outputsLabel(selectedJob.outputs)} />
              <Info label="解析方式" value={`${discoveryLabel(selectedJob.discovery)} / ${platformLabel(selectedJob.platform_hint)}`} />
              {selectedJob.error && <Info label="错误摘要" value={friendlyError(selectedJob.error)} tone="danger" />}
              <Info label="播放地址" value={selectedJob.media_url ?? "-"} copyable />
              {selectedJob.media_url && <Player url={selectedJob.media_url} />}
            </div>
          ) : (
            <Empty label="请选择一个任务" />
          )}
        </Card>
      </section>

      <section className="grid gap-4 xl:grid-cols-2">
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
                ? "转换选中资源"
                : defaultCandidateIds.length > 1
                  ? `转换推荐资源 (${defaultCandidateIds.length})`
                  : "转换推荐资源"}
            </Button>
          }
        >
          {candidates.length ? (
            <div className="flex h-full min-h-0 flex-col gap-3">
              <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-zinc-500">
                <span>
                  找到 {candidates.length} 个资源，当前显示 {pagedCandidates.items.length} 个。
                  {selectedJob && ` ${qualityAvailabilityLabel(candidates, selectedJob.bitrate)}`}
                </span>
                <Button type="button" variant="secondary" className="h-8" onClick={() => setShowAllCandidates(!showAllCandidates)}>
                  {showAllCandidates ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                  {showAllCandidates ? "只看推荐" : "显示全部"}
                </Button>
              </div>
              <div className="grid min-h-0 flex-1 auto-rows-max gap-2 overflow-y-auto pr-1">
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

        <Card title="产物" icon={<ExternalLink size={16} />} className="resource-panel" bodyClassName="resource-panel-body overflow-auto">
          {artifacts.length ? (
            <div className="grid gap-3">
              {artifacts.map((artifact) => (
                <div key={artifact.id} className="rounded-md border border-zinc-800 bg-zinc-950 p-3">
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div>
                      <div className="font-medium">{outputLabel(artifact.kind)}</div>
                      <div className="text-xs text-zinc-500">{artifact.content_type} / {formatBytes(artifact.bytes)}</div>
                    </div>
                    <div className="flex gap-2">
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

      <section className="grid gap-4 xl:grid-cols-[0.8fr_1.2fr]">
        <Card title="系统状态" icon={<Server size={16} />}>
          <div className="grid gap-3 text-sm">
            <StatusLine label="API" ok={health?.ok} value={health?.service ? "正常" : "连接中"} />
            <StatusLine
              label="浏览器探测"
              ok={capabilities?.browser_probe_configured}
              value={capabilityStatus(capabilities?.browser_probe_configured, apiKey)}
            />
            <StatusLine
              label="yt-dlp"
              ok={capabilities?.yt_dlp_configured}
              value={capabilityStatus(capabilities?.yt_dlp_configured, apiKey)}
            />
            <Info label="当前密钥" value={capabilities?.auth ? `${roleLabel(capabilities.auth.role)} / ${capabilities.auth.label}` : apiKey ? "读取中" : "未填写"} />
            <Info label="公网基址" value={health?.public_base_url ?? "-"} />
            <Info label="下载上限" value={formatBytes(health?.max_download_bytes)} />
          </div>
        </Card>

        <div className="rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2 text-sm text-zinc-400">
          {message}
        </div>
      </section>
    </>
  );

  const adminView = (
    <section className="grid gap-4 xl:grid-cols-2">
      <Card
        title="用户密钥"
        icon={<Settings size={16} />}
        action={<Button variant="secondary" onClick={refreshUserKeys}><RefreshCw size={16} /> 刷新</Button>}
      >
        <form className="grid gap-3" onSubmit={createUserKey}>
          <Field label="名称">
            <Input value={keyForm.label} onChange={(event) => setKeyForm({ ...keyForm, label: event.target.value })} />
          </Field>
          <div className="grid gap-2 sm:grid-cols-2">
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
          </div>
          <Button type="submit" disabled={busy}>创建用户密钥</Button>
        </form>
        {newUserKey && (
          <div className="mt-3 rounded-md border border-emerald-500/30 bg-emerald-500/10 p-3 text-sm">
            <div className="text-emerald-200">明文密钥只显示一次</div>
            <div className="mt-2 break-all font-mono text-xs text-zinc-100">{newUserKey}</div>
            <Button className="mt-3 h-8" variant="secondary" onClick={() => copy(newUserKey)}>
              <Clipboard size={14} /> 复制
            </Button>
          </div>
        )}
        <div className="mt-4 grid gap-2">
          {userKeys.map((key) => (
            <div key={key.id} className="rounded-md border border-zinc-800 bg-zinc-950 p-3 text-sm">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <div className="font-medium text-zinc-100">{key.label}</div>
                  <div className="text-xs text-zinc-500">{key.key_prefix}... / {roleLabel(key.role)}</div>
                </div>
                <Button className="h-8" variant="secondary" disabled={Boolean(key.revoked_at) || busy} onClick={() => revokeUserKey(key.id)}>
                  撤销
                </Button>
              </div>
              <div className="mt-2 flex flex-wrap gap-2 text-xs text-zinc-400">
                <span className="rounded bg-zinc-800 px-1.5 py-0.5">浏览器：{key.allow_browser_probe ? "允许" : "禁止"}</span>
                <span className="rounded bg-zinc-800 px-1.5 py-0.5">yt-dlp：{key.allow_ytdlp ? "允许" : "禁止"}</span>
                {key.revoked_at && <span className="rounded bg-red-500/15 px-1.5 py-0.5 text-red-200">已撤销</span>}
              </div>
            </div>
          ))}
          {!userKeys.length && <Empty label="暂无用户密钥，点击刷新或创建一个" />}
        </div>
      </Card>

      <Card title="浏览器账号配置" icon={<Server size={16} />}>
        <div className="grid gap-3">
          <Field label="Profile ID">
            <Input value={profileId} onChange={(event) => setProfileId(event.target.value)} />
          </Field>
          <div className="grid gap-2 sm:grid-cols-2">
            {["哔哩哔哩", "YouTube", "抖音", "快手"].map((platform) => (
              <Button key={platform} type="button" variant="secondary" onClick={() => startProfileLogin(platform)} disabled={busy}>
                <Play size={16} /> 准备登录 {platform}
              </Button>
            ))}
          </div>
          <form className="grid gap-2" onSubmit={importProfileCookies}>
            <label className="grid gap-1.5 text-sm text-zinc-400">
              <span>Cookie JSON</span>
              <textarea
                className="input min-h-32 py-2 font-mono text-xs"
                placeholder='[{"name":"SESSDATA","value":"...","domain":".bilibili.com","path":"/"}]'
                value={cookieJson}
                onChange={(event) => setCookieJson(event.target.value)}
              />
            </label>
            <Button type="submit" disabled={busy || !cookieJson.trim()}>导入 Cookie 到 Profile</Button>
          </form>
        </div>
      </Card>
    </section>
  );

  const helpView = (
    <section className="grid gap-4 lg:grid-cols-2">
      <HelpCard title="密钥逻辑" lines={[
        "管理密钥来自服务器 RK_API_KEY，可以创建和撤销用户密钥。",
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
  );

  return (
    <main className="min-h-screen bg-zinc-950 text-zinc-100">
      <header className="border-b border-zinc-800 bg-zinc-950/95 px-6 py-4">
        <div className="mx-auto grid max-w-7xl gap-4 lg:grid-cols-[1fr_auto_auto] lg:items-center">
          <div>
            <h1 className="text-xl font-semibold tracking-normal">Reflection King</h1>
            <p className="text-sm text-zinc-400">媒体抓取与转码控制台</p>
          </div>
          <nav className="segmented w-full lg:w-auto">
            {(["console", "admin", "help"] as ViewMode[]).map((mode) => (
              <button
                key={mode}
                className={viewMode === mode ? "active" : ""}
                type="button"
                onClick={() => {
                  setViewMode(mode);
                  if (mode === "admin" && apiKey) void refreshUserKeys();
                }}
              >
                {viewModeLabel(mode)}
              </button>
            ))}
          </nav>
          <div className="flex flex-wrap items-center gap-2">
            <Input
              className="w-64"
              type="password"
              placeholder="输入管理密钥或用户密钥"
              value={apiKey}
              onChange={(event) => setApiKey(event.target.value)}
            />
            <Button onClick={refreshSystem} variant="secondary">
              <RefreshCw size={16} /> 刷新
            </Button>
          </div>
        </div>
      </header>

      <div className="mx-auto grid max-w-7xl gap-4 p-4">
        {viewMode === "console" && consoleView}
        {viewMode === "admin" && adminView}
        {viewMode === "help" && helpView}
      </div>
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
    <section className={`rounded-lg border border-zinc-800 bg-zinc-900/80 ${props.className ?? ""}`}>
      <div className="flex items-center justify-between gap-3 border-b border-zinc-800 px-4 py-3">
        <h2 className="flex items-center gap-2 text-sm font-semibold text-zinc-100">{props.icon}{props.title}</h2>
        {props.action}
      </div>
      <div className={`p-4 ${props.bodyClassName ?? ""}`}>{props.children}</div>
    </section>
  );
}

function Field(props: { label: string; children: React.ReactNode }) {
  return (
    <label className="grid gap-1.5 text-sm text-zinc-400">
      <span>{props.label}</span>
      {props.children}
    </label>
  );
}

function ControlGroup(props: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid gap-2">
      <div className="text-sm text-zinc-400">{props.label}</div>
      {props.children}
    </div>
  );
}

const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(function Input(props, ref) {
  const { className = "", ...rest } = props;
  return <input ref={ref} className={`input ${className}`} {...rest} />;
});

function Select(props: {
  value: string;
  options: string[];
  onChange: React.ChangeEventHandler<HTMLSelectElement>;
  labelFor?: (value: string) => string;
}) {
  return (
    <select className="input" value={props.value} onChange={props.onChange}>
      {props.options.map((option) => <option key={option} value={option}>{props.labelFor?.(option) ?? option}</option>)}
    </select>
  );
}

function Button(props: React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "secondary" }) {
  const { className = "", variant = "primary", ...rest } = props;
  return <button className={`button ${variant} ${className}`} {...rest} />;
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
    <label className="flex cursor-pointer items-center justify-between gap-3 rounded-md border border-zinc-800 bg-zinc-950 px-3 py-2 text-sm text-zinc-200">
      <span>{props.label}</span>
      <input
        className="h-4 w-4"
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
      <div className="grid gap-3 text-sm text-zinc-300">
        {props.lines.map((line) => (
          <p key={line} className="leading-6">{line}</p>
        ))}
      </div>
    </Card>
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
  const validation = props.candidate.validation_status;
  const isBad = Boolean(validation?.startsWith("failed"));
  return (
    <label
      className={`grid cursor-pointer gap-3 rounded-md border p-3 text-sm transition-colors md:grid-cols-[24px_1fr] ${
        isBad
          ? "border-red-500/30 bg-red-500/5"
          : props.selected || props.recommended
          ? "border-cyan-500/50 bg-cyan-500/10"
          : "border-zinc-800 bg-zinc-950 hover:border-zinc-700"
      }`}
    >
      <input
        className="mt-1 h-4 w-4"
        type="checkbox"
        checked={props.selected}
        onChange={props.onToggle}
      />
      <div className="min-w-0 overflow-hidden">
        <div className="flex flex-wrap items-start justify-between gap-2">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-medium text-zinc-100">
              {props.recommended ? "推荐 " : ""}
              {candidateKindLabel(props.candidate.kind)}
            </span>
            <span className="rounded bg-zinc-800 px-1.5 py-0.5 text-xs text-zinc-300">{summary.quality}</span>
            <span className="rounded bg-zinc-800 px-1.5 py-0.5 text-xs text-zinc-300">{summary.source}</span>
          </div>
          <div className="text-xs text-zinc-500">
            {formatBytes(props.candidate.content_length)}
          </div>
        </div>
        <div className="mt-2 grid gap-1 text-xs text-zinc-400 sm:grid-cols-3">
          <span className="truncate">类型：{summary.kindDetail}</span>
          <span className="truncate">来源：{extractorLabel(props.candidate.extractor)}</span>
          <span className="truncate">评分：{props.candidate.score}</span>
        </div>
        <div className="mt-2 flex flex-wrap gap-1.5">
          {props.recommended && <span className="rounded bg-cyan-500/15 px-1.5 py-0.5 text-xs text-cyan-200">自动推荐</span>}
          {props.candidate.kind === "manifest" && <span className="rounded bg-blue-500/15 px-1.5 py-0.5 text-xs text-blue-200">清单流</span>}
          {props.candidate.requires_authorization && (
            <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-xs text-amber-200">需要页面授权</span>
          )}
          {summary.adRisk && <span className="rounded bg-red-500/15 px-1.5 py-0.5 text-xs text-red-200">广告/跟踪嫌疑</span>}
          {validation && (
            <span className={`rounded px-1.5 py-0.5 text-xs ${isBad ? "bg-red-500/15 text-red-200" : "bg-emerald-500/15 text-emerald-200"}`}>
              {validationLabel(validation)}
            </span>
          )}
        </div>
        <div className="mt-2 max-h-9 overflow-hidden break-all text-xs leading-4 text-zinc-500">
          第 {props.index + 1} 项 · {compactUrl(props.candidate.url)}
        </div>
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
    <div className="flex flex-wrap items-center justify-between gap-2 border-t border-zinc-900 pt-3 text-xs text-zinc-500">
      <span>第 {props.page} / {pageCount} 页，共 {props.total} 条</span>
      <div className="flex items-center gap-2">
        <select
          className="h-8 rounded-md border border-zinc-800 bg-zinc-950 px-2 text-xs text-zinc-200 outline-none"
          value={props.pageSize}
          onChange={(event) => props.onPageSizeChange(Number(event.target.value))}
        >
          {PAGE_SIZE_OPTIONS.map((value) => (
            <option key={value} value={value}>每页 {value}</option>
          ))}
        </select>
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

function Info(props: {
  label: string;
  value: string | number | null | undefined;
  copyable?: boolean;
  tone?: "normal" | "danger";
}) {
  const value = String(props.value ?? "-");
  const valueTone = props.tone === "danger" ? "text-red-200" : "text-zinc-200";
  return (
    <div className="grid gap-1 rounded-md border border-zinc-800 bg-zinc-950 px-3 py-2">
      <span className="text-xs uppercase text-zinc-500">{props.label}</span>
      <span className={`break-all ${valueTone}`}>
        {value}
        {props.copyable && value !== "-" && (
          <button className="ml-2 text-cyan-300" onClick={() => copy(value)} type="button">复制</button>
        )}
      </span>
    </div>
  );
}

function StatusLine(props: { label: string; ok?: boolean; value: string }) {
  return (
    <div className="flex items-center justify-between rounded-md border border-zinc-800 bg-zinc-950 px-3 py-2">
      <span className="text-zinc-400">{props.label}</span>
      <span className="flex items-center gap-2">
        {props.ok ? <CheckCircle2 className="text-emerald-400" size={16} /> : <AlertTriangle className="text-yellow-400" size={16} />}
        {props.value}
      </span>
    </div>
  );
}

function Badge(props: { status: string }) {
  const tone = props.status === "ready"
    ? "bg-emerald-500/15 text-emerald-300"
    : props.status === "error"
      ? "bg-red-500/15 text-red-300"
      : props.status === "candidates_ready"
        ? "bg-cyan-500/15 text-cyan-300"
        : "bg-zinc-700 text-zinc-200";
  return <span className={`inline-flex h-6 min-w-16 items-center justify-center rounded-full px-2 text-xs font-medium ${tone}`}>{statusLabel(props.status)}</span>;
}

function Empty(props: { label: string }) {
  return <div className="rounded-md border border-dashed border-zinc-800 p-6 text-center text-sm text-zinc-500">{props.label}</div>;
}

function Player(props: { url: string }) {
  const lower = props.url.toLowerCase();
  if (lower.endsWith(".mp4")) {
    return <video className="mt-3 w-full rounded-md border border-zinc-800" controls src={props.url} />;
  }
  return <audio className="mt-3 w-full" controls src={props.url} />;
}

function apiHeaders(apiKey: string): Record<string, string> {
  return apiKey ? { "x-api-key": apiKey } : {};
}

function loadHiddenJobIds(): Set<string> {
  try {
    const raw = localStorage.getItem(HIDDEN_JOBS_KEY);
    const parsed = raw ? JSON.parse(raw) : [];
    return new Set(Array.isArray(parsed) ? parsed.filter((value) => typeof value === "string") : []);
  } catch {
    return new Set();
  }
}

function errorMessage(error: unknown): string {
  return friendlyError(error instanceof Error ? error.message : String(error));
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
      .filter((candidate) => !isLikelyAdCandidate(candidate) && !candidate.validation_status?.startsWith("failed"));
    if (!mediaCandidates.length) {
      const imageCandidates = candidates.filter((candidate) => candidate.kind === "image");
      if (imageCandidates.length) return imageCandidates;
    }
    return mediaCandidates.slice(0, 1);
  }
  if (outputs.has("audio") && !outputs.has("video")) {
    const audioCandidate = candidateDisplayList(candidates, job, false)
      .find((candidate) => candidate.kind === "audio" && !candidate.validation_status?.startsWith("failed"));
    if (audioCandidate) {
      return [audioCandidate];
    }
  }
  return fallback ? [fallback] : [];
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
  if (candidate.validation_status?.startsWith("failed")) rank -= 4000;
  return rank;
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
  if (value === "ok") return "已验证可用";
  if (value.startsWith("failed:")) return `不可用：${friendlyError(value.slice("failed:".length).trim())}`;
  return value;
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

async function copy(value: string) {
  await navigator.clipboard.writeText(value);
}

createRoot(document.getElementById("root")!).render(<App />);
