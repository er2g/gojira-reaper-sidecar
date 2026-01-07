import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { Store } from "@tauri-apps/plugin-store";
import React, { useEffect, useMemo, useState } from "react";
import StatusBar from "./components/StatusBar";
import DiffViewer from "./components/DiffViewer";
import IndexMappingEditor from "./components/IndexMappingEditor";
import type { GojiraInstance, HandshakePayload, PreviewResult, StatusEvent } from "./types";

const store = new Store("prefs.bin");

export default function App() {
  const [status, setStatus] = useState<StatusEvent>({ status: "connecting" });
  const [instances, setInstances] = useState<GojiraInstance[]>([]);
  const [selectedFxGuid, setSelectedFxGuid] = useState<string>("");
  const [validationReport, setValidationReport] = useState<Record<string, string>>(
    {},
  );
  const [indexRemap, setIndexRemap] = useState<Record<number, number>>({});

  const [vaultPassphrase, setVaultPassphrase] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [apiKeyPresent, setApiKeyPresent] = useState<boolean | null>(null);

  const [prompt, setPrompt] = useState("Make me a dry modern djent rhythm tone.");
  const [previewOnly, setPreviewOnly] = useState(true);
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<PreviewResult | null>(null);

  const selectedInstance = useMemo(
    () => instances.find((i) => i.fx_guid === selectedFxGuid) ?? null,
    [instances, selectedFxGuid],
  );

  useEffect(() => {
    let unlistenFns: Array<() => void> = [];

    (async () => {
      unlistenFns.push(
        await listen<StatusEvent>("reaper://status", (e) => setStatus(e.payload)),
      );
      unlistenFns.push(
        await listen<HandshakePayload>("reaper://handshake", async (e) => {
          setInstances(e.payload.instances);
          setValidationReport(e.payload.validation_report ?? {});
          const last = (await store.get<string>("last_target_fx_guid")) ?? "";
          const next =
            (last &&
              e.payload.instances.find((x) => x.fx_guid === last)?.fx_guid) ??
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
        }),
      );

      await invoke("connect_ws");

      const saved =
        (await store.get<Record<string, number>>("index_remap_v1")) ?? {};
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
    setBusy(true);
    try {
      const res = await invoke<PreviewResult>("generate_tone", {
        targetFxGuid: selectedFxGuid,
        prompt,
        previewOnly,
      });
      setPreview(res);
    } finally {
      setBusy(false);
    }
  }

  async function doApply() {
    if (!preview || !selectedFxGuid) return;
    setBusy(true);
    try {
      await invoke("apply_tone", {
        targetFxGuid: selectedFxGuid,
        mode: "replace_active",
        params: preview.params,
      });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="page">
      <StatusBar status={status} />

      <div className="grid">
        <section className="card">
          <h2>Connection</h2>
          <div className="row">
            <label>Target Instance</label>
            <select
              value={selectedFxGuid}
              onChange={(e) => setSelectedFxGuid(e.target.value)}
            >
              {instances.map((i) => (
                <option key={i.fx_guid} value={i.fx_guid}>
                  {i.track_name || "(Track)"} — {i.fx_name || "Archetype Gojira"} (
                  {i.confidence})
                </option>
              ))}
            </select>
          </div>
          {selectedInstance ? (
            <div className="muted">
              Track GUID: {selectedInstance.track_guid}
              <br />
              FX GUID: {selectedInstance.fx_guid}
            </div>
          ) : (
            <div className="muted">
              No instances yet. Open a Reaper project with Archetype Gojira loaded.
            </div>
          )}
        </section>

        <section className="card">
          <h2>Security</h2>
          <div className="row">
            <label>Vault Passphrase</label>
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
            <label>Gemini API Key</label>
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
        </section>

        <section className="card span2">
          <h2>Tone Generator</h2>
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            rows={5}
            placeholder="Describe the tone..."
          />
          <div className="row">
            <label className="checkbox">
              <input
                checked={previewOnly}
                onChange={(e) => setPreviewOnly(e.target.checked)}
                type="checkbox"
              />
              Preview Only
            </label>
            <button className="btn primary" disabled={busy} onClick={doGenerate} type="button">
              {busy ? "Working..." : "Generate"}
            </button>
            {previewOnly && preview ? (
              <button className="btn" disabled={busy} onClick={doApply} type="button">
                Apply
              </button>
            ) : null}
          </div>
        </section>

        <section className="card span2">
          <h2>Engineer's Notes</h2>
          <div className="notes">{preview?.reasoning || "—"}</div>
          <h3>Diff</h3>
          <DiffViewer items={preview?.diff ?? []} />
        </section>

        <section className="card span2">
          <h2>Index Mapping</h2>
          <div className="muted" style={{ marginBottom: 10 }}>
            Validator report:{" "}
            {Object.keys(validationReport).length
              ? Object.entries(validationReport)
                  .map(([k, v]) => `${k}: ${v}`)
                  .join(" | ")
              : "—"}
          </div>
          <IndexMappingEditor
            remap={indexRemap}
            onChange={setIndexRemap}
            validationReport={validationReport}
          />
        </section>
      </div>
    </div>
  );
}
