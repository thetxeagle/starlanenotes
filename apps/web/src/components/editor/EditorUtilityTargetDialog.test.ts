import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { filterEditorUtilityTargets } from "./EditorUtilityTargetDialog";

describe("EditorUtilityTargetDialog", () => {
  it("filters target labels and secondary details together", () => {
    const targets = [
      { id: "space-1", label: "Engineering", detail: "engineering" },
      { id: "space-2", label: "Design", detail: "design" },
    ];
    expect(filterEditorUtilityTargets(targets, "engi")).toEqual([targets[0]]);
    expect(filterEditorUtilityTargets(targets, "DES")).toEqual([targets[1]]);
  });

  it("uses the shared dialog anatomy and connects its supporting description", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/editor/EditorUtilityTargetDialog.tsx"),
      "utf8",
    );

    expect(source).toContain("<DialogHeader>");
    expect(source).toContain("<DialogBody>");
    expect(source).toContain("<DialogFooter>");
    expect(source).toContain('describedBy="editor-utility-target-description"');
  });
});
