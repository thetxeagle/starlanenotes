import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("ReviewedDraftTransferPanel", () => {
  it("restores focus to the review trigger after an asynchronous dialog launch", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/admin/ReviewedDraftTransferPanel.tsx"),
      "utf8",
    );

    expect(source).toContain("const reviewTriggerRef = useRef<HTMLButtonElement>(null)");
    expect(source).toMatch(/<Button ref=\{reviewTriggerRef\}[\s\S]*?>\s*<ShieldCheck size=\{14\} \/> Review Transfer/);
    expect(source).toMatch(/<DialogShell[\s\S]*?triggerRef=\{reviewTriggerRef\}/);
  });
});
