import type { Candidate } from "../types";
import {
  candidateAvailability,
  candidateKindLabel,
  candidateSummary,
  compactUrl,
  extractorLabel,
  formatBytes,
  friendlyError,
  protectionLabel,
  routeLabel,
  validationLabel,
} from "../lib/format";

export function CandidateRow(props: {
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
