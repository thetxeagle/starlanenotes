import { describe, expect, it } from "vitest";
import { formatDateMetadata } from "./dateMetadata";

describe("formatDateMetadata", () => {
  it("formats valid timestamps with their context label", () => {
    expect(formatDateMetadata("2026-07-09T12:00:00.000Z", "Updated", "Update date unavailable"))
      .toMatch(/^Updated Jul \d{1,2}, 2026$/);
  });

  it("uses explicit unavailable copy for missing and invalid timestamps", () => {
    expect(formatDateMetadata(undefined, "Updated", "Update date unavailable")).toBe(
      "Update date unavailable",
    );
    expect(formatDateMetadata("not-a-date", "Starred", "Star date unavailable")).toBe(
      "Star date unavailable",
    );
  });
});
