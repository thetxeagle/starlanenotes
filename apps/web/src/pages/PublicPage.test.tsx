import { describe, expect, it } from "vitest";
import { ApiError } from "@/lib/api";
import { describePublicPageError } from "./PublicPage";

describe("PublicPage", () => {
  it("keeps unpublished pages indistinguishable from unavailable public pages", () => {
    expect(describePublicPageError(new ApiError("Page not found", 400))).toBe(
      "This public page is unavailable or has been unpublished.",
    );
  });

  it("does not expose server failures through the public page surface", () => {
    expect(describePublicPageError(new Error("database exploded"))).toBe(
      "This public page could not be loaded. Please try again later.",
    );
  });
});
