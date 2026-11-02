import { describe, expect, it } from "vitest";
import { serializeTemplateMarker } from "./templateMarkers";
import { stripTemplateGuidance } from "./templatePublication";
describe("published template guidance", () => {
  it("retains opaque CSS comments and bytes while removing guidance outside", () => {
    const guidance = serializeTemplateMarker({ kind: "guidance", id: "css-original", text: "Retain this original" });
    const block = `<!-- Starlane:custom-css -->\r\n${guidance}\r\n/* \0 */ @import 'https://invalid.example/original';\r\n${"x".repeat(20_100)}\r\n<!-- /starlane:custom-css -->`;
    expect(stripTemplateGuidance(`Before\n${guidance}\n${block}\n${guidance}\nAfter\n`)).toBe(`Before\n${block}\nAfter\n`);
  });
  it.each([
    "<!-- starlane:custom-css", "<!-- starlane:custom-c", "<!-- /starlane:custom-css -->",
    "<!-- /starlane:custom-css -->\r\n<!-- starlane:custom-css -->",
    "<!-- starlane:custom-css --><!-- starlane:custom-css --><!-- /starlane:custom-css -->",
    "<!-- starlane:custom-css --><!-- /starlane:custom-css --><!-- starlane:custom-css --><!-- /starlane:custom-css -->",
  ])("retains whole source for uncertain CSS boundaries %s", (marker) => {
    const guidance = serializeTemplateMarker({ kind: "guidance", id: "original", text: "Keep" });
    const source = ` \r\n${guidance}\r\n${marker}\r\n\0original \r\n`;
    expect(stripTemplateGuidance(source)).toBe(source);
  });
  it("omits live guidance while preserving literal examples and unknown directives", () => {
    const marker = serializeTemplateMarker({
      kind: "guidance",
      id: "summary",
      text: "Describe user impact.",
    });
    const field = serializeTemplateMarker({ kind: "field", id: "body", field_id: "service" });
    const source = `Keep legitimate prose.\n${marker}\n\`\`\`markdown\n${marker}\n\`\`\`\n    ${marker}\n\t${marker}\n${field}\n<!-- starlane-template:v2:guidance:future -->\n`;
    const published = stripTemplateGuidance(source);
    expect(published.startsWith("Keep legitimate prose.\n```markdown")).toBe(true);
    expect(published.split(marker)).toHaveLength(4);
    expect(published).toContain(field);
    expect(published).toContain("v2:guidance");
    expect(source).toContain(`prose.\n${marker}`);
  });
});
