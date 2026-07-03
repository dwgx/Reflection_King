import React, { useId } from "react";

export interface RangeSliderProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number; // default 1
  valueText?: string; // aria-valuetext for SR
  display?: React.ReactNode; // what to show in the value badge; default = value
  disabled?: boolean;
  onChange: (value: number) => void;
}

/**
 * Controlled, accessible continuous slider built on the native
 * `<input type="range">`. The filled portion of the track is driven by the
 * `--fill` CSS custom property (see slider.css for the gradient technique).
 */
export function RangeSlider({
  label,
  value,
  min,
  max,
  step = 1,
  valueText,
  display,
  disabled,
  onChange,
}: RangeSliderProps) {
  const id = useId();
  const span = max - min;
  const pct = span > 0 ? ((value - min) / span) * 100 : 0;

  return (
    <div className="slider-field">
      <div className="slider-field__head">
        <label className="slider-field__label" htmlFor={id}>
          {label}
        </label>
        <output className="slider-field__value" htmlFor={id} aria-hidden="true">
          {display ?? value}
        </output>
      </div>
      <input
        id={id}
        type="range"
        className="slider"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        aria-valuetext={valueText}
        style={{ ["--fill" as string]: `${pct}%` }}
        onChange={(e) => onChange(e.target.valueAsNumber)}
      />
    </div>
  );
}

export default RangeSlider;
