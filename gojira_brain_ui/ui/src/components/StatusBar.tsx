import React from "react";
import type { StatusEvent } from "../types";
import { isTauriRuntime, tauriInvoke as invoke } from "../platform/tauri";

const dotColor: Record<StatusEvent["status"], string> = {
  disconnected: "#999",
  connecting: "#f0c000",
  connected: "#45ffb5",
};

const statusIcon: Record<StatusEvent["status"], string> = {
  disconnected: "⚠️",
  connecting: "⟳",
  connected: "✓",
};

export default function StatusBar({ status }: { status: StatusEvent }) {
  const retryText =
    status.status === "disconnected" && status.retry_in
      ? ` Retrying in ${status.retry_in}s...`
      : "";

  return (
    <div className="statusBar">
      <div className="statusLeft">
        <span
          className="dot"
          style={{ backgroundColor: dotColor[status.status] }}
          aria-label={`Status: ${status.status}`}
        />
        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          <span className="statusText" style={{ fontWeight: 500 }}>
            {statusIcon[status.status]}{" "}
            {status.status === "connected"
              ? "Connected to REAPER"
              : status.status === "connecting"
                ? "Connecting to REAPER..."
                : `Not connected${retryText}`}
          </span>
          {status.status === "disconnected" ? (
            <span className="muted" style={{ fontSize: 11, opacity: 0.7 }}>
              💡 Make sure REAPER is running with Archetype Gojira loaded
            </span>
          ) : null}
        </div>
      </div>
      <div className="statusRight">
        <button
          className="btn btnSmall"
          onClick={() => {
            if (!isTauriRuntime()) return;
            void (async () => {
              try { await invoke("disconnect_ws"); } catch {}
              try { await invoke("connect_ws"); } catch {}
            })();
          }}
          type="button"
          disabled={!isTauriRuntime()}
          aria-label="Reconnect to REAPER"
        >
          ⟳ Reconnect
        </button>
      </div>
    </div>
  );
}
