/*
 * Theme switcher. The four modes:
 *   sakura  — soft pink palette (brand default)
 *   light   — neutral light
 *   dark    — neutral dark
 *   system  — follow OS preference (live updates via `matchMedia`)
 *
 * Mode is persisted to localStorage and applied to <html>'s data-theme
 * attribute. The inline bootstrap in index.html does the initial paint;
 * `useTheme` keeps things in sync after hydration.
 */
import { useEffect, useState } from "react";

export type ThemeMode = "sakura" | "light" | "dark" | "system";
const STORAGE_KEY = "proxy-panel-theme-mode";

function readMode(): ThemeMode {
  if (typeof window === "undefined") return "sakura";
  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (stored === "sakura" || stored === "light" || stored === "dark" || stored === "system") {
    return stored;
  }
  return "sakura";
}

function resolveTheme(mode: ThemeMode): "sakura" | "light" | "dark" {
  if (mode === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return mode;
}

function apply(mode: ThemeMode) {
  const theme = resolveTheme(mode);
  const root = document.documentElement;
  root.dataset.theme = theme;
  root.dataset.themeMode = mode;
  root.classList.toggle("dark", theme === "dark");
  root.style.colorScheme = theme === "dark" ? "dark" : "light";
}

export function useTheme(): [ThemeMode, (next: ThemeMode) => void] {
  const [mode, setMode] = useState<ThemeMode>(readMode);

  useEffect(() => {
    apply(mode);
    window.localStorage.setItem(STORAGE_KEY, mode);
  }, [mode]);

  // Keep `system` honest as OS theme changes.
  useEffect(() => {
    if (mode !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => apply("system");
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [mode]);

  return [mode, setMode];
}
