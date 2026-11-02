import { describe, expect, it } from "vitest";
import { normalizeCodeTabs, parseCodeTabsSource, serializeCodeTabs } from "./CodeTabsBlock";

describe("code tabs portable format", () => {
  it("round trips up to the first ten named language tabs", () => {
    const source = [
      "title: Deploy",
      "tab: TypeScript | ts",
      "const ready = true;",
      "---",
      "tab: CSS | css",
      ".ready { color: green; }",
    ].join("\n");
    const parsed = parseCodeTabsSource(source);
    expect(parsed).toEqual({
      title: "Deploy",
      tabs: [
        { label: "TypeScript", language: "ts", code: "const ready = true;" },
        { label: "CSS", language: "css", code: ".ready { color: green; }" },
      ],
    });
    expect(serializeCodeTabs(parsed.title, parsed.tabs)).toContain("tab: CSS | css");
    expect(normalizeCodeTabs(Array.from({ length: 12 }, (_, index) => ({ label: String(index) })))).toHaveLength(10);
  });

  it("always provides one editable empty tab for a new block", () => {
    expect(normalizeCodeTabs([])).toEqual([{ label: "Tab 1", language: "", code: "" }]);
  });

  it("preserves an intentionally empty optional tab name", () => {
    expect(normalizeCodeTabs([{ label: "", language: "bash", code: "echo ok" }])).toEqual([
      { label: "", language: "bash", code: "echo ok" },
    ]);
    expect(parseCodeTabsSource("tab:  | bash\necho ok").tabs).toEqual([
      { label: "", language: "bash", code: "echo ok" },
    ]);
  });
});
