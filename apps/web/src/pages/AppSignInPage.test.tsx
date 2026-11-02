import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AppSignInSurface } from "./AppSignInPage";

describe("AppSignInPage", () => {
  it("uses the shared authentication surface for a successful app return", () => {
    const html = renderToStaticMarkup(
      <AppSignInSurface
        title="App sign-in approved"
        description="Return to Starlane Notes to finish."
        returnUrl="starlane://auth/handoff?handoff_id=handoff-1"
      />,
    );

    expect(html).toContain("Starlane Notes");
    expect(html).toContain("App sign-in approved");
    expect(html).toContain("Return to Starlane Notes");
    expect(html).toContain("rounded-button");
    expect(html).toContain("bg-primary");
    expect(html).not.toMatch(/rounded-full|backdrop-blur/);
  });
});
