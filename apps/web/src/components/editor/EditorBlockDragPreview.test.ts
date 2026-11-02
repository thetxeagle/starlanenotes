import { describe, expect, it } from "vitest";
import { getBlockPreviewGeometry } from "./EditorBlockDragPreview";

describe("rendered block preview geometry", () => {
  it("keeps a small block at its actual size and follows the pointer", () => {
    expect(getBlockPreviewGeometry(300, 80, 1280, 800, { x: 100, y: 200 })).toEqual({
      scale: 1,
      width: 300,
      height: 80,
      left: 116,
      top: 216,
    });
  });
  it("scales columns proportionally rather than reconstructing equal text boxes", () => {
    expect(getBlockPreviewGeometry(1200, 200, 1280, 800, { x: 100, y: 200 })).toEqual({
      scale: 0.6,
      width: 720,
      height: 120,
      left: 116,
      top: 216,
    });
  });
  it("contains tall and wide blocks inside the viewport", () => {
    const result = getBlockPreviewGeometry(1200, 2400, 900, 600, { x: 890, y: 590 });
    expect(result.left).toBeGreaterThanOrEqual(8);
    expect(result.top).toBeGreaterThanOrEqual(8);
    expect(result.left + result.width).toBeLessThanOrEqual(892);
    expect(result.top + result.height).toBeLessThanOrEqual(592);
    expect(result.width / result.height).toBeCloseTo(0.5);
  });
  it("clamps pointers beyond the top/left edges and avoids division by zero", () => {
    const result = getBlockPreviewGeometry(0, 0, 800, 600, { x: -100, y: -100 });
    expect(result).toEqual({ scale: 1, width: 0, height: 0, left: 8, top: 8 });
  });
});
