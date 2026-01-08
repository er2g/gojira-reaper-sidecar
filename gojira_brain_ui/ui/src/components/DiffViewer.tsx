import React from "react";
import type { DiffItem } from "../types";

type ParamFormats = Record<string, { min: string; mid: string; max: string }>;
type ParamFormatSamples = Record<
  string,
  Array<{ norm: number; formatted: string }>
>;

function fmtNorm(v: number | null) {
  if (v === null) return "n/a";
  return v.toFixed(3);
}

function parseNumUnit(
  s: string,
): { num: number; unit: string; raw: string } | null {
  const t = (s ?? "").trim();
  if (!t) return null;
  const m = t.match(/[+-]?\d+(?:[.,]\d+)?/);
  if (!m) return null;
  const num = Number(m[0].replace(",", "."));
  if (!Number.isFinite(num)) return null;
  const unit = t.slice((m.index ?? 0) + m[0].length).trim();
  return { num, unit, raw: t };
}

function formatNumUnit(num: number, unit: string) {
  const abs = Math.abs(num);
  const decimals = abs >= 1000 ? 0 : abs >= 100 ? 1 : 2;
  const s = num.toFixed(decimals);
  return unit ? `${s} ${unit}` : s;
}

function approxFromTriplet(
  fmt: { min: string; mid: string; max: string },
  norm: number,
) {
  const a = parseNumUnit(fmt.min);
  const b = parseNumUnit(fmt.mid);
  const c = parseNumUnit(fmt.max);
  if (!a || !b || !c) return null;
  if (a.unit !== b.unit || b.unit !== c.unit) return null;
  const x = Math.max(0, Math.min(1, norm));
  const num =
    x <= 0.5
      ? a.num + (b.num - a.num) * (x / 0.5)
      : b.num + (c.num - b.num) * ((x - 0.5) / 0.5);
  return formatNumUnit(num, a.unit);
}

function approxFromSamples(
  samples: Array<{ norm: number; formatted: string }>,
  norm: number,
) {
  const pts = samples
    .map((s) => ({ norm: s.norm, parsed: parseNumUnit(s.formatted) }))
    .filter(
      (
        p,
      ): p is { norm: number; parsed: { num: number; unit: string; raw: string } } =>
        !!p.parsed,
    )
    .sort((x, y) => x.norm - y.norm);

  if (!pts.length) return null;
  const unit = pts[0].parsed.unit;
  if (pts.some((p) => p.parsed.unit !== unit)) return null;

  const x = Math.max(0, Math.min(1, norm));
  if (x <= pts[0].norm) return pts[0].parsed.raw;
  if (x >= pts[pts.length - 1].norm) return pts[pts.length - 1].parsed.raw;

  for (let i = 0; i < pts.length - 1; i++) {
    const lo = pts[i];
    const hi = pts[i + 1];
    if (x >= lo.norm && x <= hi.norm) {
      const span = hi.norm - lo.norm;
      const t = span <= 0 ? 0 : (x - lo.norm) / span;
      const num = lo.parsed.num + (hi.parsed.num - lo.parsed.num) * t;
      return formatNumUnit(num, unit);
    }
  }

  return pts[0].parsed.raw;
}

function fmtHuman(
  index: number,
  v: number | null,
  formats?: ParamFormats,
  samples?: ParamFormatSamples,
) {
  if (v === null) return null;
  const key = String(index);
  const s = samples?.[key];
  if (s?.length) {
    const out = approxFromSamples(s, v);
    if (out) return out;
  }
  const f = formats?.[key];
  if (f) {
    const out = approxFromTriplet(f, v);
    if (out) return out;
  }
  return null;
}

export default function DiffViewer(props: {
  items: DiffItem[];
  formats?: ParamFormats;
  samples?: ParamFormatSamples;
}) {
  if (!props.items.length) {
    return <div className="muted">No changes.</div>;
  }

  return (
    <div className="diffList">
      {props.items.map((it) => {
        const isUp =
          it.old_value === null
            ? true
            : it.new_value !== null && it.new_value > it.old_value;
        const hOld = fmtHuman(it.index, it.old_value, props.formats, props.samples);
        const hNew = fmtHuman(it.index, it.new_value, props.formats, props.samples);
        return (
          <div key={`${it.index}`} className="diffRow">
            <div className="diffLabel">
              <span className="badge">#{it.index}</span> {it.label}
            </div>
            <div className={`diffValue ${isUp ? "up" : "down"}`}>
              {fmtNorm(it.old_value)} -&gt; {fmtNorm(it.new_value)}
              {hOld || hNew ? (
                <span className="muted" style={{ marginLeft: 8 }}>
                  (approx {hOld ?? "n/a"} -&gt; {hNew ?? "n/a"})
                </span>
              ) : null}
            </div>
          </div>
        );
      })}
    </div>
  );
}
