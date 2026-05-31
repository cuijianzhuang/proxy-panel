/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: ["class"],
  theme: {
    extend: {
      colors: {
        // Sakura palette 🌸 — the brand accent.
        sakura: {
          50:  "#fff5f8",
          100: "#ffe4ec",
          200: "#ffc9d9",
          300: "#ffa1bd",
          400: "#ff709a",
          500: "#ff4778",
          600: "#ef2761",
          700: "#c61a4d",
          800: "#9d1a40",
          900: "#7d1a37",
        },
      },
      fontFamily: {
        sans: [
          "Inter",
          "-apple-system",
          "BlinkMacSystemFont",
          "Segoe UI",
          "Roboto",
          "Helvetica",
          "Arial",
          "system-ui",
          "sans-serif",
        ],
        mono: ["ui-monospace", "SFMono-Regular", "Menlo", "Consolas", "monospace"],
      },
    },
  },
  plugins: [],
};
