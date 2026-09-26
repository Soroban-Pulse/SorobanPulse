import { Fragment, useEffect, useId, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api, qs } from "../lib/api";
import { actionLabel, detect, isContractPrefix, KIND_LABEL, normalizeQuery, routeFor } from "../lib/detect";
import { formatNumber, shortId } from "../lib/format";
import { addRecent, clearRecent, useRecent } from "../lib/recent";
import type { ContractSearchResult } from "../lib/types";

interface Option {
  key: string;
  group: "action" | "contracts" | "recent";
  primary: string;
  secondary?: string;
  badge: string;
  query: string;
  kind: keyof typeof KIND_LABEL;
  path: string;
}

const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

function isTypingTarget(el: EventTarget | null): boolean {
  if (!(el instanceof HTMLElement)) return false;
  return el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName);
}

export function GlobalSearch() {
  const navigate = useNavigate();
  const recent = useRecent();
  const inputRef = useRef<HTMLInputElement>(null);
  const listId = useId();
  const [value, setValue] = useState("");
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [suggestions, setSuggestions] = useState<ContractSearchResult[]>([]);

  const detection = useMemo(() => detect(value), [value]);

  // Keyboard shortcuts: "/" and Cmd/Ctrl+K focus the search box.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const cmdK = e.key.toLowerCase() === "k" && (e.metaKey || e.ctrlKey);
      const slash = e.key === "/" && !e.metaKey && !e.ctrlKey && !e.altKey && !isTypingTarget(e.target);
      if (cmdK || slash) {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
        setOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Contract ID autocomplete from /v1/contracts/search (debounced).
  useEffect(() => {
    if (!isContractPrefix(value)) {
      setSuggestions([]);
      return;
    }
    const controller = new AbortController();
    const timer = setTimeout(async () => {
      try {
        const q = normalizeQuery(value).toUpperCase();
        const res = await api<{ data: ContractSearchResult[] }>(`/v1/contracts/search${qs({ q, limit: 8 })}`, {
          signal: controller.signal,
        });
        setSuggestions(res.data ?? []);
      } catch {
        setSuggestions([]);
      }
    }, 200);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [value]);

  const options: Option[] = useMemo(() => {
    const out: Option[] = [];
    if (detection.kind === "empty") {
      for (const r of recent) {
        out.push({
          key: `recent:${r.path}`,
          group: "recent",
          primary: r.query,
          badge: KIND_LABEL[r.kind],
          query: r.query,
          kind: r.kind,
          path: r.path,
        });
      }
      return out;
    }
    const path = routeFor(detection);
    const shown = detection.kind === "ledger" ? "" : String(detection.value);
    out.push({
      key: `action:${path}`,
      group: "action",
      primary: detection.kind === "ledger" ? actionLabel(detection) : `${actionLabel(detection)} ${shown}`,
      secondary: detection.kind === "text" ? detection.note : undefined,
      badge: KIND_LABEL[detection.kind],
      query: normalizeQuery(value),
      kind: detection.kind,
      path,
    });
    for (const s of suggestions) {
      if (s.contract_id === (detection.kind === "contract" ? detection.value : "")) continue;
      out.push({
        key: `contract:${s.contract_id}`,
        group: "contracts",
        primary: s.contract_id,
        secondary: `${formatNumber(s.event_count)} events`,
        badge: KIND_LABEL.contract,
        query: s.contract_id,
        kind: "contract",
        path: `/contracts/${s.contract_id}`,
      });
    }
    return out;
  }, [detection, recent, suggestions, value]);

  // Always keep the first option (the detected route) highlighted when the
  // input changes, so a paste followed by one Enter navigates correctly.
  useEffect(() => setActive(0), [value]);

  const go = (opt: Option) => {
    addRecent({ query: opt.query, kind: opt.kind, path: opt.path });
    navigate(opt.path);
    setValue("");
    setOpen(false);
    inputRef.current?.blur();
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown" && options.length) {
      e.preventDefault();
      setOpen(true);
      setActive((a) => (a + 1) % options.length);
    } else if (e.key === "ArrowUp" && options.length) {
      e.preventDefault();
      setActive((a) => (a - 1 + options.length) % options.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const opt = options[active] ?? options[0];
      if (opt) go(opt);
    } else if (e.key === "Escape") {
      if (value) setValue("");
      else inputRef.current?.blur();
      setOpen(false);
    }
  };

  const showMenu = open && (options.length > 0 || detection.kind === "empty");
  const activeId = options[active] ? `${listId}-${active}` : undefined;
  let lastGroup: Option["group"] | null = null;

  return (
    <div className="search">
      <label htmlFor={`${listId}-input`} className="visually-hidden">
        Search contracts, accounts, transactions, ledgers or text
      </label>
      <input
        id={`${listId}-input`}
        ref={inputRef}
        type="search"
        role="combobox"
        aria-expanded={showMenu}
        aria-controls={listId}
        aria-activedescendant={showMenu ? activeId : undefined}
        aria-autocomplete="list"
        autoComplete="off"
        spellCheck={false}
        placeholder="Search contract, account, tx hash, ledger…"
        value={value}
        onChange={(e) => {
          setValue(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 120)}
        onKeyDown={onKeyDown}
      />
      {detection.kind === "empty" ? (
        <span className="shortcut" aria-hidden>
          <kbd>{isMac ? "⌘K" : "Ctrl K"}</kbd>
        </span>
      ) : (
        <span className="search-kind badge accent" aria-live="polite">
          {KIND_LABEL[detection.kind]}
        </span>
      )}

      {showMenu && (
        <ul id={listId} role="listbox" className="search-menu" onMouseDown={(e) => e.preventDefault()}>
          {options.length === 0 && (
            <li className="search-hint">
              Paste a contract ID (C…), account (G… / M…), 64-hex tx hash or ledger number. Press <kbd>/</kbd> to
              search from anywhere.
            </li>
          )}
          {options.map((opt, i) => {
            const header =
              opt.group !== lastGroup ? (
                <li role="presentation" className="group">
                  {opt.group === "recent" ? "Recent searches" : opt.group === "contracts" ? "Contracts" : "Go to"}
                  {opt.group === "recent" && (
                    <button type="button" className="ghost" onClick={() => clearRecent()}>
                      Clear
                    </button>
                  )}
                </li>
              ) : null;
            lastGroup = opt.group;
            return (
              <Fragment key={opt.key}>
                {header}
                <li
                  id={`${listId}-${i}`}
                  role="option"
                  aria-selected={i === active}
                  className="search-option"
                  onMouseEnter={() => setActive(i)}
                  onClick={() => go(opt)}
                >
                  <span className="badge neutral">{opt.badge}</span>
                  <span className="primary-text">
                    <span className="truncate mono" style={{ display: "block" }} title={opt.primary}>
                      {opt.group === "contracts" ? shortId(opt.primary, 12, 8) : opt.primary}
                    </span>
                    {opt.secondary && <span className="secondary-text">{opt.secondary}</span>}
                  </span>
                  {i === active && <kbd aria-hidden>↵</kbd>}
                </li>
              </Fragment>
            );
          })}
        </ul>
      )}
    </div>
  );
}
