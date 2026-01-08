import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Store } from "@tauri-apps/plugin-store";
import React, { useEffect, useMemo, useRef, useState } from "react";
import ChatPanel from "./components/ChatPanel";
import InspectorPanel from "./components/InspectorPanel";
import SidebarPanel from "./components/SidebarPanel";
import StatusBar from "./components/StatusBar";
import type { AckMessage, GojiraInstance, HandshakePayload, PreviewResult, StatusEvent } from "./types";
import { buildPromptFromChat, initialWorkspace, mergeParamLists, nowId, type ChatMessage, type HistoryEntry, type SavedSnapshot, type WorkspaceState } from "./workspace";
import { summarizeAppliedDelta } from "./workspace";

const store = new Store("prefs.bin");

export default function App() {
  const [status, setStatus] = useState<StatusEvent>({ status: "connecting" });
  const [instances, setInstances] = useState<GojiraInstance[]>([]);
  const [selectedFxGuid, setSelectedFxGuid] = useState<string>("");
  const [validationReport, setValidationReport] = useState<Record<string, string>>({});
  const [paramEnums, setParamEnums] = useState<Record<string, Array<{ value: number; label: string }>>>({});
  const [paramFormats, setParamFormats] = useState<Record<string, { min: string; mid: string; max: string }>>({});
  const [paramFormatSamples, setParamFormatSamples] = useState<
    Record<string, Array<{ norm: number; formatted: string }>>
  >({});
  const [indexRemap, setIndexRemap] = useState<Record<number, number>>({});  

  const [vaultPassphrase, setVaultPassphrase] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [apiKeyPresent, setApiKeyPresent] = useState<boolean | null>(null);

  const [busy, setBusy] = useState(false);
  const [previewOnly, setPreviewOnly] = useState(true);
  const [refineEnabled, setRefineEnabled] = useState(true);
  const [tab, setTab] = useState<"preview" | "qc" | "mapping">("preview");
  const [composer, setComposer] = useState("Make me a dry modern djent rhythm tone.");

  const [history, setHistory] = useState<HistoryEntry[]>(() => {
    const st = initialWorkspace();
    return [{ ts: Date.now(), label: "init", anchorMessageId: st.chat[0]?.id, state: st }];
  });
  const [cursor, setCursor] = useState(0);
  const [snapshots, setSnapshots] = useState<SavedSnapshot[]>([]);

  const workspace = history[cursor]?.state ?? initialWorkspace();
  const canUndo = cursor > 0;
  const canRedo = cursor < history.length - 1;

  const cursorRef = useRef(0);
  useEffect(() => {
    cursorRef.current = cursor;
  }, [cursor]);

  const workspaceRef = useRef(workspace);
  useEffect(() => {
    workspaceRef.current = workspace;
  }, [workspace]);

  const pendingApplyIdRef = useRef<string | null>(null);
  const [pendingApplyCommandId, setPendingApplyCommandId] = useState<string | null>(null);

  function commit(next: WorkspaceState, meta: { label: string; anchorMessageId?: string }) {
    const idx = cursorRef.current;
    setHistory((prev) => {
      const trimmed = prev.slice(0, idx + 1);
      return [...trimmed, { ts: Date.now(), label: meta.label, anchorMessageId: meta.anchorMessageId, state: next }];
    });
    setCursor(idx + 1);
  }

  function clearPendingApply() {
    pendingApplyIdRef.current = null;
    setPendingApplyCommandId(null);
  }

  function undo() {
    setCursor((c) => Math.max(0, c - 1));
    clearPendingApply();
  }

  function redo() {
    setCursor((c) => Math.min(history.length - 1, c + 1));
    clearPendingApply();
  }

  function newChat() {
    const st = initialWorkspace();
    commit(st, { label: "new chat", anchorMessageId: st.chat[0]?.id });
    setComposer("");
    setTab("preview");
    clearPendingApply();
  }

  function jumpToMessage(messageId: string) {
    const idx = history.findIndex((h) => h.anchorMessageId === messageId);
    if (idx >= 0) {
      setCursor(idx);
      clearPendingApply();
    }
  }

  function snapshotMessage(messageId: string) {
    const idx = history.findIndex((h) => h.anchorMessageId === messageId);
    const st = (idx >= 0 ? history[idx]?.state : workspace) ?? workspace;
    const msg = st.chat.find((m) => m.id === messageId);
    const label = msg ? `Snapshot — ${msg.role === "user" ? "User" : "AI"}` : "Snapshot";
    setSnapshots((prev) => [{ id: nowId("snap"), ts: Date.now(), label, state: st }, ...prev]);
  }

  function restoreSnapshot(s: SavedSnapshot) {
    commit(s.state, { label: `restore: ${s.label}` });
    setTab("preview");
    clearPendingApply();
  }

  const selectedInstance = useMemo(
    () => instances.find((i) => i.fx_guid === selectedFxGuid) ?? null,
    [instances, selectedFxGuid],
  );

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
          setParamFormatSamples(e.payload.param_format_samples ?? {});

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
          const w = workspaceRef.current;
          commit({ ...w, preview: null, lastAck: null }, { label: "project changed" });
          setTab("preview");
          clearPendingApply();
        }),
      );

      unlistenFns.push(
        await listen<AckMessage>("reaper://ack", (e) => {
          const msg = e.payload;
          if (pendingApplyIdRef.current && msg.command_id === pendingApplyIdRef.current) {
            clearPendingApply();
          }
          const w = workspaceRef.current;
          commit({ ...w, lastAck: msg }, { label: "ack" });
          setTab("qc");
        }),
      );

      unlistenFns.push(
        await listen<any>("reaper://error", (e) => {
          const msg = e.payload as { type?: string; msg?: string; code?: string };
          const text = msg?.msg ? `REAPER error: ${msg.code ?? "error"} — ${msg.msg}` : "REAPER error";
          const m: ChatMessage = { id: nowId("m"), role: "assistant", ts: Date.now(), content: text };
          const w = workspaceRef.current;
          commit({ ...w, chat: [...w.chat, m] }, { label: "reaper error", anchorMessageId: m.id });
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
        entries: Object.entries(normalized).map(([from, to]) => ({ from: Number(from), to: Number(to) })),
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
        entries: Object.entries(indexRemap).map(([from, to]) => ({ from: Number(from), to: Number(to) })),
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

  const refineDisabled = !workspace.workingParams?.length;
  const refineActive = refineEnabled && !refineDisabled;

  const ackStats = useMemo(() => {
    const params = workspace.lastAck?.applied_params ?? [];
    if (!params.length) return { count: 0, mismatched: 0 };
    const threshold = 0.0005;
    const mismatched = params.filter((p) => Math.abs(p.applied - p.requested) > threshold).length;
    return { count: params.length, mismatched };
  }, [workspace.lastAck]);

  const appliedSorted = useMemo(() => {
    const items = (workspace.lastAck?.applied_params ?? []).slice();
    items.sort((a, b) => summarizeAppliedDelta(b).abs - summarizeAppliedDelta(a).abs);
    return items;
  }, [workspace.lastAck]);

  async function send() {
    if (!selectedFxGuid) return;
    const userText = composer.trim();
    if (!userText) return;

    const base = workspaceRef.current;
    const userMsg: ChatMessage = { id: nowId("m"), role: "user", ts: Date.now(), content: userText };
    const chatAfterUser = [...base.chat, userMsg];
    commit({ ...base, chat: chatAfterUser }, { label: "user message", anchorMessageId: userMsg.id });
    setComposer("");

    const mode: "merge" | "replace_active" = refineActive ? "merge" : "replace_active";

    setBusy(true);
    try {
      const prompt = buildPromptFromChat({
        messages: chatAfterUser,
        current: userText,
        refine: refineActive,
        baseParams: refineActive ? base.workingParams : null,
      });

      const res = await invoke<PreviewResult>("generate_tone", {
        targetFxGuid: selectedFxGuid,
        prompt,
        previewOnly,
        mode,
        baseParams: refineActive ? base.workingParams : null,
      });

      const assistantMsg: ChatMessage = {
        id: nowId("m"),
        role: "assistant",
        ts: Date.now(),
        content: res.reasoning || "(no reasoning)",
      };

      const nextWorking = (() => {
        if (!res.params?.length) return base.workingParams;
        if (mode === "merge" && base.workingParams?.length) {
          return mergeParamLists(base.workingParams, res.params);
        }
        return res.params;
      })();

      commit(
        {
          ...base,
          chat: [...chatAfterUser, assistantMsg],
          preview: res,
          lastGenMode: mode,
          workingParams: nextWorking ?? null,
        },
        { label: "ai reply", anchorMessageId: assistantMsg.id },
      );
      setTab("preview");
    } catch (err: any) {
      const assistantMsg: ChatMessage = {
        id: nowId("m"),
        role: "assistant",
        ts: Date.now(),
        content: `Generation failed: ${String(err)}`,
      };
      commit({ ...base, chat: [...chatAfterUser, assistantMsg] }, { label: "ai error", anchorMessageId: assistantMsg.id });
    } finally {
      setBusy(false);
    }
  }

  async function apply() {
    const w = workspaceRef.current;
    if (!w.preview || !selectedFxGuid) return;
    setBusy(true);
    try {
      const commandId = await invoke<string>("apply_tone", {
        targetFxGuid: selectedFxGuid,
        mode: w.lastGenMode,
        params: w.preview.params,
      });
      pendingApplyIdRef.current = commandId;
      setPendingApplyCommandId(commandId);
      setTab("qc");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="appShell">
      <StatusBar status={status} />
      <div className="appGrid">
        <SidebarPanel
          status={status.status}
          instances={instances}
          selectedFxGuid={selectedFxGuid}
          setSelectedFxGuid={setSelectedFxGuid}
          selectedInstance={selectedInstance}
          cursor={cursor}
          historyLen={history.length}
          canUndo={canUndo}
          canRedo={canRedo}
          onUndo={undo}
          onRedo={redo}
          onNewChat={newChat}
          previewOnly={previewOnly}
          setPreviewOnly={setPreviewOnly}
          refineEnabled={refineEnabled}
          setRefineEnabled={setRefineEnabled}
          refineDisabled={refineDisabled}
          vaultPassphrase={vaultPassphrase}
          setVaultPassphrase={setVaultPassphrase}
          apiKey={apiKey}
          setApiKey={setApiKey}
          apiKeyPresent={apiKeyPresent}
          onUnlockVault={unlockVault}
          onSaveKey={saveKey}
          onClearKey={clearKey}
          snapshots={snapshots}
          onRestoreSnapshot={restoreSnapshot}
        />

        <ChatPanel
          chat={workspace.chat}
          composer={composer}
          setComposer={setComposer}
          busy={busy}
          refineActive={refineActive}
          canSend={!!selectedFxGuid}
          canApply={!!selectedFxGuid && !!workspace.preview}
          pendingApply={!!pendingApplyCommandId}
          onSend={send}
          onApply={apply}
          tab={tab}
          setTab={setTab}
          onRevertToMessage={jumpToMessage}
          onSnapshotMessage={snapshotMessage}
        />

        <InspectorPanel
          tab={tab}
          setTab={setTab}
          preview={workspace.preview}
          lastGenMode={workspace.lastGenMode}
          lastAck={workspace.lastAck}
          appliedSorted={appliedSorted}
          ackStats={ackStats}
          validationReport={validationReport}
          indexRemap={indexRemap}
          setIndexRemap={setIndexRemap}
          paramEnums={paramEnums}
          paramFormats={paramFormats}
          paramFormatSamples={paramFormatSamples}
        />
      </div>
    </div>
  );
}
