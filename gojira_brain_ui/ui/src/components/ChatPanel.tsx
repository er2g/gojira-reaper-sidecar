import React, { useEffect, useRef } from "react";
import type { ChatMessage } from "../workspace";
import { formatTime } from "../workspace";

export default function ChatPanel(props: {
  chat: ChatMessage[];
  composer: string;
  setComposer: (v: string) => void;
  busy: boolean;
  refineActive: boolean;
  canSend: boolean;
  canApply: boolean;
  pendingApply: boolean;

  onSend: () => void;
  onApply: () => void;

  tab: "preview" | "qc" | "mapping";
  setTab: (t: "preview" | "qc" | "mapping") => void;

  onRevertToMessage: (id: string) => void;
  onSnapshotMessage: (id: string) => void;
}) {
  const listRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const el = listRef.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight });
  }, [props.chat.length]);

  return (
    <main className="panel chat">
      <div className="panelHeader">
        <div className="panelTitle">
          <h2>AI Chat</h2>
          <div className="muted">
            {props.refineActive ? "Editing current tone (delta only)" : "Generating a fresh tone"}
          </div>
        </div>
        <div className="muted">{props.busy ? "Working…" : ""}</div>
      </div>

      <div className="chatWrap">
        <div className="messageList" ref={listRef}>
          {props.chat.map((m) => (
            <div key={m.id} className={`bubble ${m.role === "user" ? "bubbleUser" : "bubbleAssistant"}`}>
              {m.content}
              <div className="bubbleMeta">
                <span>{m.role === "user" ? "You" : "AI"}</span>
                <span style={{ display: "flex", gap: 10, alignItems: "center" }}>
                  <button
                    className="btn"
                    style={{ padding: "4px 8px" }}
                    type="button"
                    onClick={() => props.onRevertToMessage(m.id)}
                  >
                    Revert
                  </button>
                  <button
                    className="btn"
                    style={{ padding: "4px 8px" }}
                    type="button"
                    onClick={() => props.onSnapshotMessage(m.id)}
                  >
                    Snapshot
                  </button>
                  <span>{formatTime(m.ts)}</span>
                </span>
              </div>
            </div>
          ))}
        </div>

        <div className="composer">
          <textarea
            value={props.composer}
            onChange={(e) => props.setComposer(e.target.value)}
            placeholder='Ask for a specific tone… or tweak the current one (e.g. "törpüle biraz, high-mid daha az, gate biraz daha az").'
          />
          <div className="composerActions">
            <div className="segmented" aria-label="Inspector">
              <button
                className={`segBtn ${props.tab === "preview" ? "segBtnActive" : ""}`}
                type="button"
                onClick={() => props.setTab("preview")}
              >
                Preview
              </button>
              <button
                className={`segBtn ${props.tab === "qc" ? "segBtnActive" : ""}`}
                type="button"
                onClick={() => props.setTab("qc")}
              >
                Applied QC
              </button>
              <button
                className={`segBtn ${props.tab === "mapping" ? "segBtnActive" : ""}`}
                type="button"
                onClick={() => props.setTab("mapping")}
              >
                Mapping
              </button>
            </div>

            <div className="composerButtons">
              <button className="btn primary" disabled={props.busy || !props.canSend} onClick={props.onSend} type="button">
                {props.busy ? "Generating…" : "Send"}
              </button>
              <button
                className="btn"
                disabled={props.busy || !props.canApply}
                onClick={props.onApply}
                type="button"
                title={props.pendingApply ? "Waiting for ACK…" : "Apply to REAPER"}
              >
                {props.pendingApply ? "Applying…" : "Apply"}
              </button>
            </div>
          </div>
        </div>
      </div>
    </main>
  );
}
