import React from "react";
import type { ApiProviderOption, ProviderId } from "../apiProviders";
import type { GojiraInstance } from "../types";
import type { PickupPosition, SavedSnapshot } from "../workspace";
import { formatTime } from "../workspace";
import type { ChatSessionMeta } from "../chatArchive";

export default function SidebarPanel(props: {
  status: "connecting" | "connected" | "disconnected";
  instances: GojiraInstance[];
  selectedFxGuid: string;
  setSelectedFxGuid: (v: string) => void;
  selectedInstance: GojiraInstance | null;

  chats: ChatSessionMeta[];
  activeChatId: string;
  onNewChatSession: () => void;
  onOpenChatSession: (id: string) => void;
  onDeleteChatSession: (id: string) => void;

  cursor: number;
  historyLen: number;
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;

  previewOnly: boolean;
  setPreviewOnly: (v: boolean) => void;

  refineEnabled: boolean;
  setRefineEnabled: (v: boolean) => void;
  refineDisabled: boolean;

  providers: ApiProviderOption[];
  apiProvider: ProviderId;
  setApiProvider: (v: ProviderId) => void;
  apiModel: string;
  setApiModel: (v: string) => void;
  vaultPassphrase: string;
  setVaultPassphrase: (v: string) => void;
  apiKeyDrafts: Record<ProviderId, string>;
  setApiKeyDraft: (provider: ProviderId, value: string) => void;
  apiKeyPresence: Record<ProviderId, boolean>;
  onUnlockVault: () => void;
  onSaveKey: (provider: ProviderId) => void;
  onClearKey: (provider: ProviderId) => void;

  pickupNeck: string;
  setPickupNeck: (v: string) => void;
  pickupMiddle: string;
  setPickupMiddle: (v: string) => void;
  pickupBridge: string;
  setPickupBridge: (v: string) => void;
  pickupActive: PickupPosition | null;
  setPickupActive: (v: PickupPosition | null) => void;

  snapshots: SavedSnapshot[];
  onRestoreSnapshot: (s: SavedSnapshot) => void;
  onApplySnapshot: (s: SavedSnapshot) => void;
}) {
  return (
    <aside className="panel sidebar">
      <div className="panelHeader">
        <div className="panelTitle">
          <h2>Controls</h2>
          <div className="muted">
            {props.instances.length ? `${props.instances.length} plugin${props.instances.length > 1 ? 's' : ''} found` : "Waiting for REAPER..."}
          </div>
        </div>
        <span className="badge">{props.status}</span>
      </div>

      <div className="panelBody">
        <div className="row">
          <label>Target Plugin</label>
          <select
            value={props.selectedFxGuid}
            onChange={(e) => props.setSelectedFxGuid(e.target.value)}
            aria-label="Select target Gojira plugin"
          >
            {props.instances.map((i) => (
              <option key={i.fx_guid} value={i.fx_guid}>
                {(i.track_name || "Track") + " — " + (i.fx_name || "Archetype Gojira")}
              </option>
            ))}
          </select>
        </div>

        {props.selectedInstance ? (
          <div className="muted" style={{ marginTop: 6 }}>
            <div>📍 {props.selectedInstance.track_name || "Unnamed track"}</div>
            <div>🎸 {props.selectedInstance.fx_name || "Archetype Gojira"}</div>
          </div>
        ) : (
          <div className="muted" style={{ marginTop: 6, padding: 8, background: 'rgba(255, 200, 100, 0.08)', borderRadius: 8, border: '1px solid rgba(255, 200, 100, 0.15)' }}>
            ⚠️ Open a REAPER project with Archetype Gojira to get started
          </div>
        )}

        <div className="divider" />

        <h3 style={{ marginTop: 12, marginBottom: 10, fontSize: 13, fontWeight: 600 }}>History</h3>
        <div className="row" style={{ marginBottom: 8, justifyContent: "space-between" }}>
          <div style={{ display: "flex", gap: 8 }}>
            <button
              className="btn"
              disabled={!props.canUndo}
              type="button"
              onClick={props.onUndo}
              title="Undo last action"
              aria-label="Undo"
            >
              ← Undo
            </button>
            <button
              className="btn"
              disabled={!props.canRedo}
              type="button"
              onClick={props.onRedo}
              title="Redo last action"
              aria-label="Redo"
            >
              Redo →
            </button>
          </div>
          <button
            className="btn primary"
            type="button"
            onClick={props.onNewChatSession}
            aria-label="Start new chat session"
          >
            + New Chat
          </button>
        </div>

        <div className="muted" style={{ fontSize: 11, opacity: 0.7 }}>
          Step {props.cursor + 1} of {props.historyLen}
        </div>

        {props.chats.length ? (
          <>
            <div className="divider" />
            <h3 style={{ marginTop: 12, marginBottom: 10, fontSize: 13, fontWeight: 600 }}>
              Sessions ({props.chats.length})
            </h3>
            <div className="diffList" style={{ maxHeight: 200, overflow: "auto" }}>
              {props.chats.map((c) => (
                <div
                  key={c.id}
                  className="diffRow"
                  style={{
                    alignItems: "center",
                    opacity: c.id === props.activeChatId ? 1 : 0.85,
                    border: c.id === props.activeChatId ? "1px solid rgba(110, 168, 255, 0.3)" : undefined,
                    background: c.id === props.activeChatId ? "rgba(110, 168, 255, 0.05)" : undefined,
                  }}
                >
                  <div className="diffLabel" style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontWeight: c.id === props.activeChatId ? 600 : 400 }}>
                      {c.title || "Untitled"}
                    </div>
                    <div className="muted" style={{ marginTop: 4, fontSize: 11 }}>
                      {formatTime(c.updatedAt || c.createdAt)}
                    </div>
                  </div>
                  <div style={{ display: "flex", gap: 6 }}>
                    {c.id !== props.activeChatId && (
                      <button
                        className="btn btnSmall"
                        type="button"
                        onClick={() => props.onOpenChatSession(c.id)}
                        aria-label={`Open ${c.title || 'Untitled'} chat`}
                      >
                        Open
                      </button>
                    )}
                    <button
                      className="btn btnSmall danger"
                      type="button"
                      disabled={props.chats.length <= 1 && c.id === props.activeChatId}
                      title={props.chats.length <= 1 && c.id === props.activeChatId ? "Cannot delete the last session" : "Delete session"}
                      onClick={() => props.onDeleteChatSession(c.id)}
                      aria-label={`Delete ${c.title || 'Untitled'} chat`}
                    >
                      ✕
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </>
        ) : null}

        <div className="divider" />

        <h3 style={{ marginTop: 12, marginBottom: 10, fontSize: 13, fontWeight: 600 }}>Generation Mode</h3>

        <div className="row" style={{ marginBottom: 10 }}>
          <div className="segmented" style={{ width: "100%", justifyContent: "space-between" }}>
            <button
              className={`segBtn ${!props.refineEnabled || props.refineDisabled ? "segBtnActive" : ""}`}
              type="button"
              onClick={() => props.setRefineEnabled(false)}
              aria-label="New tone mode"
            >
              🎸 New Tone
            </button>
            <button
              className={`segBtn ${props.refineEnabled && !props.refineDisabled ? "segBtnActive" : ""}`}
              type="button"
              disabled={props.refineDisabled}
              onClick={() => props.setRefineEnabled(true)}
              title={props.refineDisabled ? "Generate at least one tone first" : "Tweak the current tone"}
              aria-label="Tweak mode"
            >
              ✨ Tweak
            </button>
          </div>
        </div>

        <div className="row" style={{ marginBottom: 0, alignItems: "flex-start" }}>
          <label className="checkbox">
            <input
              checked={props.previewOnly}
              onChange={(e) => props.setPreviewOnly(e.target.checked)}
              type="checkbox"
              aria-label="Preview only mode"
            />
            <span>Preview only (don't auto-apply)</span>
          </label>
        </div>

        <details style={{ marginTop: 12 }}>
          <summary className="muted" style={{ cursor: "pointer", fontWeight: 600 }}>
            ⚙️ Advanced: AI Settings
          </summary>
          <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 10 }}>
            <div className="row">
              <label>Vault passphrase</label>
              <input
                value={props.vaultPassphrase}
                onChange={(e) => props.setVaultPassphrase(e.target.value)}
                type="password"
                placeholder="Passphrase (never your API key)"
              />
              <button className="btn" onClick={props.onUnlockVault} type="button">
                Unlock
              </button>
            </div>

            <div className="row">
              <label>Active provider</label>
              <select value={props.apiProvider} onChange={(e) => props.setApiProvider(e.target.value)}>
                {props.providers.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
              </select>
            </div>

            <div className="row">
              <label>Model</label>
              <input
                value={props.apiModel}
                onChange={(e) => props.setApiModel(e.target.value)}
                placeholder="gemini-2.5-pro, gpt-4o, claude-3.5-sonnet, etc."
              />
            </div>

            <div className="muted">
              Keys live in the Stronghold vault. Gemini remains the active tone engine for now; other providers are staged here so you can save
              credentials and model names ahead of time.
            </div>

            <div className="divider" />
            <div className="muted">Provider credentials</div>
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              {props.providers.map((p) => {
                const draft = props.apiKeyDrafts[p.id] ?? "";
                const stored = props.apiKeyPresence[p.id];
                return (
                  <div key={p.id} className="row" style={{ alignItems: "center", gap: 8 }}>
                    <div style={{ minWidth: 170 }}>
                      <div>{p.label}</div>
                      {p.hint ? (
                        <div className="muted" style={{ fontSize: 12 }}>
                          {p.hint}
                        </div>
                      ) : null}
                      <div className="muted" style={{ fontSize: 12 }}>
                        Stored: {stored === undefined ? "unknown" : stored ? "yes" : "no"}
                      </div>
                    </div>
                    <input
                      value={draft}
                      onChange={(e) => props.setApiKeyDraft(p.id, e.target.value)}
                      type="password"
                      placeholder={p.placeholder || "API key / token"}
                    />
                    <button className="btn" onClick={() => props.onSaveKey(p.id)} type="button" disabled={!draft.trim()}>
                      Save
                    </button>
                    <button className="btn danger" onClick={() => props.onClearKey(p.id)} type="button">
                      Clear
                    </button>
                  </div>
                );
              })}
            </div>
          </div>
        </details>

        <details style={{ marginTop: 12 }}>
          <summary className="muted" style={{ cursor: "pointer", fontWeight: 600 }}>
            🎸 Advanced: Guitar Setup
          </summary>
          <div style={{ marginTop: 10 }}>
            <div className="row">
              <label>Active pickup</label>
              <select
                value={props.pickupActive ?? ""}
                onChange={(e) => {
                  const v = e.target.value as PickupPosition | "";
                  props.setPickupActive(v ? (v as PickupPosition) : null);
                }}
              >
                <option value="">(unspecified)</option>
                <option value="bridge">Bridge</option>
                <option value="middle">Middle</option>
                <option value="neck">Neck</option>
              </select>
            </div>
            <div className="row">
              <label>Bridge</label>
              <input
                value={props.pickupBridge}
                onChange={(e) => props.setPickupBridge(e.target.value)}
                placeholder="e.g. Seymour Duncan SH-4 JB (humbucker)"
              />
            </div>
            <div className="row">
              <label>Middle</label>
              <input
                value={props.pickupMiddle}
                onChange={(e) => props.setPickupMiddle(e.target.value)}
                placeholder="e.g. Single coil (stock)"
              />
            </div>
            <div className="row">
              <label>Neck</label>
              <input
                value={props.pickupNeck}
                onChange={(e) => props.setPickupNeck(e.target.value)}
                placeholder="e.g. EMG 60 (humbucker)"
              />
            </div>
            <div className="muted">
              Used as tone context only (not a plugin parameter).
            </div>
          </div>
        </details>

        {props.snapshots.length ? (
          <>
            <div className="divider" />
            <h3 style={{ marginTop: 12, marginBottom: 10, fontSize: 13, fontWeight: 600 }}>
              ★ Saved Snapshots ({props.snapshots.length})
            </h3>
            <div className="diffList" style={{ maxHeight: 220, overflow: "auto" }}>
              {props.snapshots.map((s) => (
                <div
                  key={s.id}
                  className="diffRow"
                  style={{
                    alignItems: "center",
                    background: "rgba(69, 255, 181, 0.03)",
                    border: "1px solid rgba(69, 255, 181, 0.15)"
                  }}
                >
                  <div className="diffLabel" style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontWeight: 500 }}>{s.label}</div>
                    <div className="muted" style={{ marginTop: 4, fontSize: 11 }}>
                      {formatTime(s.ts)}
                    </div>
                  </div>
                  <div style={{ display: "flex", gap: 6 }}>
                    <button
                      className="btn btnSmall"
                      type="button"
                      onClick={() => props.onRestoreSnapshot(s)}
                      aria-label={`Load ${s.label} snapshot`}
                    >
                      Load
                    </button>
                    <button
                      className="btn btnSmall btnApply"
                      type="button"
                      disabled={!props.selectedFxGuid || !s.state.preview?.params?.length}
                      title={
                        !props.selectedFxGuid
                          ? "Select a target Gojira plugin first"
                          : !s.state.preview?.params?.length
                            ? "Snapshot has no parameters to apply"
                            : "Apply snapshot to REAPER"
                      }
                      onClick={() => props.onApplySnapshot(s)}
                      aria-label={`Apply ${s.label} snapshot to REAPER`}
                    >
                      Apply
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </>
        ) : null}
      </div>
    </aside>
  );
}
