import { useId } from "react";

export interface StepSliderProps {
  label: string;
  options: string[]; // ordered stops
  value: string; // current option (must be one of options)
  labelFor?: (value: string) => string; // optional display label per stop
  onChange: (value: string) => void;
  disabled?: boolean;
  showTicks?: boolean; // default true: render tick labels under the track
}

/**
 * Discrete snap-to-stops slider for an ORDERED options array (quality stops,
 * page sizes, etc). The native range input is driven by the option INDEX; the
 * value/aria reflect the resolved option string. Filled track is driven by the
 * `--fill` CSS custom property (see slider.css).
 */
export function StepSlider({
  label,
  options,
  value,
  labelFor,
  onChange,
  disabled,
  showTicks = true,
}: StepSliderProps) {
  const id = useId();
  const index = Math.max(0, options.indexOf(value));
  const max = Math.max(0, options.length - 1);
  const pct = max > 0 ? (index / max) * 100 : 0;
  const display = labelFor?.(value) ?? value;

  return (
    <div className="slider-field">
      <div className="slider-field__head">
        <label className="slider-field__label" htmlFor={id}>
          {label}
        </label>
        <output className="slider-field__value" htmlFor={id} aria-hidden="true">
          {display}
        </output>
      </div>
      <input
        id={id}
        type="range"
        className="slider"
        min={0}
        max={max}
        step={1}
        value={index}
        disabled={disabled}
        aria-valuetext={display}
        style={{ ["--fill" as string]: `${pct}%` }}
        onChange={(e) => {
          const next = options[e.target.valueAsNumber];
          if (next !== undefined) onChange(next);
        }}
      />
      {showTicks ? (
        <div className="slider-ticks">
          {options.map((opt, i) => (
            <span
              key={opt}
              className={i === index ? "slider-tick active" : "slider-tick"}
            >
              {labelFor?.(opt) ?? opt}
            </span>
          ))}
        </div>
      ) : null}
    </div>
  );
}

export default StepSlider;
