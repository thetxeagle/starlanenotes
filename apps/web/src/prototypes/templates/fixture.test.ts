import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { FixtureOutput } from "./TemplateExperiencePrototype";
import { describe, expect, it } from "vitest";
import { buildIncidentOutput, EMPTY_RESPONSES, type IncidentFieldValues } from "./fixture";

const fields: IncidentFieldValues = {
  service: "Payments API",
  severity: "SEV-2",
  date: "2026-10-07",
  owner: "Maya Chen",
};

describe("template experience fixture model", () => {
  it("keeps authored prose that happens to contain the word Guidance", () => {
    const output = buildIncidentOutput(fields, {
      ...EMPTY_RESPONSES,
      impact: "Guidance: this is a customer quote, not an instruction.",
    });
    expect(output.sections.find((section) => section.id === "impact")?.text).toBe(
      "Guidance: this is a customer quote, not an instruction.",
    );
    expect(
      output.sections.some((section) => section.text.includes("Include affected services")),
    ).toBe(false);
  });

  it("keeps bound values as plain data for safe text-node rendering", () => {
    const unsafe = { ...fields, service: "**Payments** #1 <img src=x onerror=alert(1)> &" };
    const output = buildIncidentOutput(unsafe, EMPTY_RESPONSES);
    expect(output.title).toContain("Incident — **Payments** #1");
    expect(output.properties.Service).toBe(unsafe.service);
    expect(output.summaryFields.service).toBe(unsafe.service);
  });

  it("preserves authored paragraphs and multiline responses", () => {
    const output = buildIncidentOutput(fields, {
      ...EMPTY_RESPONSES,
      summary: "First line\nsecond line\n\nNew paragraph",
    });
    expect(output.sections.find((section) => section.id === "summary")?.text).toBe(
      "First line\nsecond line\n\nNew paragraph",
    );
  });
});

describe("fixture output safe rendering", () => {
  it("renders field markup as literal text while preserving ordinary punctuation and authored guidance quotes", () => {
    const html = renderToStaticMarkup(
      createElement(FixtureOutput, {
        fields: { ...fields, service: "<img src=x onerror=alert(1)> **literal**" },
        responses: {
          ...EMPTY_RESPONSES,
          summary: "Guidance: a legitimate quote.\nSecond paragraph",
        },
        title: "Custom incident title",
      }),
    );
    expect(html).not.toContain("<img");
    expect(html).toContain("&lt;img");
    expect(html).not.toContain("<strong>literal");
    expect(html).toContain("SEV-2");
    expect(html).toContain("2026-10-07");
    expect(html).toContain("Guidance: a legitimate quote.");
    expect(html).not.toContain("Describe the user impact");
    expect(html).toContain("Custom incident title");
  });
});
