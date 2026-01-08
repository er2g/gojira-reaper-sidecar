import React from "react";
import type { GojiraInstance } from "../types";
import type { SavedSnapshot } from "../workspace";
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

  vaultPassphrase: string;
  setVaultPassphrase: (v: string) => void;
  apiKey: string;
  setApiKey: (v: string) => void;
  apiKeyPresent: boolean | null;
  onUnlockVault: () => void;
  onSaveKey: () => void;
  onClearKey: () => void;

  snapshots: SavedSnapshot[];
  onRestoreSnapshot: (s: SavedSnapshot) => void;
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
            Security / API key
          </summary>
          <div style={{ marginTop: 10 }}>
            <div className="row">
              <label>Vault passphrase</label>
              <input
                value={props.vaultPassphrase}
                onChange={(e) => props.setVaultPassphrase(e.target.value)}
                type="password"
                placeholder="Passphrase (not your API key)"
              />
              <button className="btn" onClick={props.onUnlockVault} type="button">
                Unlock
              </button>
            </div>
            <div className="row">
              <label>Gemini API key</label>
              <input value={props.apiKey} onChange={(e) => props.setApiKey(e.target.value)} type="password" placeholder="AIza..." />
              <button className="btn" onClick={props.onSaveKey} type="button">
                Save
              </button>
              <button className="btn danger" onClick={props.onClearKey} type="button">
                Clear
              </button>
            </div>
            <div className="muted">
              Stored in Stronghold vault:{" "}
              {props.apiKeyPresent === null ? "unknown" : props.apiKeyPresent ? "yes" : "no"}
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
                      Restore
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

