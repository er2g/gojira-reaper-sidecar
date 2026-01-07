import { invoke } from "@tauri-apps/api/core";
import React from "react";
import type { StatusEvent } from "../types";

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
        <span className="statusText">
          {status.status === "connected"
            ? "Connected to Reaper"
            : status.status === "connecting"
              ? "Connecting..."
              : `Disconnected${retryText}`}
        </span>
      </div>
      <div className="statusRight">
        <button
          className="btn"
          onClick={() => invoke("connect_ws")}
          type="button"
        >
          Reconnect Now
        </button>
      </div>
    </div>
  );
}
