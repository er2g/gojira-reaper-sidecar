import React, { useEffect, useMemo, useRef, useState } from "react";
import ChatPanel from "./components/ChatPanel";
import InspectorPanel from "./components/InspectorPanel";
import SidebarPanel from "./components/SidebarPanel";
import StatusBar from "./components/StatusBar";
import { API_PROVIDERS, type ProviderId } from "./apiProviders";
import type { AckMessage, GojiraInstance, HandshakePayload, PreviewResult, StatusEvent } from "./types";
import { buildPromptFromChat, initialWorkspace, mergeParamLists, nowId, type ChatMessage, type HistoryEntry, type PickupPosition, type SavedSnapshot, type WorkspaceState } from "./workspace";
import { summarizeAppliedDelta } from "./workspace";
import { getPrefsStore, type PrefsStore } from "./platform/prefsStore";
import { isTauriRuntime, tauriInvoke as invoke, tauriListen as listen } from "./platform/tauri";

const store: PrefsStore = {
  get: async <T,>(key: string) => (await getPrefsStore()).get<T>(key),
  set: async <T,>(key: string, value: T) => (await getPrefsStore()).set<T>(key, value),
  delete: async (key: string) => (await getPrefsStore()).delete(key),
  save: async () => (await getPrefsStore()).save(),
};

