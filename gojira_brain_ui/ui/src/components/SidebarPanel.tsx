import React from "react";
import type { ApiProviderOption, ProviderId } from "../apiProviders";
import type { GojiraInstance } from "../types";
import type { PickupPosition, SavedSnapshot } from "../workspace";
import { formatTime } from "../workspace";

export default function SidebarPanel(props: {
  status: "connecting" | "connected" | "disconnected";
  instances: GojiraInstance[];
  selectedFxGuid: string;
  setSelectedFxGuid: (v: string) => void;
  selectedInstance: GojiraInstance | null;

  cursor: number;
  historyLen: number;
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;
  onNewChat: () => void;

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
          <h2>Session</h2>
          <div className="muted">
            {props.instances.length ? `${props.instances.length} instance(s)` : "Waiting for REAPER…"}
          </div>
        </div>
        <span className="badge">{props.status}</span>
      </div>

      <div className="panelBody">
        <div className="row">
          <label>Target</label>
          <select value={props.selectedFxGuid} onChange={(e) => props.setSelectedFxGuid(e.target.value)}>
            {props.instances.map((i) => (
              <option key={i.fx_guid} value={i.fx_guid}>
                {(i.track_name || "(Track)") + " — " + (i.fx_name || "Archetype Gojira")} ({i.confidence})
              </option>
            ))}
          </select>
        </div>

        {props.selectedInstance ? (
          <div className="muted">
            <div>Track: {props.selectedInstance.track_name || "(unnamed)"}</div>
            <div>FX: {props.selectedInstance.fx_name || "Archetype Gojira"}</div>
          </div>
        ) : (
          <div className="muted">Open a REAPER project with Archetype Gojira loaded.</div>
        )}

        <div className="divider" />

        <div className="row" style={{ marginBottom: 0, justifyContent: "space-between" }}>
          <div style={{ display: "flex", gap: 8 }}>
            <button className="btn" disabled={!props.canUndo} type="button" onClick={props.onUndo}>
              Undo
            </button>
            <button className="btn" disabled={!props.canRedo} type="button" onClick={props.onRedo}>
              Redo
            </button>
          </div>
          <button className="btn" type="button" onClick={props.onNewChat}>
            New chat
          </button>
        </div>

        <div className="muted" style={{ marginTop: 8 }}>
          Timeline: {props.cursor + 1}/{props.historyLen}
        </div>

        <div className="divider" />

        <div className="row" style={{ marginBottom: 0 }}>
          <label className="checkbox">
            <input checked={props.previewOnly} onChange={(e) => props.setPreviewOnly(e.target.checked)} type="checkbox" />
            Preview only
          </label>
        </div>

        <div className="row" style={{ marginTop: 10 }}>
          <label>Mode</label>
          <div className="segmented" style={{ width: "100%", justifyContent: "space-between" }}>
            <button
              className={`segBtn ${!props.refineEnabled || props.refineDisabled ? "segBtnActive" : ""}`}
              type="button"
              onClick={() => props.setRefineEnabled(false)}
            >
              New tone
            </button>
            <button
              className={`segBtn ${props.refineEnabled && !props.refineDisabled ? "segBtnActive" : ""}`}
              type="button"
              disabled={props.refineDisabled}
              onClick={() => props.setRefineEnabled(true)}
              title={props.refineDisabled ? "Generate at least one tone first" : "Refine current tone"}
            >
              Refine current
            </button>
          </div>
        </div>

        <details style={{ marginTop: 10 }}>
          <summary className="muted" style={{ cursor: "pointer" }}>
            Credentials / models
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

        <details style={{ marginTop: 10 }}>
          <summary className="muted" style={{ cursor: "pointer" }}>
            Guitar / pickups
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
            <div className="muted" style={{ marginBottom: 8 }}>
              Snapshots
            </div>
            <div className="diffList" style={{ maxHeight: 240, overflow: "auto" }}>
              {props.snapshots.map((s) => (
                <div key={s.id} className="diffRow" style={{ alignItems: "center" }}>
                  <div className="diffLabel">
                    {s.label}
                    <div className="muted" style={{ marginTop: 4 }}>
                      {formatTime(s.ts)}
                    </div>
                  </div>
                  <div style={{ display: "flex", gap: 8 }}>
                    <button className="btn" type="button" onClick={() => props.onRestoreSnapshot(s)}>
                      Load
                    </button>
                    <button
                      className="btn"
                      type="button"
                      disabled={!props.selectedFxGuid || !s.state.preview?.params?.length}
                      title={
                        !props.selectedFxGuid
                          ? "Select a target Gojira instance first."
                          : !s.state.preview?.params?.length
                            ? "Snapshot has no preview params to apply."
                            : "Apply snapshot params to REAPER."
                      }
                      onClick={() => props.onApplySnapshot(s)}
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
