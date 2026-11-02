import { describe, expect, it } from "vitest";
import { Fragment, Schema } from "prosemirror-model";
import { EditorState } from "prosemirror-state";
import { history, undo } from "prosemirror-history";
import type { TemplateRecord } from "@/lib/dataClient";
import { serializeTemplateMarker } from "@/lib/templateMarkers";
import { createTemplateSectionInsertionTransaction, getTemplateSectionFieldIds, getTemplateSectionFields, resolveTemplateSection } from "./templateSection";

const template = (content: string): TemplateRecord => ({
  id: "template-1",
  space_id: "space-1",
  name: "Incident summary",
  content,
  properties: {
    __starlane_template: {
      schema_version: 1,
      fields: [
        { id: "summary", label: "Summary", type: "multiline", required: true, bindings: [{ target: "property", property_id: "summary-schema" }] },
        { id: "service", label: "Service", type: "text", bindings: [{ target: "property", property_id: "service-schema" }] },
        { id: "unused", label: "Unused", type: "text" },
      ],
      title: [{ kind: "text", value: "Incident — " }, { kind: "field", field_id: "service" }],
    },
  },
  tags: [],
  created_at: "2026-10-07T00:00:00.000Z",
  updated_at: "2026-10-07T00:00:00.000Z",
});

describe("template section resolution", () => {
  it("finds only active body fields and skips code examples", () => {
    const content = [
      serializeTemplateMarker({ kind: "field", id: "marker-a", field_id: "summary" }),
      "```md",
      serializeTemplateMarker({ kind: "field", id: "marker-code", field_id: "service" }),
      "```",
      `    ${serializeTemplateMarker({ kind: "field", id: "marker-indented", field_id: "unused" })}`,
    ].join("\n");

    expect([...getTemplateSectionFieldIds(content)]).toEqual(["summary"]);
    expect(getTemplateSectionFields(template(content)).map((field) => field.id)).toEqual(["summary"]);
  });

  it("drops title and property bindings while resolving content and preserving guidance", () => {
    const guidance = serializeTemplateMarker({ kind: "guidance", id: "guidance-a", text: "Describe impact." });
    const fieldMarker = serializeTemplateMarker({ kind: "field", id: "marker-a", field_id: "service" });
    const source = template(`## {{title}}\n\n${fieldMarker}\n\n${guidance}\n\n{{unfamiliar_token}}`);
    const result = resolveTemplateSection({
      template: source,
      context: { title: "Current page", space: "Ops" },
      fieldValues: { service: "API" },
    });

    expect(result.title).toBeUndefined();
    expect(result.property_values).toEqual([]);
    expect(result.fields.map((field) => field.id)).toEqual(["service"]);
    expect(result.content).toContain("## Current page");
    expect(result.content).toContain("API");
    expect(result.content).toContain(guidance);
    expect(result.content).toContain("{{unfamiliar_token}}");
  });

  it("keeps legacy literal tokens and has no fields for a field-free section", () => {
    const result = resolveTemplateSection({
      template: template("{{date}}\n\n{{custom.legacy}}"),
      context: { date: new Date("2026-10-07T12:00:00.000Z") },
    });
    expect(result.fields).toEqual([]);
    expect(result.content).toMatch(/October 7, 2026/);
    expect(result.content).toContain("{{custom.legacy}}");
  });

  it("carries schema dropdown options into insertion after removing the property binding", () => {
    const source = template(serializeTemplateMarker({ kind: "field", id: "severity-marker", field_id: "severity" }));
    source.properties = {
      __starlane_template: {
        schema_version: 1,
        fields: [{
          id: "severity",
          label: "Severity",
          type: "dropdown",
          options: ["Old option"],
          bindings: [{ target: "property", property_id: "severity-schema" }],
        }],
      },
    };
    const fields = getTemplateSectionFields(source, [{
      id: "severity-schema",
      name: "Severity",
      property_type: "select",
      options: ["Low", "High"],
      sort_order: 0,
    }]);

    expect(fields[0].bindings).toBeUndefined();
    expect(fields[0].options).toEqual(["Low", "High"]);
  });
});

describe("template section insertion transaction", () => {
  const schema = new Schema({
    nodes: {
      doc: { content: "block+" },
      paragraph: { content: "text*", group: "block" },
      text: { group: "inline" },
    },
  });

  it("replaces only the slash query and gives insertion its own undo step", () => {
    let state = EditorState.create({
      schema,
      doc: schema.node("doc", null, schema.node("paragraph", null, schema.text("start /template end"))),
      plugins: [history()],
    });

    state = state.apply(state.tr.insertText(" typed", 6));
    const queryStart = state.doc.textContent.indexOf("/template");
    const from = 1 + queryStart;
    const to = from + "/template".length;
    const transaction = createTemplateSectionInsertionTransaction(
      state,
      { from, to },
      Fragment.from(schema.text("Reusable section")),
    );
    expect(transaction).not.toBeNull();
    state = state.apply(transaction!);
    expect(state.doc.textContent).toBe("start typed Reusable section end");

    let didUndo = false;
    undo(state, (undoTransaction) => {
      state = state.apply(undoTransaction);
      didUndo = true;
    });
    expect(didUndo).toBe(true);
    expect(state.doc.textContent).toBe("start typed /template end");
  });

  it("rejects invalid captured ranges without creating a transaction", () => {
    const state = EditorState.create({
      schema,
      doc: schema.node("doc", null, schema.node("paragraph", null, schema.text("short"))),
    });

    expect(createTemplateSectionInsertionTransaction(state, { from: 1, to: 999 }, Fragment.empty)).toBeNull();
  });
});
