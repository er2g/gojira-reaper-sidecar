import React, { useState } from "react";

export type IndexRemapEntry = { from: number; to: number; label: string };

const knownRoles: Array<IndexRemapEntry> = [
  { label: "Delay: Active", from: 101, to: 101 },
  { label: "Delay: Mix", from: 105, to: 105 },
  { label: "Delay: Time", from: 108, to: 108 },
  { label: "Reverb: Active", from: 112, to: 112 },
  { label: "Reverb: Mix", from: 114, to: 114 },
  { label: "Reverb: Time", from: 115, to: 115 },
  { label: "Noise Gate", from: 2, to: 2 },
  { label: "Amp Type Selector", from: 29, to: 29 },
  { label: "Overdrive: Active", from: 13, to: 13 },
];

export default function IndexMappingEditor({
  remap,
  onChange,
}: {
  remap: Record<number, number>;
  onChange: (next: Record<number, number>) => void;
  validationReport?: Record<string, string>;
}) {
  const [customFrom, setCustomFrom] = useState("");
  const [customTo, setCustomTo] = useState("");

  function set(from: number, to: number) {
    const next = { ...remap };
    if (from === to) {
      delete next[from];
    } else {
      next[from] = to;
    }
    onChange(next);
  }

  function resetAll() {
    onChange({});
  }

  function addCustom() {
    const f = Number(customFrom);
    const t = Number(customTo);
    if (!Number.isFinite(f) || !Number.isFinite(t)) return;
    set(f, t);
    setCustomFrom("");
    setCustomTo("");
  }

  return (
    <div>
      <div className="row" style={{ justifyContent: "space-between" }}>
        <div className="muted">Canonical → Actual (only stored when different)</div>
        <button className="btn danger" type="button" onClick={resetAll}>
          Reset mapping
        </button>
      </div>

      <div className="diffList">
        {knownRoles.map((r) => {
          const current = remap[r.from] ?? r.from;
          return (
            <div key={r.from} className="diffRow">
              <div className="diffLabel">
                <span className="badge">#{r.from}</span> {r.label}
                <div className="muted" style={{ marginTop: 6 }}>
                  Current actual index: <b>{current}</b>
                </div>
              </div>
              <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                <input
                  style={{ width: 120, flex: "none" }}
                  type="number"
                  value={String(current)}
                  onChange={(e) => set(r.from, Number(e.target.value))}
                />
              </div>
            </div>
          );
        })}
      </div>

      <h3>Custom mapping</h3>
      <div className="row">
        <label>From</label>
        <input
          value={customFrom}
          onChange={(e) => setCustomFrom(e.target.value)}
          placeholder="e.g. 106"
          type="number"
        />
        <label style={{ width: 70 }}>To</label>
        <input
          value={customTo}
          onChange={(e) => setCustomTo(e.target.value)}
          placeholder="e.g. 105"
          type="number"
        />
        <button className="btn" type="button" onClick={addCustom}>
          Add
        </button>
      </div>
    </div>
  );
}

