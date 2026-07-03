import { useEffect, useRef, useState } from "react";
import { ChevronDown } from "lucide-react";
import { PAGE_SIZE_OPTIONS } from "../constants";
import { Button } from "./primitives";

export function Dropdown(props: {
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

export function SegmentedControl(props: {
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

export function Pager(props: {
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
