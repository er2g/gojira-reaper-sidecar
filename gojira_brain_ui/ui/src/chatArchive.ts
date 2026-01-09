import type { SavedSnapshot, WorkspaceState } from "./workspace";

export type ChatSessionMeta = {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
};

export type ChatSessionDataV1 = ChatSessionMeta & {
  version: 1;
  history: Array<{
    ts: number;
    label: string;
    anchorMessageId?: string;
    state: WorkspaceState;
  }>;
  cursor: number;
  snapshots: SavedSnapshot[];
  composer: string;
};

export const CHAT_INDEX_KEY_V1 = "chat_index_v1";
export const CHAT_ACTIVE_KEY_V1 = "chat_active_id_v1";
export const CHAT_SESSION_KEY_PREFIX_V1 = "chat_session_v1:";

export const MAX_SESSIONS = 25;
export const MAX_HISTORY_ENTRIES = 200;
export const MAX_SNAPSHOTS = 100;

export function summarizeTitle(data: ChatSessionDataV1): string {
  const firstUser = data.history
    .flatMap((h) => h.state.chat)
    .find((m) => m.role === "user" && m.content.trim());
  const t = (firstUser?.content ?? "").trim();
  if (!t) return "Untitled chat";
  return t.length > 40 ? t.slice(0, 40).trim() + "…" : t;
}

export function clampSessionData(data: ChatSessionDataV1): ChatSessionDataV1 {
  const history = data.history.slice(-MAX_HISTORY_ENTRIES);
  const cursor = Math.min(Math.max(0, data.cursor), Math.max(0, history.length - 1));
  const snapshots = data.snapshots.slice(0, MAX_SNAPSHOTS);
  return { ...data, history, cursor, snapshots };
}
