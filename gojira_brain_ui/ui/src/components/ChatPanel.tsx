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
          <h2>Tone Generator</h2>
          <div className="muted">
            {props.busy ? "Creating your tone..." : props.refineActive ? "Tweaking current tone" : "Ready to create"}
          </div>
        </div>
      </div>

      <div className="chatWrap">
        <div className="messageList" ref={listRef}>
          {props.chat.map((m) => (
            <div key={m.id} className={`bubble ${m.role === "user" ? "bubbleUser" : "bubbleAssistant"}`}>
              {m.content}
              <div className="bubbleMeta">
                <span>{m.role === "user" ? "You" : "Gojira AI"}</span>
                <span style={{ display: "flex", gap: 8, alignItems: "center" }}>
                  <button
                    className="btn btnSmall"
                    type="button"
                    onClick={() => props.onRevertToMessage(m.id)}
                    title="Go back to this point"
                    aria-label="Revert to this message"
                  >
                    ↩ Go Back
                  </button>
                  <button
                    className="btn btnSmall"
                    type="button"
                    onClick={() => props.onSnapshotMessage(m.id)}
                    title="Save this tone for later"
                    aria-label="Save snapshot"
                  >
                    ★ Save
                  </button>
                  <span className="timestamp">{formatTime(m.ts)}</span>
                </span>
              </div>
            </div>
          ))}
        </div>

        <div className="composer">
          <textarea
            value={props.composer}
            onChange={(e) => props.setComposer(e.target.value)}
            onKeyDown={(e) => {
              if (e.key !== "Enter") return;
              if (e.shiftKey) return;
              e.preventDefault();
              if (props.busy) return;
              if (!props.canSend) return;
              props.onSend();
            }}
            placeholder="Describe the tone you want... (e.g., 'heavy modern djent rhythm' or 'add more high-mid, reduce gate')"
            aria-label="Tone description input"
          />
          <div className="composerActions">
            <div className="segmented" aria-label="View options">
              <button
                className={`segBtn ${props.tab === "preview" ? "segBtnActive" : ""}`}
                type="button"
                onClick={() => props.setTab("preview")}
                aria-label="Preview tab"
              >
                Preview
              </button>
              <button
                className={`segBtn ${props.tab === "qc" ? "segBtnActive" : ""}`}
                type="button"
                onClick={() => props.setTab("qc")}
                aria-label="Quality check tab"
              >
                Quality Check
              </button>
              <button
                className={`segBtn ${props.tab === "mapping" ? "segBtnActive" : ""}`}
                type="button"
                onClick={() => props.setTab("mapping")}
                aria-label="Parameter mapping tab"
              >
                Settings
              </button>
            </div>

            <div className="composerButtons">
              <button
                className="btn primary"
                disabled={props.busy || !props.canSend}
                onClick={props.onSend}
                type="button"
                aria-label={props.busy ? "Generating tone" : "Generate tone"}
              >
                {props.busy ? "⚡ Generating..." : "⚡ Generate"}
              </button>
              <button
                className="btn btnApply"
                disabled={props.busy || !props.canApply}
                onClick={props.onApply}
                type="button"
                title={props.pendingApply ? "Applying to REAPER..." : "Apply tone to REAPER"}
                aria-label={props.pendingApply ? "Applying tone" : "Apply tone to REAPER"}
              >
                {props.pendingApply ? "⟳ Applying..." : "✓ Apply to REAPER"}
              </button>
            </div>
          </div>
        </div>
      </div>
    </main>
  );
}
