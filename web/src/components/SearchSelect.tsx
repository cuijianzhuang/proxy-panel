/**
 * SearchSelect — a polished combobox that replaces native <select>
 *
 * Features:
 *   - Trigger button shows selected label
 *   - Click to open floating dropdown anchored to the trigger
 *   - Live-filter search input inside the dropdown
 *   - Keyboard: Escape → close, ArrowDown/Up → navigate, Enter → select
 *   - Click-outside to close
 *   - Matches the panel's sakura/light/dark token system
 */

import { useEffect, useRef, useState } from "react";

export type SelectOption = {
  value: string;
  label: string;
  /** Optional second line shown in muted color */
  sub?: string;
};

type Props = {
  options:       SelectOption[];
  value:         string;
  onChange:      (v: string) => void;
  placeholder?:  string;
  searchPlaceholder?: string;
  disabled?:     boolean;
  /** If true, the dropdown width matches the trigger width */
  fullWidth?:    boolean;
};

export function SearchSelect({
  options, value, onChange,
  placeholder = "— 请选择 —",
  searchPlaceholder = "搜索…",
  disabled = false,
  fullWidth = true,
}: Props) {
  const [open,    setOpen]    = useState(false);
  const [query,   setQuery]   = useState("");
  const [focused, setFocused] = useState(-1);

  const triggerRef = useRef<HTMLButtonElement>(null);
  const dropRef    = useRef<HTMLDivElement>(null);
  const searchRef  = useRef<HTMLInputElement>(null);

  const selected = options.find(o => o.value === value);

  // close on outside click
  useEffect(() => {
    if (!open) return;
    function handler(e: MouseEvent) {
      const t = e.target as Node;
      if (!triggerRef.current?.contains(t) && !dropRef.current?.contains(t)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  // focus search on open
  useEffect(() => {
    if (open) {
      setTimeout(() => searchRef.current?.focus(), 40);
      setFocused(-1);
      setQuery("");
    }
  }, [open]);

  const filtered = query.trim()
    ? options.filter(o =>
        o.label.toLowerCase().includes(query.toLowerCase()) ||
        (o.sub ?? "").toLowerCase().includes(query.toLowerCase())
      )
    : options;

  function select(v: string) {
    onChange(v);
    setOpen(false);
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (!open) {
      if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
        e.preventDefault();
        setOpen(true);
      }
      return;
    }
    if (e.key === "Escape") { setOpen(false); triggerRef.current?.focus(); return; }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setFocused(i => Math.min(i + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setFocused(i => Math.max(i - 1, 0));
    } else if (e.key === "Enter" && focused >= 0) {
      e.preventDefault();
      select(filtered[focused].value);
    }
  }

  return (
    <div style={{ position: "relative", width: "100%" }} onKeyDown={handleKeyDown}>
      {/* Trigger */}
      <button
        ref={triggerRef}
        type="button"
        disabled={disabled}
        onClick={() => !disabled && setOpen(o => !o)}
        className={[
          "input",
          "text-left flex items-center justify-between gap-2",
          disabled ? "opacity-50 cursor-not-allowed" : "cursor-pointer",
        ].join(" ")}
        style={{ userSelect: "none" }}
      >
        <span style={{
          flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
          color: selected ? "var(--fg)" : "var(--fg-muted)",
          opacity: selected ? 1 : 0.6,
        }}>
          {selected ? selected.label : placeholder}
        </span>
        {/* Chevron */}
        <svg
          width="14" height="14" viewBox="0 0 14 14"
          fill="none" stroke="currentColor" strokeWidth="1.8"
          strokeLinecap="round" strokeLinejoin="round"
          style={{
            flexShrink: 0,
            color: open ? "var(--accent)" : "var(--fg-muted)",
            transform: open ? "rotate(180deg)" : "none",
            transition: "transform 150ms ease, color 150ms ease",
          }}
        >
          <path d="M3 5l4 4 4-4"/>
        </svg>
      </button>

      {/* Dropdown */}
      {open && (
        <div
          ref={dropRef}
          style={{
            position: "absolute",
            top: "calc(100% + 6px)",
            left: 0,
            minWidth: fullWidth ? "100%" : 240,
            zIndex: 1000,
            background: "var(--bg-elev)",
            border: "1.5px solid var(--border)",
            borderRadius: 12,
            boxShadow: "0 8px 24px rgba(0,0,0,.12), 0 2px 8px rgba(0,0,0,.08)",
            overflow: "hidden",
            animation: "searchSelectIn 120ms ease",
          }}
        >
          {/* Search row */}
          <div style={{
            padding: "8px 10px",
            borderBottom: "1px solid var(--border)",
            display: "flex",
            alignItems: "center",
            gap: 6,
          }}>
            <svg width="13" height="13" viewBox="0 0 14 14" fill="none"
                 stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"
                 style={{ color: "var(--fg-muted)", flexShrink: 0 }}>
              <circle cx="6" cy="6" r="4"/>
              <path d="M9.5 9.5l2.5 2.5"/>
            </svg>
            <input
              ref={searchRef}
              type="text"
              value={query}
              onChange={e => { setQuery(e.target.value); setFocused(-1); }}
              placeholder={searchPlaceholder}
              style={{
                border: "none",
                background: "transparent",
                outline: "none",
                color: "var(--fg)",
                fontSize: 13,
                flex: 1,
                minWidth: 0,
              }}
            />
            {query && (
              <button
                type="button"
                onClick={() => setQuery("")}
                style={{
                  border: "none", background: "none", cursor: "pointer",
                  color: "var(--fg-muted)", lineHeight: 1, padding: 2,
                }}
              >✕</button>
            )}
          </div>

          {/* Options list */}
          <div style={{ maxHeight: 280, overflowY: "auto" }}>
            {filtered.length === 0 ? (
              <div style={{
                padding: "16px 14px", textAlign: "center",
                fontSize: 13, color: "var(--fg-muted)",
              }}>没有匹配的选项</div>
            ) : (
              filtered.map((opt, idx) => {
                const isSelected = opt.value === value;
                const isFocused  = idx === focused;
                return (
                  <div
                    key={opt.value}
                    role="option"
                    aria-selected={isSelected}
                    onMouseEnter={() => setFocused(idx)}
                    onClick={() => select(opt.value)}
                    style={{
                      padding: "8px 14px",
                      cursor: "pointer",
                      fontSize: 13,
                      background: isFocused
                        ? "var(--accent-soft)"
                        : isSelected
                          ? "color-mix(in srgb, var(--accent-soft) 60%, transparent)"
                          : "transparent",
                      borderLeft: isSelected ? "3px solid var(--accent)" : "3px solid transparent",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      gap: 8,
                      transition: "background 80ms ease",
                    }}
                  >
                    <span style={{ flex: 1, minWidth: 0 }}>
                      <span style={{
                        display: "block",
                        fontWeight: isSelected ? 600 : 400,
                        color: isSelected ? "var(--accent)" : "var(--fg)",
                        overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                      }}>{opt.label}</span>
                      {opt.sub && (
                        <span style={{
                          display: "block", fontSize: 11,
                          color: "var(--fg-muted)", marginTop: 1,
                        }}>{opt.sub}</span>
                      )}
                    </span>
                    {isSelected && (
                      <svg width="12" height="12" viewBox="0 0 12 12" fill="none"
                           stroke="var(--accent)" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
                        <path d="M2 6l3 3 5-5"/>
                      </svg>
                    )}
                  </div>
                );
              })
            )}
          </div>
        </div>
      )}

      {/* Enter animation */}
      <style>{`
        @keyframes searchSelectIn {
          from { opacity: 0; transform: translateY(-4px) scale(0.98); }
          to   { opacity: 1; transform: translateY(0) scale(1); }
        }
      `}</style>
    </div>
  );
}
