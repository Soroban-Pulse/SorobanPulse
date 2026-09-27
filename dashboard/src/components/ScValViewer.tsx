import { useState } from "react";
import { TruncatedText } from "./TruncatedText";

type ViewMode = "tree" | "table" | "code";

interface ScValViewerProps {
  data: Record<string, unknown>;
}

/**
 * Renders nested ScVal data in three view modes:
 * - Tree: collapsible key-value tree
 * - Table: flat key-value table
 * - Code: raw JSON block
 */
export function ScValViewer({ data }: ScValViewerProps) {
  const [mode, setMode] = useState<ViewMode>("tree");

  const jsonString = JSON.stringify(data, null, 2);

  return (
    <div className="scval-viewer">
      <div className="scval-viewer-tabs">
        <button
          className={`scval-tab ${mode === "tree" ? "scval-tab--active" : ""}`}
          onClick={() => setMode("tree")}
        >
          Tree
        </button>
        <button
          className={`scval-tab ${mode === "table" ? "scval-tab--active" : ""}`}
          onClick={() => setMode("table")}
        >
          Table
        </button>
        <button
          className={`scval-tab ${mode === "code" ? "scval-tab--active" : ""}`}
          onClick={() => setMode("code")}
        >
          Code
        </button>
      </div>

      <div className="scval-viewer-content">
        {mode === "tree" && <TreeViewer data={data} depth={0} />}
        {mode === "table" && <TableView data={data} />}
        {mode === "code" && (
          <pre className="scval-code">
            <code>{jsonString}</code>
          </pre>
        )}
      </div>
    </div>
  );
}

function TreeViewer({ data, depth }: { data: Record<string, unknown>; depth: number }) {
  const entries = Object.entries(data);

  if (entries.length === 0) {
    return <span className="scval-empty">{"{}"}</span>;
  }

  return (
    <div className="scval-tree" style={{ paddingLeft: depth > 0 ? "1rem" : undefined }}>
      {entries.map(([key, value]) => (
        <div key={key} className="scval-tree-node">
          <span className="scval-key">{key}:</span>{" "}
          {renderValue(value)}
        </div>
      ))}
    </div>
  );
}

function TableView({ data }: { data: Record<string, unknown> }) {
  const entries = Object.entries(data);

  if (entries.length === 0) {
    return <span className="scval-empty">No data</span>;
  }

  return (
    <table className="scval-table">
      <tbody>
        {entries.map(([key, value]) => (
          <tr key={key}>
            <td className="scval-table-key">{key}</td>
            <td className="scval-table-value">{formatScalar(value)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function renderValue(value: unknown): JSX.Element {
  if (value === null) return <span className="scval-null">null</span>;
  if (typeof value === "boolean") return <span className="scval-bool">{String(value)}</span>;
  if (typeof value === "number") return <span className="scval-number">{String(value)}</span>;
  if (typeof value === "string") {
    // Truncate long strings (IDs, hashes)
    if (value.length > 16) {
      return <TruncatedText text={value} maxChars={16} />;
    }
    return <span className="scval-string">"{value}"</span>;
  }
  if (Array.isArray(value)) {
    return (
      <span className="scval-array">
        [{value.map((item) => renderValue(item)).reduce((prev, curr, i) => (
          <>
            {prev}
            {i > 0 && <span className="scval-comma">, </span>}
            {curr}
          </>
        ), <></>)}]
      </span>
    );
  }
  if (typeof value === "object") {
    return <TreeViewer data={value as Record<string, unknown>} depth={1} />;
  }
  return <span>{String(value)}</span>;
}

function formatScalar(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "string" && value.length > 16) {
    return value; // TruncatedText handles display in the table cell
  }
  return JSON.stringify(value);
}
