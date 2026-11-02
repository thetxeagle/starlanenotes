import { describe, expect, it } from "vitest";
import { clearEditorInstanceIfCurrent } from "./editorInstanceLifecycle";

describe("clearEditorInstanceIfCurrent", () => {
  it("does not clear a newer editor when an older instance is destroyed", () => {
    const olderEditor = { id: "older" };
    const currentEditor = { id: "current" };
    const reference = { current: currentEditor };

    clearEditorInstanceIfCurrent(reference, olderEditor);

    expect(reference.current).toBe(currentEditor);
  });

  it("clears the reference when the active editor is destroyed", () => {
    const currentEditor = { id: "current" };
    const reference: { current: typeof currentEditor | null } = { current: currentEditor };

    clearEditorInstanceIfCurrent(reference, currentEditor);

    expect(reference.current).toBeNull();
  });
});
