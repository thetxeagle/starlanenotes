import { describe, expect, it } from "vitest";
import { serializeTemplateMarker } from "@/lib/templateMarkers";
import { TEMPLATE_EXPERIENCE_KEY, getTemplateExperience, resolveTemplateExperience } from "./templateExperience";

const textField = {
  id: "field_service",
  label: "Service",
  type: "text" as const,
  required: true,
};

describe("template experience resolver", () => {
  it("resolves a field marker as a literal editor value and retains guidance structurally", () => {
    const marker = serializeTemplateMarker({ kind: "field", id: "body_service", field_id: textField.id });
    const guidance = serializeTemplateMarker({ kind: "guidance", id: "guidance_summary", text: "Describe the impact." });
    const hostile = `**literal** <img src=x onerror=alert(1)> {{date}}`;
    const result = resolveTemplateExperience({
      content: `Service:\n${marker}\n\n${guidance}\n`,
      config: { schema_version: 1, fields: [textField] },
      fieldValues: { [textField.id]: hostile },
      context: { date: new Date("2026-10-07T12:00:00.000Z") },
    });

    expect(result.content).toContain(`<span data-starlane-template-value="v1" style="white-space:pre-wrap">**literal** &lt;img src=x onerror=alert(1)&gt; {{date}}</span>`);
    expect(result.content).toContain(guidance);
    expect(result.content).not.toContain("Wednesday, October");
    expect(result.guidance).toEqual([{ id: "guidance_summary", text: "Describe the impact." }]);
  });

  it("does not interpret template markers inside fenced or indented code", () => {
    const marker = serializeTemplateMarker({ kind: "field", id: "body_service", field_id: textField.id });
    const source = `\`\`\`md\n${marker}\n\`\`\`\n\n    ${marker}\n`;
    const result = resolveTemplateExperience({ content: source, config: { schema_version: 1, fields: [textField] }, fieldValues: { [textField.id]: "API" } });
    expect(result.content).toBe(source);
  });

  it("uses an explicitly empty value instead of restoring the field default", () => {
    const result = resolveTemplateExperience({
      config: { schema_version: 1, fields: [{ ...textField, required: false, default_value: "Default" }] },
      fieldValues: { [textField.id]: "" },
    });
    expect(result.unresolvedFields).toEqual([]);
  });

  it("validates dates, dropdown schema options, and duplicate property bindings", () => {
    const result = resolveTemplateExperience({
      config: {
        schema_version: 1,
        fields: [
          { id: "date", label: "Date", type: "date", required: true },
          { id: "severity", label: "Severity", type: "dropdown", options: ["Low"], bindings: [{ target: "property", property_id: "schema-severity" }] },
          { id: "impact", label: "Impact", type: "dropdown", options: ["Low", "High"], bindings: [{ target: "property", property_id: "schema-severity" }] },
        ],
      },
      fieldValues: { date: "2026-02-31", severity: "Low", impact: "High" },
      schemas: [{ id: "schema-severity", name: "Severity", property_type: "select", options: ["Low", "High"], sort_order: 0 }],
    });
    expect(result.unresolvedFields).toEqual(expect.arrayContaining([
      expect.objectContaining({ id: "date", reason: "Enter a valid date." }),
      expect.objectContaining({ id: "impact", reason: expect.stringContaining("already bound") }),
    ]));
    expect(result.property_values).toEqual([{ property_id: "schema-severity", value: "Low" }]);
  });

  it("blocks oversized Unicode property answers before a creation request is frozen", () => {
    const result = resolveTemplateExperience({
      config: { schema_version: 1, fields: [{ ...textField, bindings: [{ target: "property", property_id: "service" }] }] },
      fieldValues: { [textField.id]: "🦅".repeat(25_001) },
      schemas: [{ id: "service", name: "Service", property_type: "text", options: null, sort_order: 0 }],
    });
    expect(result.property_values).toEqual([]);
    expect(result.unresolvedFields).toEqual([expect.objectContaining({ id: textField.id, reason: expect.stringContaining("100,000 bytes") })]);
  });

  it("reads only the v1 envelope and keeps legacy templates field-free", () => {
    expect(getTemplateExperience({ [TEMPLATE_EXPERIENCE_KEY]: { schema_version: 1, fields: [textField] } }).fields).toEqual([textField]);
    expect(getTemplateExperience({ content: "{{date}}" }).fields).toEqual([]);
    expect(getTemplateExperience({ [TEMPLATE_EXPERIENCE_KEY]: { schema_version: 2, fields: [textField] } }).fields).toEqual([]);
  });
});