export default function App() {
  const tauri = isTauriRuntime();
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

  const [pickupNeck, setPickupNeck] = useState("");
  const [pickupMiddle, setPickupMiddle] = useState("");
  const [pickupBridge, setPickupBridge] = useState("");
  const [pickupActive, setPickupActive] = useState<PickupPosition | null>(null);

  const [vaultPassphrase, setVaultPassphrase] = useState("");
  const [apiProvider, setApiProvider] = useState<ProviderId>("gemini");
  const [apiModel, setApiModel] = useState("");
  const [apiKeyDrafts, setApiKeyDrafts] = useState<Record<ProviderId, string>>(
    {} as Record<ProviderId, string>,
  );
  const [apiKeyPresence, setApiKeyPresence] = useState<Record<ProviderId, boolean>>(
    {} as Record<ProviderId, boolean>,
  );
  const [credentialsLoaded, setCredentialsLoaded] = useState(false);

  const [busy, setBusy] = useState(false);
  const [previewOnly, setPreviewOnly] = useState(false);
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

  const providerIds = useMemo<ProviderId[]>(() => API_PROVIDERS.map((p) => p.id), []);

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

  async function applySnapshot(s: SavedSnapshot) {
    const effectiveFxGuid = selectedInstance?.fx_guid ?? instances[0]?.fx_guid ?? "";
    if (!effectiveFxGuid) return;
    const params = s.state.preview?.params ?? null;
    if (!params?.length) return;

    setBusy(true);
    try {
      const commandId = await invoke<string>("apply_tone", {
        targetFxGuid: effectiveFxGuid,
        mode: s.state.lastGenMode ?? "merge",
        params,
      });
      pendingApplyIdRef.current = commandId;
      setPendingApplyCommandId(commandId);
      setTab("qc");
    } finally {
      setBusy(false);
    }
  }

  const selectedInstance = useMemo(
    () => instances.find((i) => i.fx_guid === selectedFxGuid) ?? null,
    [instances, selectedFxGuid],
  );

  useEffect(() => {
    if (!instances.length) return;
    if (selectedFxGuid && instances.some((i) => i.fx_guid === selectedFxGuid)) return;
    setSelectedFxGuid(instances[0]?.fx_guid ?? "");
  }, [instances]);

  useEffect(() => {
    let unlistenFns: Array<() => void> = [];

    (async () => {
      const tauri = isTauriRuntime();
      if (!tauri) setStatus({ status: "disconnected", retry_in: 0 });

      if (tauri) {
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
      }

      const pNeck = (await store.get<string>("pickup_neck_v1")) ?? "";
      const pMiddle = (await store.get<string>("pickup_middle_v1")) ?? "";
      const pBridge = (await store.get<string>("pickup_bridge_v1")) ?? "";
      const pActive = ((await store.get<string>("pickup_active_v1")) ?? "").trim();
      setPickupNeck(pNeck);
      setPickupMiddle(pMiddle);
      setPickupBridge(pBridge);
      if (pActive === "neck" || pActive === "middle" || pActive === "bridge") {
        setPickupActive(pActive as PickupPosition);
      } else {
        setPickupActive(null);
      }

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
      if (tauri) {
        await invoke("set_index_remap", {
          entries: Object.entries(normalized).map(([from, to]) => ({ from: Number(from), to: Number(to) })),
        });
      }

      const storedProvider = (await store.get<string>("llm_provider_v1")) ?? "gemini";
      const provider = API_PROVIDERS.find((p) => p.id === storedProvider)?.id ?? "gemini";
      setApiProvider(provider as ProviderId);

      const storedModel = (await store.get<string>("llm_model_v1")) ?? "";
      setApiModel(storedModel);
      setCredentialsLoaded(true);
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
      if (isTauriRuntime()) {
        await invoke("set_index_remap", {
          entries: Object.entries(indexRemap).map(([from, to]) => ({ from: Number(from), to: Number(to) })),
        });
      }
    })();
  }, [indexRemap]);

  useEffect(() => {
    if (!selectedFxGuid) return;
    void (async () => {
      await store.set("last_target_fx_guid", selectedFxGuid);
      await store.save();
    })();
  }, [selectedFxGuid]);

  useEffect(() => {
    if (!credentialsLoaded) return;
    void (async () => {
      await store.set("llm_provider_v1", apiProvider);
      await store.save();
    })();
  }, [apiProvider, credentialsLoaded]);

  useEffect(() => {
    if (!credentialsLoaded) return;
    void (async () => {
      await store.set("llm_model_v1", apiModel);
      await store.save();
    })();
  }, [apiModel, credentialsLoaded]);

  const pickupSaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (pickupSaveTimer.current) clearTimeout(pickupSaveTimer.current);
    pickupSaveTimer.current = setTimeout(() => {
      void (async () => {
        await store.set("pickup_neck_v1", pickupNeck);
        await store.set("pickup_middle_v1", pickupMiddle);
        await store.set("pickup_bridge_v1", pickupBridge);
        await store.set("pickup_active_v1", pickupActive ?? "");
        await store.save();
      })();
    }, 250);

    return () => {
      if (pickupSaveTimer.current) clearTimeout(pickupSaveTimer.current);
    };
  }, [pickupNeck, pickupMiddle, pickupBridge, pickupActive]);

  async function unlockVault() {
    if (!isTauriRuntime()) {
      setApiKeyPresence({} as Record<ProviderId, boolean>);
      return;
    }
    await invoke("set_vault_passphrase", { passphrase: vaultPassphrase });
    try {
      const presence = await invoke<Record<string, boolean>>("list_api_key_presence", {
        providers: providerIds,
      });
      const normalized: Record<ProviderId, boolean> = {} as Record<ProviderId, boolean>;
      for (const id of providerIds) {
        normalized[id] = !!(presence as Record<string, boolean>)[id];
      }
      setApiKeyPresence(normalized);
    } catch {
      setApiKeyPresence({} as Record<ProviderId, boolean>);
    }
  }

  function setApiKeyDraft(provider: ProviderId, value: string) {
    setApiKeyDrafts((prev) => ({ ...prev, [provider]: value }));
  }

  async function saveKey(provider: ProviderId) {
    if (!isTauriRuntime()) return;
    const value = (apiKeyDrafts[provider] ?? "").trim();
    if (!value) return;
    await invoke("save_api_key", { provider, apiKey: value });
    setApiKeyDrafts((prev) => ({ ...prev, [provider]: "" }));
    setApiKeyPresence((prev) => ({ ...prev, [provider]: true }));
  }

  async function clearKey(provider: ProviderId) {
    if (!isTauriRuntime()) return;
    await invoke("clear_api_key", { provider });
    setApiKeyPresence((prev) => ({ ...prev, [provider]: false }));
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
    if (!tauri) return;
    const userText = composer.trim();
    if (!userText) return;

    const effectiveFxGuid = selectedInstance?.fx_guid ?? instances[0]?.fx_guid ?? "";
    const base = workspaceRef.current;
    const userMsg: ChatMessage = { id: nowId("m"), role: "user", ts: Date.now(), content: userText };
    const chatAfterUser = [...base.chat, userMsg];
    commit({ ...base, chat: chatAfterUser }, { label: "user message", anchorMessageId: userMsg.id });
    setComposer("");

    const mode: "merge" | "replace_active" = refineActive ? "merge" : "replace_active";
    const noTargetSelected = !effectiveFxGuid;
    const effectivePreviewOnly = previewOnly || noTargetSelected;

    setBusy(true);
    try {
      const prompt = buildPromptFromChat({
        messages: chatAfterUser,
        current: userText,
        refine: refineActive,
        baseParams: refineActive ? base.workingParams : null,
        formats: paramFormats,
        samples: paramFormatSamples,
        pickups: {
          neck: pickupNeck,
          middle: pickupMiddle,
          bridge: pickupBridge,
          active: pickupActive,
        },
      });

      const res = await invoke<PreviewResult>("generate_tone", {
        targetFxGuid: effectiveFxGuid || "preview",
        prompt,
        previewOnly: effectivePreviewOnly,
        mode,
        baseParams: refineActive ? base.workingParams : null,
        provider: apiProvider,
        model: apiModel.trim() || null,
      });

      const assistantMsg: ChatMessage = {
        id: nowId("m"),
        role: "assistant",
        ts: Date.now(),
        content: res.reasoning || "(no reasoning)",
      };
      const noTargetMsg: ChatMessage | null =
        !previewOnly && noTargetSelected
          ? {
              id: nowId("m"),
              role: "assistant",
              ts: Date.now(),
              content: "No REAPER Gojira target is selected; generated in Preview-only mode.",
            }
          : null;

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
          chat: noTargetMsg ? [...chatAfterUser, noTargetMsg, assistantMsg] : [...chatAfterUser, assistantMsg],
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
    const effectiveFxGuid = selectedInstance?.fx_guid ?? instances[0]?.fx_guid ?? "";
    if (!w.preview || !effectiveFxGuid) return;
    setBusy(true);
    try {
      const commandId = await invoke<string>("apply_tone", {
        targetFxGuid: effectiveFxGuid,
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
          providers={API_PROVIDERS}
          apiProvider={apiProvider}
          setApiProvider={setApiProvider}
          apiModel={apiModel}
          setApiModel={setApiModel}
          apiKeyDrafts={apiKeyDrafts}
          setApiKeyDraft={setApiKeyDraft}
          apiKeyPresence={apiKeyPresence}
          onUnlockVault={unlockVault}
          onSaveKey={saveKey}
          onClearKey={clearKey}
          pickupNeck={pickupNeck}
          setPickupNeck={setPickupNeck}
          pickupMiddle={pickupMiddle}
          setPickupMiddle={setPickupMiddle}
          pickupBridge={pickupBridge}
          setPickupBridge={setPickupBridge}
          pickupActive={pickupActive}
          setPickupActive={setPickupActive}
          snapshots={snapshots}
          onRestoreSnapshot={restoreSnapshot}
          onApplySnapshot={applySnapshot}
        />

        <ChatPanel
          chat={workspace.chat}
          composer={composer}
          setComposer={setComposer}
          busy={busy}
          refineActive={refineActive}
          canSend={tauri}
          canApply={!!(selectedInstance?.fx_guid ?? instances[0]?.fx_guid) && !!workspace.preview}
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
