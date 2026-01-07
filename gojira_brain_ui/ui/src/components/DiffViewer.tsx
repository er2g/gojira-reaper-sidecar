import React from "react";
import type { DiffItem } from "../types";

function fmt(v: number | null) {
  if (v === null) return "∅";
  return v.toFixed(3);
}

export default function DiffViewer({ items }: { items: DiffItem[] }) {
  if (!items.length) {
    return <div className="muted">No changes.</div>;
  }

  return (
    <div className="diffList">
      {items.map((it) => {
        const isUp =
          it.old_value === null
            ? true
            : it.new_value !== null && it.new_value > it.old_value;
        return (
          <div key={`${it.index}`} className="diffRow">
            <div className="diffLabel">
              <span className="badge">#{it.index}</span> {it.label}
            </div>
            <div className={`diffValue ${isUp ? "up" : "down"}`}>
              {fmt(it.old_value)} ➔ {fmt(it.new_value)}
            </div>
          </div>
        );
      })}
    </div>
  );
}
