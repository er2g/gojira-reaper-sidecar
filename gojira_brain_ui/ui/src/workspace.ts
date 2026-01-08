import type { AckMessage, AppliedParam, ParamChange, PreviewResult } from "./types";

export type ChatRole = "user" | "assistant";

export type ChatMessage = {
  id: string;
  role: ChatRole;
  content: string;
  ts: number;
};

export type WorkspaceState = {
  chat: ChatMessage[];
  preview: PreviewResult | null;
  lastAck: AckMessage | null;
  workingParams: ParamChange[] | null;
  lastGenMode: "replace_active" | "merge";
};

export type HistoryEntry = {
  ts: number;
  label: string;
  anchorMessageId?: string;
  state: WorkspaceState;
};

export type SavedSnapshot = {
  id: string;
  ts: number;
  label: string;
  state: WorkspaceState;
};

export function nowId(prefix: string) {
  return `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function formatTime(ts: number) {
  try {
    return new Date(ts).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
}

export function mergeParamLists(base: ParamChange[], delta: ParamChange[]) {    
  const map = new Map<number, number>();
  for (const p of base) map.set(p.index, p.value);
  for (const p of delta) map.set(p.index, p.value);
  return Array.from(map.entries())
    .map(([index, value]) => ({ index, value }))
    .sort((a, b) => a.index - b.index);
}

type ParamFormats = Record<string, { min: string; mid: string; max: string }>;
type ParamFormatSamples = Record<
  string,
  Array<{ norm: number; formatted: string }>
>;

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

function closestSample(
  samples: Array<{ norm: number; formatted: string }>,
  norm: number,
) {
  let best: { norm: number; formatted: string } | null = null;
  let bestDist = Infinity;
  for (const s of samples) {
    const d = Math.abs(s.norm - norm);
    if (d < bestDist) {
      best = s;
      bestDist = d;
    }
  }
  return best?.formatted ?? null;
}

function humanizeParam(
  index: number,
  norm: number,
  formats?: ParamFormats,
  samples?: ParamFormatSamples,
) {
  const key = String(index);
  const s = samples?.[key];
  if (s?.length) {
    const out = closestSample(s, norm);
    if (out) return out;
  }
  const f = formats?.[key];
  if (f) {
    const out = approxFromTriplet(f, norm);
    if (out) return out;
  }
  return null;
}

export function buildPromptFromChat(args: {
  messages: ChatMessage[];
  current: string;
  refine: boolean;
  baseParams: ParamChange[] | null;
  formats?: ParamFormats;
  samples?: ParamFormatSamples;
}) {
  const recent = args.messages.filter((m) => m.content.trim()).slice(-8);
  const lines: string[] = [];

  lines.push("CONVERSATION CONTEXT (for tone continuity):");
  for (const m of recent) {
    const head = m.role === "user" ? "USER" : "ASSISTANT";
    lines.push(`${head}: ${m.content.trim()}`);
  }
  lines.push("");

  if (args.refine && args.baseParams?.length) {
    lines.push("CURRENT PRESET (baseline for iterative editing):");
    lines.push(
      "Below is the current preset as normalized 0..1 parameters. Treat it as the baseline.",
    );
    lines.push(
      "When possible, express values in human units (dB/ms/bpm) or knob scales (e.g. 6.5 out of 10); the backend will convert to normalized 0..1.",
    );
    if (args.formats || args.samples) {
      const human = args.baseParams
        .map((p) => ({
          index: p.index,
          value: humanizeParam(p.index, p.value, args.formats, args.samples),
        }))
        .filter((p): p is { index: number; value: string } => !!p.value);
      if (human.length) {
        lines.push(
          "Below is the same baseline rendered in human-friendly units (use this for reasoning and edits when possible):",
        );
        lines.push("BASE_PARAMS_HUMAN_JSON=" + JSON.stringify(human));
      }
    }
    lines.push(
      "Goal: refine this preset. Return ONLY a small list of parameter changes (deltas) relative to the baseline.",
    );
    lines.push("Keep the change list minimal and targeted (typically <= 12 params).");
    lines.push(
      "Avoid resetting unrelated modules. If you touch a module's parameters, include its Active toggle only when required.",
    );
    lines.push(
      "If you believe a full rebuild is necessary, briefly explain why and still return the best minimal deltas you can.",
    );
    lines.push("BASE_PARAMS_JSON=" + JSON.stringify(args.baseParams));
    lines.push("");
    lines.push("EDIT REQUEST:");
    lines.push(args.current.trim());
    lines.push("");
    lines.push("IMPORTANT: Return only the changed params (deltas), not a full preset.");
  } else {
    lines.push("CURRENT REQUEST:");
    lines.push(
      "Hint: prefer human units (dB/ms/bpm), option labels, or knob scales when you can; the backend will convert to normalized 0..1.",
    );
    lines.push(args.current.trim());
  }

  return lines.join("\n");
}

export function summarizeAppliedDelta(a: AppliedParam) {
  const d = a.applied - a.requested;
  return { delta: d, abs: Math.abs(d) };
}

export function initialWorkspace(): WorkspaceState {
  const seed: ChatMessage = {
    id: nowId("m"),
    role: "assistant",
    ts: Date.now(),
    content:
      "Describe a tone (band + era + tuning + pick attack). If you want to tweak an existing tone, say what to change and I’ll only adjust it instead of regenerating from scratch.",
  };
  return {
    chat: [seed],
    preview: null,
    lastAck: null,
    workingParams: null,
    lastGenMode: "replace_active",
  };
}
