import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AvatarCropDialog, getAvatarDragDelta } from "./AvatarCropDialog";

describe("AvatarCropDialog", () => {
  it("preserves crop movement when the displayed canvas is scaled down", () => {
    expect(getAvatarDragDelta(28, 280)).toBe(28);
    expect(getAvatarDragDelta(14, 140)).toBe(28);
    expect(getAvatarDragDelta(-35, 140)).toBe(-70);
  });
  it("uses shared dialog anatomy while preserving its canvas and zoom controls", () => {
    const html = renderToStaticMarkup(
      <AvatarCropDialog
        source="data:image/png;base64,test"
        onCancel={() => undefined}
        onConfirm={() => undefined}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="avatar-crop-title"');
    expect(html).toContain('aria-describedby="avatar-crop-description"');
    expect(html).toContain('aria-label="Close profile picture editor"');
    expect(html).toContain("<canvas");
    expect(html).toContain('aria-label="Avatar zoom"');
    expect(html).toContain("Use picture");
    expect(html).toContain("border-b");
    expect(html).toContain("border-t");
  });
});
