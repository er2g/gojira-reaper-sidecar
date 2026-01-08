import React from "react";
import DiffViewer from "./DiffViewer";
import IndexMappingEditor from "./IndexMappingEditor";
import type { AckMessage, AppliedParam, ParamChange, PreviewResult } from "../types";

export default function InspectorPanel(props: {
  tab: "preview" | "qc" | "mapping";
  setTab: (t: "preview" | "qc" | "mapping") => void;

  preview: PreviewResult | null;
  lastGenMode: "replace_active" | "merge";
  lastAck: AckMessage | null;
  appliedSorted: AppliedParam[];
  ackStats: { count: number; mismatched: number };

  validationReport: Record<string, string>;
  indexRemap: Record<number, number>;
  setIndexRemap: (m: Record<number, number>) => void;
  paramEnums: Record<string, Array<{ value: number; label: string }>>;
  paramFormats: Record<string, { min: string; mid: string; max: string }>;
}) {
  return (
    <aside className="panel inspector">
      <div className="panelHeader">
        <div className="panelTitle">
          <h2>Inspector</h2>
          <div className="muted">
            {props.tab === "preview"
              ? "Preview + diff"
              : props.tab === "qc"
                ? `Readback QC (${props.ackStats.mismatched}/${props.ackStats.count} mismatched)`
                : "Index mapping / meta"}
          </div>
        </div>
        <div className="tabs">
          <button className={`tab ${props.tab === "preview" ? "tabActive" : ""}`} type="button" onClick={() => props.setTab("preview")}>
            Preview
          </button>
          <button className={`tab ${props.tab === "qc" ? "tabActive" : ""}`} type="button" onClick={() => props.setTab("qc")}>
            QC
          </button>
          <button className={`tab ${props.tab === "mapping" ? "tabActive" : ""}`} type="button" onClick={() => props.setTab("mapping")}>
            Mapping
          </button>
        </div>
      </div>

      <div className="panelBody" style={{ padding: 0 }}>
        <div style={{ display: props.tab === "preview" ? "block" : "none", padding: "12px 14px" }}>
          <h3>Engineer’s Notes</h3>
          <div className="notes">{props.preview?.reasoning || "Generate a tone to see reasoning."}</div>
          <h3>Diff</h3>
          <DiffViewer items={props.preview?.diff ?? []} />
          <div className="muted" style={{ marginTop: 10 }}>
            {props.lastGenMode === "merge"
              ? "Preview shows changes vs current preset; Apply sends only deltas."
              : "Preview is a full preset; Apply replaces active chain."}
          </div>
        </div>

        <div style={{ display: props.tab === "qc" ? "block" : "none", padding: "12px 14px" }}>
          {!props.lastAck ? (
            <div className="muted">No applied readback yet. Hit Apply to see REAPER readback.</div>
          ) : (
            <>
              <div className="muted" style={{ marginBottom: 10 }}>
                Ack: <span className="badge">{props.lastAck.command_id}</span>
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
                    {props.appliedSorted.map((p) => {
                      const d = p.applied - p.requested;
                      const cls = Math.abs(d) > 0.0005 ? "deltaBad" : "deltaGood";
                      return (
                        <tr key={`ap:${p.index}`}>
                          <td>#{p.index}</td>
                          <td>{p.requested.toFixed(6)}</td>
                          <td>{p.applied.toFixed(6)}</td>
                          <td className={cls}>
                            {d >= 0 ? "+" : ""}
                            {d.toFixed(6)}
                          </td>
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

        <div style={{ display: props.tab === "mapping" ? "block" : "none", padding: "12px 14px" }}>
          <h3>Index Mapping</h3>
          <div className="muted" style={{ marginBottom: 10 }}>
            Validator report:{" "}
            {Object.keys(props.validationReport).length
              ? Object.entries(props.validationReport)
                  .map(([k, v]) => `${k}: ${v}`)
                  .join(" | ")
              : "(disabled)"}
          </div>
          <IndexMappingEditor remap={props.indexRemap} onChange={props.setIndexRemap} validationReport={props.validationReport} />

          <details style={{ marginTop: 12 }}>
            <summary className="muted" style={{ cursor: "pointer" }}>
              Cab / IR options (from REAPER)
            </summary>
            <div className="muted" style={{ marginTop: 8 }}>
              Cab Type (84): {props.paramEnums["84"]?.length ?? 0} | Mic IR Cab1 (92):{" "}
              {props.paramEnums["92"]?.length ?? 0} | Mic IR Cab2 (99): {props.paramEnums["99"]?.length ?? 0}
            </div>
            <div style={{ marginTop: 8 }}>
              {props.paramEnums["84"]?.length ? (
                <div style={{ marginBottom: 10 }}>
                  <div className="muted">Cab Type (84)</div>
                  <div style={{ maxHeight: 160, overflow: "auto" }}>
                    {props.paramEnums["84"].map((o) => (
                      <div key={`84:${o.value}`}>
                        {o.label} <span className="muted">({o.value.toFixed(3)})</span>
                      </div>
                    ))}
                  </div>
                </div>
              ) : null}

              {props.paramEnums["92"]?.length ? (
                <div style={{ marginBottom: 10 }}>
                  <div className="muted">Cab 1 Mic IR (92)</div>
                  <div style={{ maxHeight: 160, overflow: "auto" }}>
                    {props.paramEnums["92"].map((o) => (
                      <div key={`92:${o.value}`}>
                        {o.label} <span className="muted">({o.value.toFixed(3)})</span>
                      </div>
                    ))}
                  </div>
                </div>
              ) : null}

              {props.paramEnums["99"]?.length ? (
                <div style={{ marginBottom: 10 }}>
                  <div className="muted">Cab 2 Mic IR (99)</div>
                  <div style={{ maxHeight: 160, overflow: "auto" }}>
                    {props.paramEnums["99"].map((o) => (
                      <div key={`99:${o.value}`}>
                        {o.label} <span className="muted">({o.value.toFixed(3)})</span>
                      </div>
                    ))}
                  </div>
                </div>
              ) : null}

              {props.paramFormats["87"] ||
              props.paramFormats["88"] ||
              props.paramFormats["89"] ||
              props.paramFormats["94"] ||
              props.paramFormats["95"] ||
              props.paramFormats["96"] ? (
                <div style={{ marginBottom: 10 }}>
                  <div className="muted">Formatted value examples</div>
                  {["87", "88", "89", "94", "95", "96"].map((k) =>
                    props.paramFormats[k] ? (
                      <div key={`fmt:${k}`}>
                        idx {k}: min="{props.paramFormats[k].min}", mid="{props.paramFormats[k].mid}", max="{props.paramFormats[k].max}"
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
  );
}

