import { describe, expect, it } from "vitest";
import { CUSTOM_CSS_START_MARKER, CUSTOM_CSS_END_MARKER, readCustomCss, replaceCustomCss, sanitizeCustomCss } from "./documentContent";

describe("retained CSS rendering boundary", () => {
  it.each(["@import 'https://example.invalid/style.css';", ".notice { background: url(https://example.invalid/image); }"])("rejects oversized CSS containing external loading syntax: %s", (blocked) => {
    for (const css of [blocked + ".padding { color: blue; }".repeat(1000), ".notice { color: blue; }".repeat(1000) + blocked]) {
      expect(sanitizeCustomCss(css)).toBe("");
      const source = `Body\n${CUSTOM_CSS_START_MARKER}\n${css}\n${CUSTOM_CSS_END_MARKER}`;
      expect(readCustomCss(source)).toBe("");
      expect(replaceCustomCss("Body", css)).toBe("Body");
    }
  });

  it("retains approved CSS within the existing size limit and rejects short external references", () => {
    const css = ".notice { color: var(--primary); }".repeat(1000);
    expect(sanitizeCustomCss(css)).toBe(css.slice(0, 20_000));
    expect(sanitizeCustomCss(" \0.notice { color: blue; } ")).toBe(".notice { color: blue; }");
    expect(sanitizeCustomCss("@import 'https://example.invalid/style.css';")).toBe("");
  });
});
