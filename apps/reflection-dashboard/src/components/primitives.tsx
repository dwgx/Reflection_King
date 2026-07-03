import React from "react";
import { AlertTriangle, CheckCircle2, Settings } from "lucide-react";
import type { JobView } from "../types";
import { statusLabel, jobIssue, copy } from "../lib/format";

export function Card(props: { title: string; icon: React.ReactNode; action?: React.ReactNode; children: React.ReactNode; className?: string; bodyClassName?: string; }) {
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

export function Field(props: { label: string; children: React.ReactNode }) {
  return (<label className="field"><span>{props.label}</span>{props.children}</label>);
}

export function ControlGroup(props: { label: string; children: React.ReactNode }) {
  return (<div className="control-group"><div>{props.label}</div>{props.children}</div>);
}

export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(function Input(props, ref) {
  const { className = "", ...rest } = props;
  return <input ref={ref} className={`input ${className}`} {...rest} />;
});

export function Button(props: React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "secondary" }) {
  const { className = "", variant = "primary", type = "button", ...rest } = props;
  return <button type={type} className={`button ${variant} ${className}`} {...rest} />;
}

export function ActionLink(props: React.AnchorHTMLAttributes<HTMLAnchorElement> & { variant?: "primary" | "secondary" }) {
  const { className = "", variant = "primary", target = "_blank", rel = "noopener noreferrer", ...rest } = props;
  return <a target={target} rel={rel} className={`button ${variant} ${className}`} {...rest} />;
}

export function Toggle(props: { checked: boolean; label: string; onChange: (checked: boolean) => void }) {
  return (<label className="toggle"><span>{props.label}</span><input type="checkbox" checked={props.checked} onChange={(event) => props.onChange(event.target.checked)} /></label>);
}

export function HelpCard(props: { title: string; lines: string[] }) {
  return (<Card title={props.title} icon={<Settings size={16} />}><div className="help-copy">{props.lines.map((line) => (<p key={line}>{line}</p>))}</div></Card>);
}

export function Badge(props: { status: string }) {
  const tone = props.status === "ready" ? "ready" : props.status === "error" ? "error" : props.status === "candidates_ready" ? "candidates" : "progress";
  return <span className={`badge ${tone}`}>{statusLabel(props.status)}</span>;
}

export function JobStatusBadge(props: { job: JobView }) {
  const issue = jobIssue(props.job);
  if (!issue) return <Badge status={props.job.status} />;
  return <span className={`badge ${issue.tone}`}>{issue.label}</span>;
}

export function Empty(props: { label: string }) {
  return <div className="empty-state">{props.label}</div>;
}

export function StatusLine(props: { label: string; ok?: boolean; value: string }) {
  return (<div className="system-row"><span>{props.label}</span><strong>{props.ok ? <CheckCircle2 className="text-emerald-400" size={16} /> : <AlertTriangle className="text-yellow-400" size={16} />}{props.value}</strong></div>);
}

export function MetaLine(props: { label: string; value: string | number | null | undefined; copyable?: boolean; href?: string | null; }) {
  const value = String(props.value ?? "-");
  return (<div className="meta-line"><span>{props.label}</span><strong>{value}{props.copyable && value !== "-" && (<button onClick={() => copy(value)} type="button">复制</button>)}{props.href && value !== "-" && (<a href={props.href} target="_blank" rel="noopener noreferrer">打开</a>)}</strong></div>);
}
