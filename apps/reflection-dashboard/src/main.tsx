import React, { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  Activity,
  AlertTriangle,
  ChevronDown,
  ChevronUp,
  CheckCircle2,
  Clipboard,
  ClipboardPaste,
  ExternalLink,
  FileAudio,
  ListRestart,
  Loader2,
  Play,
  RefreshCw,
  Search,
  Server,
  Settings,
} from "lucide-react";
import "./styles.css";

type DiscoveryMode = "direct" | "external" | "browser" | "auto";
type PlatformHint = "auto" | "bilibili" | "youtube" | "soundcloud";
type OutputKind = "audio" | "video" | "image" | "page_html";

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
  auth_mode: "none" | "profile" | "cookies";
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
  const [form, setForm] = useState<CreateJobPayload>({
    url: "",
    bitrate: "auto",
    discovery: "auto",
    platform_hint: "auto",
    outputs: ["audio"],
    profile_id: "admin_default",
    auth_mode: "none",
  });

  const headers = useMemo(() => apiHeaders(apiKey), [apiKey]);

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

  async function refreshJobs() {
    try {
      const data = await request<JobView[]>("/api/jobs?limit=100");
      setJobs(data);
      setJobPage((page) => clampPage(page, data.length, jobPageSize));
      if (!selectedJobId && data[0]) {
        setSelectedJobId(data[0].id);
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
        await pasteWithPrompt();
        return;
      }
      const text = await navigator.clipboard.readText();
      if (!text.trim()) {
        setMessage("剪贴板为空");
        return;
      }
      setForm({ ...form, url: text.trim() });
      setMessage("已粘贴剪贴板内容");
    } catch {
      await pasteWithPrompt();
    }
  }

  async function pasteWithPrompt() {
    const text = window.prompt("浏览器禁止直接读取剪贴板，请在这里粘贴 URL");
    if (!text?.trim()) {
      setMessage("未粘贴内容");
      return;
    }
    setForm({ ...form, url: text.trim() });
    setMessage("已填入粘贴内容");
  }

  function clearForm() {
    setForm({ ...form, url: "" });
    setMessage("已清空来源 URL");
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

  function toggleOutput(output: OutputKind) {
    const next = new Set(form.outputs);
    if (next.has(output)) next.delete(output);
    else next.add(output);
    setForm({ ...form, outputs: next.size ? Array.from(next) : ["audio"] });
  }

  function toggleCandidate(id: string) {
    const next = new Set(selectedCandidates);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelectedCandidates(next);
  }

  return (
    <main className="min-h-screen bg-zinc-950 text-zinc-100">
      <header className="border-b border-zinc-800 bg-zinc-950/95 px-6 py-4">
        <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold tracking-normal">Reflection King</h1>
            <p className="text-sm text-zinc-400">媒体抓取与转码控制台</p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Input
              className="w-64"
              type="password"
              placeholder="输入管理密钥"
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
        {!apiKey && (
          <div className="rounded-md border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-100">
            填写管理密钥后可查看解析能力、任务列表、资源选择和产物。
          </div>
        )}

        <section className="mx-auto w-full max-w-5xl">
          <Card title="创建任务" icon={<Play size={16} />}>
            <form className="space-y-3" onSubmit={createJob}>
              <Field label="来源 URL">
                <Input
                  required
                  type="url"
                  placeholder="https://example.com/watch/123"
                  value={form.url}
                  onChange={(event) => setForm({ ...form, url: event.target.value })}
                />
              </Field>
              <div className="grid gap-3 md:grid-cols-4">
                <Field label="解析方式">
                  <Select
                    value={form.discovery}
                    onChange={(event) => setForm({ ...form, discovery: event.target.value as DiscoveryMode })}
                    options={["auto", "external", "browser", "direct"]}
                    labelFor={discoveryLabel}
                  />
                </Field>
                <Field label="平台">
                  <Select
                    value={form.platform_hint}
                    onChange={(event) => setForm({ ...form, platform_hint: event.target.value as PlatformHint })}
                    options={["auto", "bilibili", "youtube", "soundcloud"]}
                    labelFor={platformLabel}
                  />
                </Field>
                <Field label="码率">
                  <Select
                    value={form.bitrate}
                    onChange={(event) => setForm({ ...form, bitrate: event.target.value })}
                    options={["auto", "96k", "128k", "160k", "192k", "256k", "320k"]}
                    labelFor={bitrateLabel}
                  />
                </Field>
                <Field label="授权模式">
                  <Select
                    value={form.auth_mode}
                    onChange={(event) => setForm({ ...form, auth_mode: event.target.value as CreateJobPayload["auth_mode"] })}
                    options={["none", "profile", "cookies"]}
                    labelFor={authModeLabel}
                  />
                </Field>
              </div>
              <div className="grid gap-3 md:grid-cols-[1fr_1.4fr]">
                <Field label="浏览器配置 ID">
                  <Input
                    value={form.profile_id}
                    onChange={(event) => setForm({ ...form, profile_id: event.target.value })}
                  />
                </Field>
                <Field label="输出类型">
                  <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
                    {OUTPUTS.map((output) => (
                      <label key={output} className="flex items-center gap-2 rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2 text-sm">
                        <input
                          type="checkbox"
                          checked={form.outputs.includes(output)}
                          onChange={() => toggleOutput(output)}
                        />
                        {outputLabel(output)}
                      </label>
                    ))}
                  </div>
                </Field>
              </div>
              <div className="grid gap-2 sm:grid-cols-[1fr_1fr_1.3fr]">
                <Button type="button" variant="secondary" onClick={clearForm} disabled={busy}>
                  清空内容
                </Button>
                <Button type="button" variant="secondary" onClick={pasteFromClipboard} disabled={busy}>
                  <ClipboardPaste size={16} /> 粘贴剪贴板
                </Button>
                <Button type="submit" disabled={busy}>
                  {busy ? <Loader2 className="animate-spin" size={16} /> : <Search size={16} />}
                  创建
                </Button>
              </div>
            </form>
          </Card>
        </section>

        <section className="grid items-stretch gap-4 xl:grid-cols-[1.05fr_0.95fr]">
          <Card
            title="任务列表"
            icon={<Activity size={16} />}
            action={<Button variant="secondary" onClick={refreshJobs}><ListRestart size={16} /> 刷新</Button>}
            className="h-full"
          >
            <div className="grid h-full gap-3">
            <div className="overflow-x-auto">
              <table className="w-full min-w-[720px] table-fixed text-left text-sm">
                <thead className="text-xs uppercase text-zinc-500">
                  <tr>
                    <th className="w-24 px-2 py-2">状态</th>
                    <th className="px-2 py-2">来源</th>
                    <th className="w-36 px-2 py-2">解析方式</th>
                    <th className="w-36 px-2 py-2">更新时间</th>
                  </tr>
                </thead>
                <tbody>
                  {pagedJobs.items.map((job) => (
                    <tr
                      key={job.id}
                      className={`cursor-pointer border-t border-zinc-900 hover:bg-zinc-900/70 ${job.id === selectedJobId ? "bg-zinc-900 ring-1 ring-inset ring-cyan-500/25" : ""}`}
                      onClick={() => setSelectedJobId(job.id)}
                    >
                      <td className="px-2 py-3"><Badge status={job.status} /></td>
                      <td className="min-w-0 px-2 py-3">
                        <div className="truncate text-zinc-100">{sourceTitle(job.source_url)}</div>
                        <div className="truncate text-xs text-zinc-500">{job.error ? friendlyError(job.error) : job.source_url}</div>
                      </td>
                      <td className="px-2 py-3 text-zinc-400">{discoveryLabel(job.discovery)}/{platformLabel(job.platform_hint)}</td>
                      <td className="px-2 py-3 text-zinc-500">{formatShortDate(job.updated_at)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
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

          <div className="grid h-full gap-4">
            <Card title="任务详情" icon={<Settings size={16} />} className="min-h-[260px]">
              {selectedJob ? (
                <div className="grid gap-3 text-sm">
                  <Info label="ID" value={selectedJob.id} />
                  <Info label="状态" value={statusLabel(selectedJob.status)} tone={selectedJob.status === "error" ? "danger" : "normal"} />
                  <Info label="输出类型" value={selectedJob.outputs.map(outputLabel).join(", ")} />
                  {selectedJob.error && <Info label="错误摘要" value={friendlyError(selectedJob.error)} tone="danger" />}
                  <Info label="播放地址" value={selectedJob.media_url ?? "-"} copyable />
                  {selectedJob.media_url && <Player url={selectedJob.media_url} />}
                </div>
              ) : (
                <Empty label="请选择一个任务" />
              )}
            </Card>

            <Card
              title="资源选择"
              icon={<FileAudio size={16} />}
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
                <div className="grid gap-3">
                  <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-zinc-500">
                    <span>已找到 {candidates.length} 个资源，当前显示 {pagedCandidates.items.length} 个。</span>
                    <Button type="button" variant="secondary" className="h-8" onClick={() => setShowAllCandidates(!showAllCandidates)}>
                      {showAllCandidates ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                      {showAllCandidates ? "收起低价值资源" : "显示全部资源"}
                    </Button>
                  </div>
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
          </div>
        </section>

        <section className="grid gap-4 xl:grid-cols-2">
          <Card title="产物" icon={<ExternalLink size={16} />}>
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
              <Info label="公网基址" value={health?.public_base_url ?? "-"} />
              <Info label="ffmpeg" value={health?.ffmpeg_path ?? "-"} />
              <Info label="yt-dlp" value={capabilities?.yt_dlp_path ?? (apiKey ? "-" : "需要管理密钥")} />
              <Info label="下载上限" value={formatBytes(health?.max_download_bytes)} />
            </div>
          </Card>

          <div className="rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2 text-sm text-zinc-400">
            {message}
          </div>
        </section>
      </div>
    </main>
  );
}

function Card(props: { title: string; icon: React.ReactNode; action?: React.ReactNode; children: React.ReactNode; className?: string }) {
  return (
    <section className={`rounded-lg border border-zinc-800 bg-zinc-900/80 ${props.className ?? ""}`}>
      <div className="flex items-center justify-between gap-3 border-b border-zinc-800 px-4 py-3">
        <h2 className="flex items-center gap-2 text-sm font-semibold text-zinc-100">{props.icon}{props.title}</h2>
        {props.action}
      </div>
      <div className="p-4">{props.children}</div>
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

function Input(props: React.InputHTMLAttributes<HTMLInputElement>) {
  const { className = "", ...rest } = props;
  return <input className={`input ${className}`} {...rest} />;
}

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

function CandidateRow(props: {
  candidate: Candidate;
  index: number;
  selected: boolean;
  recommended: boolean;
  onToggle: () => void;
}) {
  const meta = candidateMeta(props.candidate);
  return (
    <label
      className={`grid cursor-pointer gap-3 rounded-md border p-3 text-sm transition-colors md:grid-cols-[24px_1fr_auto] ${
        props.selected || props.recommended
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
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-medium text-zinc-100">
            {props.recommended ? "推荐 " : ""}
            {candidateKindLabel(props.candidate.kind)}
          </span>
          <span className="rounded bg-zinc-800 px-1.5 py-0.5 text-xs text-zinc-300">{meta}</span>
          {props.candidate.requires_authorization && (
            <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-xs text-amber-200">需要页面授权</span>
          )}
        </div>
        <div className="mt-1 truncate text-xs text-zinc-500">{compactUrl(props.candidate.url)}</div>
      </div>
      <div className="text-left text-xs text-zinc-500 md:text-right">
        <div>{props.candidate.quality_label ?? `第 ${props.index + 1} 项`}</div>
        <div>{formatBytes(props.candidate.content_length)}</div>
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

function bitrateLabel(value: string): string {
  return value === "auto" ? "自动" : value;
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
    const mediaCandidates = candidates.filter((candidate) => candidate.kind === "video" || candidate.kind === "manifest");
    if (!mediaCandidates.length) {
      const imageCandidates = candidates.filter((candidate) => candidate.kind === "image");
      if (imageCandidates.length) return imageCandidates;
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
