import { describe, expect, it } from "vitest";
import { ApiError } from "@/lib/api";
import { describePublicPageError } from "./PublicPage";

describe("PublicPage", () => {
  it.each([400, 404])("keeps unavailable public pages indistinguishable for HTTP %s", (status) => {
    expect(describePublicPageError(new ApiError("Page not found", status))).toBe(
      "This public page is unavailable or has been unpublished.",
    );
  });

  it("does not expose server failures through the public page surface", () => {
    expect(describePublicPageError(new Error("database exploded"))).toBe(
      "This public page could not be loaded. Please try again later.",
    );
  });
});
