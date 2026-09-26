import { useState } from "react";

/** Collapsible, theme-aware JSON viewer. Colours come from --color-code-* tokens. */
export function JsonTree({ value, collapseDepth = 2 }: { value: unknown; collapseDepth?: number }) {
  return (
    <div className="json-tree">
      <Node value={value} depth={0} collapseDepth={collapseDepth} />
    </div>
  );
}

function Node({ name, value, depth, collapseDepth }: { name?: string; value: unknown; depth: number; collapseDepth: number }) {
  const [open, setOpen] = useState(depth < collapseDepth);
  const label = name !== undefined ? <span className="json-key">{JSON.stringify(name)}: </span> : null;

  if (value === null || typeof value !== "object") {
    return (
      <div>
        <span style={{ display: "inline-block", width: 14 }} />
        {label}
        <Primitive value={value} />
      </div>
    );
  }

  const isArray = Array.isArray(value);
  const entries = isArray ? (value as unknown[]).map((v, i) => [String(i), v] as const) : Object.entries(value);
  const [openBr, closeBr] = isArray ? ["[", "]"] : ["{", "}"];

  return (
    <div>
      <button type="button" className="toggle" onClick={() => setOpen(!open)} aria-expanded={open}>
        {open ? "▾" : "▸"}
      </button>
      {label}
      {openBr}
      {!open && (
        <>
          <span className="muted"> {entries.length} {isArray ? "items" : "keys"} </span>
          {closeBr}
        </>
      )}
      {open && (
        <>
          <ul>
            {entries.map(([k, v]) => (
              <li key={k}>
                <Node name={isArray ? undefined : k} value={v} depth={depth + 1} collapseDepth={collapseDepth} />
              </li>
            ))}
          </ul>
          <span style={{ display: "inline-block", width: 14 }} />
          {closeBr}
        </>
      )}
    </div>
  );
}

function Primitive({ value }: { value: unknown }) {
  if (value === null || value === undefined) return <span className="json-null">null</span>;
  if (typeof value === "string") return <span className="json-string break">{JSON.stringify(value)}</span>;
  if (typeof value === "number") return <span className="json-number">{value}</span>;
  if (typeof value === "boolean") return <span className="json-boolean">{String(value)}</span>;
  return <span>{String(value)}</span>;
}
