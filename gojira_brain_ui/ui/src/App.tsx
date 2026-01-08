import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Store } from "@tauri-apps/plugin-store";
import React, { useEffect, useMemo, useRef, useState } from "react";
import DiffViewer from "./components/DiffViewer";
import IndexMappingEditor from "./components/IndexMappingEditor";
import StatusBar from "./components/StatusBar";
import type {
  AckMessage,
  AppliedParam,
  GojiraInstance,
  HandshakePayload,
  PreviewResult,
  StatusEvent,
} from "./types";

const store = new Store("prefs.bin");

type ChatRole = "user" | "assistant";
type ChatMessage = {
  id: string;
  role: ChatRole;
  content: string;
  ts: number;
};

function nowId(prefix: string) {
  return `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function formatTime(ts: number) {
  try {
    return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  } catch {
    return "";
  }
}

function buildPromptFromChat(messages: ChatMessage[], current: string) {
  const recent = messages.filter((m) => m.content.trim()).slice(-6);
  const lines: string[] = [];
  if (recent.length) {
    lines.push("CONVERSATION CONTEXT (for tone continuity, do not repeat verbatim):");
    for (const m of recent) {
      const head = m.role === "user" ? "USER" : "ASSISTANT";
      lines.push(`${head}: ${m.content.trim()}`);
    }
    lines.push("");
  }
  lines.push("CURRENT REQUEST:");
  lines.push(current.trim());
  return lines.join("\n");
}

function summarizeDelta(a: AppliedParam) {
  const d = a.applied - a.requested;
  return { delta: d, abs: Math.abs(d) };
}

export default function App() {
  const [status, setStatus] = useState<StatusEvent>({ status: "connecting" });
  const [instances, setInstances] = useState<GojiraInstance[]>([]);
  const [selectedFxGuid, setSelectedFxGuid] = useState<string>("");
  const [validationReport, setValidationReport] = useState<Record<string, string>>({});
  const [paramEnums, setParamEnums] = useState<
    Record<string, Array<{ value: number; label: string }>>
  >({});
  const [paramFormats, setParamFormats] = useState<Record<string, { min: string; mid: string; max: string }>>(
    {},
  );
  const [indexRemap, setIndexRemap] = useState<Record<number, number>>({});

  const [vaultPassphrase, setVaultPassphrase] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [apiKeyPresent, setApiKeyPresent] = useState<boolean | null>(null);

  const [previewOnly, setPreviewOnly] = useState(true);
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<PreviewResult | null>(null);

  const [tab, setTab] = useState<"preview" | "qc" | "mapping">("preview");
  const [lastAck, setLastAck] = useState<AckMessage | null>(null);
  const [pendingApplyCommandId, setPendingApplyCommandId] = useState<string | null>(null);
  const pendingApplyIdRef = useRef<string | null>(null);

  const [chat, setChat] = useState<ChatMessage[]>(() => [
    {
      id: nowId("m"),
      role: "assistant",
      ts: Date.now(),
      content:
        "Describe a tone (band + era + guitar + tuning + pick attack), then hit Generate.\n\nTip: If you set Cab mic IR by label and keep mic Level around -12 dB, it stays clean and avoids clipping.",
    },
  ]);
  const [composer, setComposer] = useState("Make me a dry modern djent rhythm tone.");

  const selectedInstance = useMemo(
    () => instances.find((i) => i.fx_guid === selectedFxGuid) ?? null,
    [instances, selectedFxGuid],
  );

  const listRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const el = listRef.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight });
  }, [chat.length]);

  useEffect(() => {
    let unlistenFns: Array<() => void> = [];

    (async () => {
      unlistenFns.push(await listen<StatusEvent>("reaper://status", (e) => setStatus(e.payload)));

      unlistenFns.push(
        await listen<HandshakePayload>("reaper://handshake", async (e) => {
          setInstances(e.payload.instances);
          setValidationReport(e.payload.validation_report ?? {});
          setParamEnums(e.payload.param_enums ?? {});
          setParamFormats(e.payload.param_formats ?? {});

          const last = (await store.get<string>("last_target_fx_guid")) ?? "";
          const next =
            (last && e.payload.instances.find((x) => x.fx_guid === last)?.fx_guid) ??
            e.payload.instances[0]?.fx_guid ??
            "";
          setSelectedFxGuid(next);
          await store.set("last_target_fx_guid", next);
          await store.save();
        }),
      );

      unlistenFns.push(
        await listen("reaper://project_changed", () => {
          setPreview(null);
          setLastAck(null);
          setPendingApplyCommandId(null);
        }),
      );

      unlistenFns.push(
        await listen<AckMessage>("reaper://ack", (e) => {
          const msg = e.payload;
          setLastAck(msg);
          if (pendingApplyIdRef.current && msg.command_id === pendingApplyIdRef.current) {
            setPendingApplyCommandId(null);
            pendingApplyIdRef.current = null;
          }
          setTab("qc");
        }),
      );

      unlistenFns.push(
        await listen<any>("reaper://error", (e) => {
          const msg = e.payload as { type?: string; msg?: string; code?: string };
          const text = msg?.msg ? `REAPER error: ${msg.code ?? "error"} — ${msg.msg}` : "REAPER error";
          setChat((prev) => [
            ...prev,
            { id: nowId("m"), role: "assistant", ts: Date.now(), content: text },
          ]);
        }),
      );

      await invoke("connect_ws");

      const saved = (await store.get<Record<string, number>>("index_remap_v1")) ?? {};
      const normalized: Record<number, number> = {};
      for (const [k, v] of Object.entries(saved)) {
        const from = Number(k);
        const to = Number(v);
        if (Number.isFinite(from) && Number.isFinite(to) && from !== to) {
          normalized[from] = to;
        }
      }
      setIndexRemap(normalized);
      await invoke("set_index_remap", {
        entries: Object.entries(normalized).map(([from, to]) => ({
          from: Number(from),
          to: Number(to),
        })),
      });
    })();

    return () => {
      unlistenFns.forEach((f) => f());
      unlistenFns = [];
    };
  }, []);

  useEffect(() => {
    void (async () => {
      const toStore: Record<string, number> = {};
      for (const [from, to] of Object.entries(indexRemap)) {
        toStore[from] = to;
      }
      await store.set("index_remap_v1", toStore);
      await store.save();
      await invoke("set_index_remap", {
        entries: Object.entries(indexRemap).map(([from, to]) => ({
          from: Number(from),
          to: Number(to),
        })),
      });
    })();
  }, [indexRemap]);

  useEffect(() => {
    if (!selectedFxGuid) return;
    void (async () => {
      await store.set("last_target_fx_guid", selectedFxGuid);
      await store.save();
    })();
  }, [selectedFxGuid]);

  async function unlockVault() {
    await invoke("set_vault_passphrase", { passphrase: vaultPassphrase });
    try {
      const ok = await invoke<boolean>("has_api_key");
      setApiKeyPresent(ok);
    } catch {
      setApiKeyPresent(null);
    }
  }

  async function saveKey() {
    await invoke("save_api_key", { apiKey });
    setApiKey("");
    setApiKeyPresent(true);
  }

  async function clearKey() {
    await invoke("clear_api_key");
    setApiKeyPresent(false);
  }

  async function doGenerate() {
    if (!selectedFxGuid) return;
    const userText = composer.trim();
    if (!userText) return;

    setChat((prev) => [...prev, { id: nowId("m"), role: "user", ts: Date.now(), content: userText }]);
    setBusy(true);
    try {
      const fullPrompt = buildPromptFromChat(chat, userText);
      const res = await invoke<PreviewResult>("generate_tone", {
        targetFxGuid: selectedFxGuid,
        prompt: fullPrompt,
        previewOnly,
      });
      setPreview(res);
      setTab("preview");
      setChat((prev) => [
        ...prev,
        { id: nowId("m"), role: "assistant", ts: Date.now(), content: res.reasoning || "(no reasoning)" },
      ]);
    } finally {
      setBusy(false);
    }
  }

  async function doApply() {
    if (!preview || !selectedFxGuid) return;
    setBusy(true);
    try {
      const commandId = await invoke<string>("apply_tone", {
        targetFxGuid: selectedFxGuid,
        mode: "replace_active",
        params: preview.params,
      });
      setPendingApplyCommandId(commandId);
      pendingApplyIdRef.current = commandId;
      setTab("qc");
    } finally {
      setBusy(false);
    }
  }

  function resetChat() {
    setChat([
      {
        id: nowId("m"),
        role: "assistant",
        ts: Date.now(),
        content:
          "New chat. Ask for a tone with references (band/era/tuning), and I’ll generate a preset + show applied readback QC when you apply.",
      },
    ]);
    setPreview(null);
    setLastAck(null);
    setPendingApplyCommandId(null);
    pendingApplyIdRef.current = null;
    setComposer("");
    setTab("preview");
  }

  const ackStats = useMemo(() => {
    const params = lastAck?.applied_params ?? [];
    if (!params.length) return { count: 0, mismatched: 0 };
    const threshold = 0.0005;
    const mismatched = params.filter((p) => Math.abs(p.applied - p.requested) > threshold).length;
    return { count: params.length, mismatched };
  }, [lastAck]);

  const appliedSorted = useMemo(() => {
    const items = (lastAck?.applied_params ?? []).slice();
    items.sort((a, b) => summarizeDelta(b).abs - summarizeDelta(a).abs);
    return items;
  }, [lastAck]);

  return (
    <div className="appShell">
      <StatusBar status={status} />

      <div className="appGrid">
        <aside className="panel sidebar">
          <div className="panelHeader">
            <div className="panelTitle">
              <h2>Session</h2>
              <div className="muted">
                {instances.length ? `${instances.length} instance(s)` : "Waiting for REAPER…"}
              </div>
            </div>
            <span className="badge">{status.status}</span>
          </div>
          <div className="panelBody">
            <div className="row">
              <label>Target</label>
              <select value={selectedFxGuid} onChange={(e) => setSelectedFxGuid(e.target.value)}>
                {instances.map((i) => (
                  <option key={i.fx_guid} value={i.fx_guid}>
                    {(i.track_name || "(Track)") + " — " + (i.fx_name || "Archetype Gojira")} ({i.confidence})
                  </option>
                ))}
              </select>
            </div>

            {selectedInstance ? (
              <div className="muted">
                <div>Track: {selectedInstance.track_name || "(unnamed)"}</div>
                <div>FX: {selectedInstance.fx_name || "Archetype Gojira"}</div>
              </div>
            ) : (
              <div className="muted">Open a REAPER project with Archetype Gojira loaded.</div>
            )}

            <div className="divider" />

            <div className="row" style={{ marginBottom: 0 }}>
              <label className="checkbox">
                <input checked={previewOnly} onChange={(e) => setPreviewOnly(e.target.checked)} type="checkbox" />
                Preview only
              </label>
              <button className="btn" type="button" onClick={resetChat}>
                New chat
              </button>
            </div>

            <div className="divider" />

            <details>
              <summary className="muted" style={{ cursor: "pointer" }}>
                Security / API key
              </summary>
              <div style={{ marginTop: 10 }}>
                <div className="row">
                  <label>Vault passphrase</label>
                  <input
                    value={vaultPassphrase}
                    onChange={(e) => setVaultPassphrase(e.target.value)}
                    type="password"
                    placeholder="Passphrase (not your API key)"
                  />
                  <button className="btn" onClick={unlockVault} type="button">
                    Unlock
                  </button>
                </div>
                <div className="row">
                  <label>Gemini API key</label>
                  <input
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    type="password"
                    placeholder="AIza..."
                  />
                  <button className="btn" onClick={saveKey} type="button">
                    Save
                  </button>
                  <button className="btn danger" onClick={clearKey} type="button">
                    Clear
                  </button>
                </div>
                <div className="muted">
                  Stored in Stronghold vault:{" "}
                  {apiKeyPresent === null ? "unknown" : apiKeyPresent ? "yes" : "no"}
                </div>
              </div>
            </details>
          </div>
        </aside>

        <main className="panel chat">
          <div className="panelHeader">
            <div className="panelTitle">
              <h2>AI Chat</h2>
              <div className="muted">Generate a tone preset from conversation context.</div>
            </div>
            <div className="muted">{busy ? "Working…" : ""}</div>
          </div>
          <div className="chatWrap">
            <div className="messageList" ref={listRef}>
              {chat.map((m) => (
                <div
                  key={m.id}
                  className={`bubble ${m.role === "user" ? "bubbleUser" : "bubbleAssistant"}`}
                >
                  {m.content}
                  <div className="bubbleMeta">
                    <span>{m.role === "user" ? "You" : "AI"}</span>
                    <span>{formatTime(m.ts)}</span>
                  </div>
                </div>
              ))}
            </div>

            <div className="composer">
              <textarea
                value={composer}
                onChange={(e) => setComposer(e.target.value)}
                placeholder="Ask for a specific tone… (e.g. “Gojira – FMTS era raw crunch rhythm, drop C, tight gate, no fizz, Cab 2, Dynamic 421 off-axis, mic level -12 dB”)"
              />
              <div className="composerActions">
                <div className="segmented" aria-label="Actions">
                  <button className={`segBtn ${tab === "preview" ? "segBtnActive" : ""}`} type="button" onClick={() => setTab("preview")}>
                    Preview
                  </button>
                  <button className={`segBtn ${tab === "qc" ? "segBtnActive" : ""}`} type="button" onClick={() => setTab("qc")}>
                    Applied QC
                  </button>
                  <button className={`segBtn ${tab === "mapping" ? "segBtnActive" : ""}`} type="button" onClick={() => setTab("mapping")}>
                    Mapping
                  </button>
                </div>
                <div style={{ display: "flex", gap: 10, alignItems: "center" }}>
                  <button className="btn primary" disabled={busy || !selectedFxGuid} onClick={doGenerate} type="button">
                    {busy ? "Generating…" : "Generate"}
                  </button>
                  <button className="btn" disabled={busy || !preview || !selectedFxGuid} onClick={doApply} type="button">
                    {pendingApplyCommandId ? "Applying…" : "Apply"}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </main>

        <aside className="panel inspector">
          <div className="panelHeader">
            <div className="panelTitle">
              <h2>Inspector</h2>
              <div className="muted">
                {tab === "preview"
                  ? "Preview + diff"
                  : tab === "qc"
                    ? `Readback QC (${ackStats.mismatched}/${ackStats.count} mismatched)`
                    : "Index mapping / meta"}
              </div>
            </div>
            <div className="tabs">
              <button className={`tab ${tab === "preview" ? "tabActive" : ""}`} type="button" onClick={() => setTab("preview")}>
                Preview
              </button>
              <button className={`tab ${tab === "qc" ? "tabActive" : ""}`} type="button" onClick={() => setTab("qc")}>
                QC
              </button>
              <button className={`tab ${tab === "mapping" ? "tabActive" : ""}`} type="button" onClick={() => setTab("mapping")}>
                Mapping
              </button>
            </div>
          </div>

          <div className="panelBody" style={{ padding: 0 }}>
            <div style={{ display: tab === "preview" ? "block" : "none", padding: "12px 14px" }}>
              <h3>Engineer’s Notes</h3>
              <div className="notes">{preview?.reasoning || "Generate a tone to see reasoning."}</div>
              <h3>Diff</h3>
              <DiffViewer items={preview?.diff ?? []} />
            </div>

            <div style={{ display: tab === "qc" ? "block" : "none", padding: "12px 14px" }}>
              {!lastAck ? (
                <div className="muted">No applied readback yet. Hit Apply to see REAPER readback.</div>
              ) : (
                <>
                  <div className="muted" style={{ marginBottom: 10 }}>
                    Ack: <span className="badge">{lastAck.command_id}</span>
                  </div>
                  <div style={{ maxHeight: 520, overflow: "auto" }}>
                    <table className="table">
                      <thead>
                        <tr>
                          <th style={{ width: 64 }}>Idx</th>
                          <th style={{ width: 92 }}>Req</th>
                          <th style={{ width: 92 }}>Applied</th>
                          <th style={{ width: 90 }}>Δ</th>
                          <th>Formatted</th>
                        </tr>
                      </thead>
                      <tbody>
                        {appliedSorted.map((p) => {
                          const d = p.applied - p.requested;
                          const cls = Math.abs(d) > 0.0005 ? "deltaBad" : "deltaGood";
                          return (
                            <tr key={`ap:${p.index}`}>
                              <td>#{p.index}</td>
                              <td>{p.requested.toFixed(6)}</td>
                              <td>{p.applied.toFixed(6)}</td>
                              <td className={cls}>{d >= 0 ? "+" : ""}{d.toFixed(6)}</td>
                              <td style={{ whiteSpace: "nowrap" }}>{p.formatted || ""}</td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                </>
              )}
            </div>

            <div style={{ display: tab === "mapping" ? "block" : "none", padding: "12px 14px" }}>
              <h3>Index Mapping</h3>
              <div className="muted" style={{ marginBottom: 10 }}>
                Validator report:{" "}
                {Object.keys(validationReport).length
                  ? Object.entries(validationReport)
                      .map(([k, v]) => `${k}: ${v}`)
                      .join(" | ")
                  : "(disabled)"}
              </div>
              <IndexMappingEditor remap={indexRemap} onChange={setIndexRemap} validationReport={validationReport} />

              <details style={{ marginTop: 12 }}>
                <summary className="muted" style={{ cursor: "pointer" }}>
                  Cab / IR options (from REAPER)
                </summary>
                <div className="muted" style={{ marginTop: 8 }}>
                  Cab Type (84): {paramEnums["84"]?.length ?? 0} | Mic IR Cab1 (92):{" "}
                  {paramEnums["92"]?.length ?? 0} | Mic IR Cab2 (99): {paramEnums["99"]?.length ?? 0}
                </div>
                <div style={{ marginTop: 8 }}>
                  {paramEnums["84"]?.length ? (
                    <div style={{ marginBottom: 10 }}>
                      <div className="muted">Cab Type (84)</div>
                      <div style={{ maxHeight: 160, overflow: "auto" }}>
                        {paramEnums["84"].map((o) => (
                          <div key={`84:${o.value}`}>
                            {o.label} <span className="muted">({o.value.toFixed(3)})</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  ) : null}

                  {paramEnums["92"]?.length ? (
                    <div style={{ marginBottom: 10 }}>
                      <div className="muted">Cab 1 Mic IR (92)</div>
                      <div style={{ maxHeight: 160, overflow: "auto" }}>
                        {paramEnums["92"].map((o) => (
                          <div key={`92:${o.value}`}>
                            {o.label} <span className="muted">({o.value.toFixed(3)})</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  ) : null}

                  {paramEnums["99"]?.length ? (
                    <div style={{ marginBottom: 10 }}>
                      <div className="muted">Cab 2 Mic IR (99)</div>
                      <div style={{ maxHeight: 160, overflow: "auto" }}>
                        {paramEnums["99"].map((o) => (
                          <div key={`99:${o.value}`}>
                            {o.label} <span className="muted">({o.value.toFixed(3)})</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  ) : null}

                  {paramFormats["87"] ||
                  paramFormats["88"] ||
                  paramFormats["89"] ||
                  paramFormats["94"] ||
                  paramFormats["95"] ||
                  paramFormats["96"] ? (
                    <div style={{ marginBottom: 10 }}>
                      <div className="muted">Formatted value examples</div>
                      {["87", "88", "89", "94", "95", "96"].map((k) =>
                        paramFormats[k] ? (
                          <div key={`fmt:${k}`}>
                            idx {k}: min="{paramFormats[k].min}", mid="{paramFormats[k].mid}", max="{paramFormats[k].max}"
                          </div>
                        ) : null,
                      )}
                    </div>
                  ) : null}
                </div>
              </details>
            </div>
          </div>
        </aside>
      </div>
    </div>
  );
}
