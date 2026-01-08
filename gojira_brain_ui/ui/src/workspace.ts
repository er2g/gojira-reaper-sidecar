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

export function buildPromptFromChat(args: {
  messages: ChatMessage[];
  current: string;
  refine: boolean;
  baseParams: ParamChange[] | null;
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
