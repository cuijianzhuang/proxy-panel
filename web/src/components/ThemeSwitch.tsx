import { useTheme, ThemeMode } from "../lib/theme";

const ITEMS: { mode: ThemeMode; label: string; emoji: string }[] = [
  { mode: "sakura", label: "Sakura", emoji: "🌸" },
  { mode: "light",  label: "Light",  emoji: "☀️" },
  { mode: "dark",   label: "Dark",   emoji: "🌙" },
  { mode: "system", label: "System", emoji: "🖥️" },
];

export function ThemeSwitch() {
  const [mode, setMode] = useTheme();
  return (
    <div className="flex gap-1 w-full p-1 rounded-md" style={{ background: "var(--bg-elev)" }}>
      {ITEMS.map((it) => {
        const active = mode === it.mode;
        return (
          <button
            key={it.mode}
            onClick={() => setMode(it.mode)}
            title={it.label}
            className="flex-1 py-1.5 rounded text-sm transition-colors"
            style={{
              background: active ? "var(--accent)" : "transparent",
              color: active ? "var(--accent-fg)" : "var(--fg)",
            }}
          >
            <span className="text-base">{it.emoji}</span>
          </button>
        );
      })}
    </div>
  );
}
