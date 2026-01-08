export type StatusEvent = {
  status: "connecting" | "connected" | "disconnected";
  retry_in?: number | null;
};

export type Confidence = "high" | "low";

export type GojiraInstance = {
  track_guid: string;
  track_name: string;
  fx_guid: string;
  fx_name: string;
  last_known_fx_index: number;
  confidence: Confidence;
};

export type HandshakePayload = {
  session_token: string;
  instances: GojiraInstance[];
  validation_report: Record<string, string>;
  param_enums?: Record<string, Array<{ value: number; label: string }>>;
  param_formats?: Record<string, { min: string; mid: string; max: string }>;
  param_format_samples?: Record<string, Array<{ norm: number; formatted: string }>>;
};

export type ParamChange = {
  index: number;
  value: number;
};

export type DiffItem = {
  label: string;
  index: number;
  old_value: number | null;
  new_value: number | null;
};

export type PreviewResult = {
  reasoning: string;
  params: ParamChange[];
  diff: DiffItem[];
};
