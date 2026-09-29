import { useState, useRef, useEffect, useId } from "react";
import type { ColumnDef, ColumnId } from "../pages/EventsPage.tsx";

interface Props {
  columns: ColumnDef[];
  hiddenCols: Set<ColumnId>;
  onToggle: (id: ColumnId) => void;
}

/**
 * A toggle button that opens a small popover panel for showing/hiding
 * individual table columns.
 *
 * - Closes on outside click and on Escape
 * - Each checkbox is labelled and keyboard-accessible
 * - At least one column is always kept visible (the last visible col
 *   is disabled so the table is never blank)
 */
export function ColumnTogglePanel({ columns, hiddenCols, onToggle }: Props) {
  const [open, setOpen] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);
  const btnRef   = useRef<HTMLButtonElement>(null);
  const labelId  = useId();

  const visibleCount = columns.filter((c) => !hiddenCols.has(c.id)).length;

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    function handler(e: MouseEvent) {
      if (
        panelRef.current &&
        !panelRef.current.contains(e.target as Node) &&
        !btnRef.current?.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  // Close on Escape
  useEffect(() => {
    if (!open) return;
    function handler(e: KeyboardEvent) {
      if (e.key === "Escape") {
        setOpen(false);
        btnRef.current?.focus();
      }
    }
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open]);

  return (
    <div className="col-toggle-wrap" style={{ position: "relative" }}>
      <button
        ref={btnRef}
        className="btn col-toggle-btn"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label="Toggle column visibility"
        onClick={() => setOpen((o) => !o)}
      >
        ⊞ Columns
      </button>

      {open && (
        <div
          ref={panelRef}
          className="col-toggle-panel"
          role="dialog"
          aria-label="Column visibility"
          aria-labelledby={labelId}
        >
          <p id={labelId} className="col-toggle-panel__title">
            Columns
          </p>

          <ul className="col-toggle-panel__list" role="list">
            {columns.map((col) => {
              const checked   = !hiddenCols.has(col.id);
              // Prevent toggling off the last visible column
              const isLastVis = checked && visibleCount === 1;
              const inputId   = `col-toggle-${col.id}`;

              return (
                <li key={col.id} className="col-toggle-panel__item">
                  <label
                    htmlFor={inputId}
                    className={`col-toggle-panel__label${isLastVis ? " col-toggle-panel__label--disabled" : ""}`}
                  >
                    <input
                      id={inputId}
                      type="checkbox"
                      className="col-toggle-panel__checkbox"
                      checked={checked}
                      disabled={isLastVis}
                      onChange={() => onToggle(col.id)}
                      aria-label={`${checked ? "Hide" : "Show"} ${col.label} column`}
                    />
                    {col.label}
                  </label>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
}
