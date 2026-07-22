# Themes

Starlane Notes ships with two built-in themes. The active theme is controlled by the `.dark` CSS class on the root element.

## Void Space (Dark Theme)

The default dark theme. Applied when the `.dark` class is present.

| Token | Value | Usage |
|---|---|---|
| `--bg` | `#0d1117` | Page background |
| `--surface` | `#161b22` | Card and panel backgrounds |
| `--surface-raised` | `#1f2937` | Elevated surfaces, code blocks, table headers |
| `--border` | `#30363d` | Borders and dividers |
| `--primary` | `#58a6ff` | Interactive elements (links, buttons, checkboxes, focus rings) |
| `--accent` | `#ea580c` | Highlights, notifications, badges, text marks |
| `--ai` | `#7c3aed` | AI-related UI elements only |
| `--text` | `#c9d1d9` | Primary text |
| `--text-muted` | `#8b949e` | Secondary text, placeholders, blockquotes |

## Cloud Canvas (Light Theme)

Applied when the `.dark` class is absent (default `:root` styles).

| Token | Value | Usage |
|---|---|---|
| `--bg` | `#fafafa` | Page background |
| `--surface` | `#ffffff` | Card and panel backgrounds |
| `--surface-raised` | `#f3f4f6` | Elevated surfaces, code blocks, table headers |
| `--border` | `#e5e7eb` | Borders and dividers |
| `--primary` | `#2563eb` | Interactive elements |
| `--accent` | `#ea580c` | Highlights, notifications, badges |
| `--ai` | `#7c3aed` | AI-related UI elements only |
| `--text` | `#0f172a` | Primary text |
| `--text-muted` | `#6b7280` | Secondary text, placeholders |

## Color Semantics

Colors have strict semantic assignments across both themes:

| Color | Semantic | Examples |
|---|---|---|
| Blue (`--primary`) | Interactive elements | Links, buttons, active states, focus rings, checkbox accents |
| Orange (`--accent`) | Attention and emphasis | Highlighted text, notification badges, warning indicators |
| Purple (`--ai`) | AI features exclusively | AI chat sidebar, AI-generated content indicators, AI action buttons |

Purple is never used for non-AI UI elements. This ensures users can always distinguish AI-generated content from human-authored content.

## Typography

| Context | Font | Source |
|---|---|---|
| UI text | Inter | Self-hosted |
| Code / monospace | JetBrains Mono | Self-hosted |

Base font size is 14px with 1.5 line height. Font smoothing is enabled on both macOS (`-webkit-font-smoothing: antialiased`) and Firefox (`-moz-osx-font-smoothing: grayscale`).

The web app bundles these fonts through local `@fontsource` WOFF2 assets. Do not add Google Fonts, Adobe Fonts, or any other runtime font CDN.

## Using Theme Tokens

All theme tokens are CSS custom properties on `:root` (light) and `.dark` (dark). Use them in your CSS:

```css
.my-component {
  background-color: var(--surface);
  color: var(--text);
  border: 1px solid var(--border);
}

.my-button {
  background-color: var(--primary);
}

.ai-indicator {
  color: var(--ai);
}
```

In Tailwind, these tokens are mapped to utility classes via the Tailwind config (e.g., `bg-bg`, `text-primary`, `border-border`).

## Editor Styles

The TipTap editor uses theme tokens for all styling:

- Links use `--primary`
- Highlighted text (`<mark>`) uses `--accent` at 20% opacity
- Code blocks and inline code use `--surface-raised` background
- Blockquote borders use `--border` with `--text-muted` text
- Focus selection outlines use `--primary`
- Task list checkbox accents use `--primary`
