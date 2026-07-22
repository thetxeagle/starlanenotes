import type { Config } from "tailwindcss";
import typography from "@tailwindcss/typography";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        // Dark mode: Void Space
        bg: "var(--bg)",
        surface: "var(--surface)",
        "surface-raised": "var(--surface-raised)",
        border: "var(--border)",
        primary: "var(--primary)",
        accent: "var(--accent)",
        ai: "var(--ai)",
        text: "var(--text)",
        "text-muted": "var(--text-muted)",
        success: "var(--success)",
        danger: "var(--danger)",
        warning: "var(--warning)",
      },
      fontFamily: {
        sans: ["Inter", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "monospace"],
      },
      fontSize: {
        body: ["14px", "22px"],
        "body-lg": ["16px", "24px"],
      },
      spacing: {
        "sp-4": "4px",
        "sp-8": "8px",
        "sp-12": "12px",
        "sp-16": "16px",
        "sp-24": "24px",
        "sp-32": "32px",
      },
      borderRadius: {
        card: "8px",
        button: "6px",
        input: "4px",
      },
      boxShadow: {
        card: "0 2px 8px rgba(0,0,0,0.10)",
        modal: "0 8px 24px rgba(0,0,0,0.18)",
      },
      transitionDuration: {
        fast: "100ms",
        normal: "150ms",
      },
      width: {
        sidebar: "280px",
      },
    },
  },
  plugins: [typography],
} satisfies Config;
