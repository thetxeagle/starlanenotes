import { describe, expect, it } from "vitest";
import {
  addField,
  createBlankTemplate,
  createPageSnapshot,
  initialFieldValues,
  renameFieldBindings,
  validateFields,
  validateDefinition,
  isValidDate,
  removeField,
  type AuthoringTemplate,
} from "./authoring";

function sampleTemplate(): AuthoringTemplate {
  return {
    id: "sample",
    name: "Sample",
    description: "",
    fields: [
      {
        id: "owner-id",
        label: "Owner",
        kind: "text",
        required: true,
        options: [],
        defaultValue: "",
      },
    ],
    content: "## Owner\n{{Owner}} wrote this.",
    guidance: [],
  };
}

describe("template authoring model", () => {
  it("starts truly blank and creates unique blank drafts when given distinct identities", () => {
    expect(createBlankTemplate("draft-a")).toMatchObject({
      id: "draft-a",
      content: "",
      fields: [],
      guidance: [],
    });
    expect(createBlankTemplate("draft-b").id).not.toBe(createBlankTemplate("draft-a").id);
  });

  it("adds configurable fields and validates only required answers", () => {
    const withField = addField(createBlankTemplate(), "field-id");
    const required = {
      ...withField,
      fields: withField.fields.map((item) => ({ ...item, required: true })),
    };
    expect(validateFields(required, initialFieldValues(required))).toEqual(["New field 1"]);
    expect(validateFields(required, { "field-id": "  Ada " })).toEqual([]);
  });

  it("keeps bindings when a field label changes", () => {
    const renamed = renameFieldBindings(sampleTemplate(), "owner-id", "Author");
    expect(renamed.content).toContain("{{Author}}");
    expect(renamed.fields[0].id).toBe("owner-id");
  });

  it("creates an independent page snapshot with binding data for text-node replacement", () => {
    const template = sampleTemplate();
    const page = createPageSnapshot(
      template,
      { "owner-id": "**not markdown**" },
      "My page",
      "page-1",
    );
    expect(page).toMatchObject({
      id: "page-1",
      title: "My page",
      content: template.content,
      bindingsResolved: false,
    });
    expect(page.bindings).toEqual([{ marker: "{{Owner}}", value: "**not markdown**" }]);
    expect(page.properties).toEqual({ Owner: "**not markdown**" });
    expect(template.content).toContain("{{Owner}}");
  });

  it("rejects duplicate labels, invalid dates and invalid dropdown suggestions", () => {
    expect(isValidDate("2026-02-31")).toBe(false);
    expect(isValidDate("2024-02-29")).toBe(true);
    const template = sampleTemplate();
    template.fields.push({ ...template.fields[0], id: "duplicate", label: "owner" });
    expect(validateDefinition(template)).toContain("Field labels must be unique");
    template.fields = [
      { ...template.fields[0], kind: "dropdown", options: ["Ready"], defaultValue: "Missing" },
    ];
    expect(validateDefinition(template)).toContain("Owner: suggested answer must match an option");
  });

  it("removes only the selected field's bindings and preserves unrelated prose", () => {
    const template = { ...sampleTemplate(), content: "Guidance: a quote. {{Owner}} {{Unknown}}" };
    const removed = removeField(template, "owner-id");
    expect(removed.fields).toEqual([]);
    expect(removed.content).toBe("Guidance: a quote.  {{Unknown}}");
    expect(template.content).toContain("{{Owner}}");
  });
  it("retains bindings when a label is cleared and replaced while editing", () => {
    const cleared = renameFieldBindings(sampleTemplate(), "owner-id", "");
    const renamed = renameFieldBindings(cleared, "owner-id", "Author");
    expect(renamed.content).toContain("{{Author}}");
    expect(renamed.fields[0].id).toBe("owner-id");
  });
});
