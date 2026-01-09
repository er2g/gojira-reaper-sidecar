import React from "react";
import type { StatusEvent } from "../types";
import { isTauriRuntime, tauriInvoke as invoke } from "../platform/tauri";

const dotColor: Record<StatusEvent["status"], string> = {
  disconnected: "#777",
  connecting: "#f0c000",
  connected: "#00b070",
};

export default function StatusBar({ status }: { status: StatusEvent }) {
  const retryText =
    status.status === "disconnected" && status.retry_in
      ? ` (Retrying in ${status.retry_in}s...)`
      : "";

  return (
    <div className="statusBar">
      <div className="statusLeft">
        <span
          className="dot"
          style={{ backgroundColor: dotColor[status.status] }}
        />
        <div style={{ display: "flex", flexDirection: "column" }}>
          <span className="statusText">
            {status.status === "connected"
              ? "Connected to Reaper"
              : status.status === "connecting"
                ? "Connecting..."
                : `Disconnected${retryText}`}
          </span>
          {status.status === "disconnected" ? (
            <span className="muted" style={{ fontSize: 12 }}>
              Tip: REAPER loads extension DLLs on startup. If you installed/updated the DLL while REAPER was open, restart REAPER.
            </span>
          ) : null}
        </div>
      </div>
      <div className="statusRight">
        <button
          className="btn"
          onClick={() => {
            if (!isTauriRuntime()) return;
            void (async () => {
              try { await invoke("disconnect_ws"); } catch {}
              try { await invoke("connect_ws"); } catch {}
            })();
          }}
          type="button"
          disabled={!isTauriRuntime()}
        >
          Reconnect Now
        </button>
      </div>
    </div>
  );
}
