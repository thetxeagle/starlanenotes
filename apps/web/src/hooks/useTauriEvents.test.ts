import { describe, expect, it } from "vitest";
import { getTauriDeepLinkRoutePath } from "./useTauriEvents";

describe("getTauriDeepLinkRoutePath", () => {
  it("builds encoded document and space routes from native deep links", () => {
    expect(
      getTauriDeepLinkRoutePath({
        action: "open_note",
        space_id: "space/one",
        doc_id: "doc/two",
      }),
    ).toBe("/spaces/space%2Fone/docs/doc%2Ftwo");

    expect(
      getTauriDeepLinkRoutePath({
        action: "open_space",
        space_id: "space one",
      }),
    ).toBe("/spaces/space%20one/docs");
  });

  it("ignores incomplete or non-navigation native deep links", () => {
    expect(getTauriDeepLinkRoutePath({ action: "open_note", space_id: "space-1" })).toBeNull();
    expect(getTauriDeepLinkRoutePath({ action: "search", query: "roadmap" })).toBeNull();
  });
});
