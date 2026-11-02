import { describe, expect, it } from "vitest";
import { parseTemplateMarker, serializeTemplateMarker } from "./templateMarkers";

describe("portable template markers", () => {
  it("round trips UTF-8 guidance text as an inert comment", () => {
    const marker = { kind: "guidance" as const, id: "impact_1", text: "Describe users’ impact — and avoid `--`." };
    const source = serializeTemplateMarker(marker);

    expect(source).toMatch(/^<!-- starlane-template:v1:guidance:[A-Za-z0-9_-]+ -->$/);
    expect(source).not.toContain("-- and");
    expect(parseTemplateMarker(source)).toEqual({ ...marker, version: 1 });
  });

  it("round trips stable named-field bindings", () => {
    const marker = { kind: "field" as const, id: "service_slot_1", field_id: "field_service" };
    expect(parseTemplateMarker(serializeTemplateMarker(marker))).toEqual({ ...marker, version: 1 });
  });

  it("leaves unrelated comments alone and preserves malformed or future directives opaquely", () => {
    expect(parseTemplateMarker("<!-- ordinary authored comment -->")).toBeNull();
    const future = "<!-- starlane-template:v2:guidance:eyJpZCI6ImYiLCJ0ZXh0IjoieCJ9 -->";
    const malformed = "<!-- starlane-template:v1:guidance:not-base64! -->";
    expect(parseTemplateMarker(future)).toEqual({ kind: "opaque", raw: future });
    expect(parseTemplateMarker(malformed)).toEqual({ kind: "opaque", raw: malformed });
  });

  it("does not interpret malformed field payloads", () => {
    const malformed = "<!-- starlane-template:v1:field:eyJpZCI6Im9ubHktaWQifQ -->";
    expect(parseTemplateMarker(malformed)).toEqual({ kind: "opaque", raw: malformed });
  });

});
