import { describe, expect, it } from "vitest";
import { readCustomCss } from "./documentContent";
import {
  inspectPageCssSource,
  rebuildPageCssSource,
  retainedPageCssTextLength,
  validatePageCssProvenance,
  type PageCssProvenance,
} from "./pageCssProvenance";
const OPEN = "<!-- starlane:custom-css -->";
const CLOSE = "<!-- /starlane:custom-css -->";
const block = (css: string) => `${OPEN}${css}${CLOSE}`;

describe("page CSS source provenance", () => {
  it.each([
    "",
    " \r\n ",
    "\0",
    "@import 'https://example.invalid/a.css';",
    "x".repeat(20_001),
    "x".repeat(20_001) + "url(https://example.invalid/a)",
  ])("preserves exact original span through changed body and portable recovery (%#)", (css) => {
    const rawBlock = `<!--\tSTARLANE:custom-css -->${css}<!-- /STARLANE:custom-css\t-->`;
    const source = `Before\r\n${rawBlock}\r\nAfter`;
    const inspected = inspectPageCssSource(source);
    expect(inspected).toEqual({
      kind: "single",
      provenance: { schemaVersion: 1, kind: "single", rawBlock },
      bodySource: "Before\r\n\r\nAfter",
      renderCss: readCustomCss(source),
    });
    const recovered: unknown = JSON.parse(JSON.stringify(inspected.provenance));
    expect(validatePageCssProvenance(recovered)).toBe(true);
    if (!validatePageCssProvenance(recovered)) throw new Error("fixture provenance");
    const rebuilt = rebuildPageCssSource("Edited body", recovered);
    expect(rebuilt.kind).toBe("ready");
    if (rebuilt.kind !== "ready") throw new Error("fixture rebuild");
    expect(rebuilt.content.endsWith(rawBlock)).toBe(true);
    expect(inspectPageCssSource(rebuilt.content).provenance).toEqual(recovered);
    expect(recovered).not.toHaveProperty("originalSource");
  });

  it("distinguishes absent CSS from explicitly empty CSS", () => {
    const absent = inspectPageCssSource("Body\0\r\n<!-- ordinary comment -->");
    const empty = inspectPageCssSource(block(""));
    expect(absent.provenance).toEqual({ schemaVersion: 1, kind: "absent" });
    expect(empty.provenance).toEqual({ schemaVersion: 1, kind: "single", rawBlock: block("") });
    expect(rebuildPageCssSource("Edited", absent.provenance)).toEqual({
      kind: "ready",
      content: "Edited",
    });
    expect(rebuildPageCssSource("", empty.provenance)).toEqual({
      kind: "ready",
      content: block(""),
    });
  });

  it.each(["\r\n", "\n", "\r"])(
    "uses raw block newline convention without changing body or block (%j)",
    (newline) => {
      const rawBlock = block(`${newline}.notice { font-weight: 700; }${newline}`);
      expect(
        rebuildPageCssSource("Exact body\0\n", { schemaVersion: 1, kind: "single", rawBlock }),
      ).toEqual({ kind: "ready", content: `Exact body\0\n${newline}${newline}${rawBlock}` });
    },
  );

  it.each([
    `${block("one")}${block("two")}`,
    `${OPEN}outer${OPEN}inner${CLOSE}${CLOSE}`,
    `${OPEN}unclosed`,
    `${CLOSE}reversed${OPEN}`,
    "Body <!-- starlane:custom-css",
    `${block("good")} <!-- /starlane:custom-css broken`,
  ])("retains ambiguous whole source and holds ordinary reconstruction (%#)", (originalSource) => {
    const inspected = inspectPageCssSource(originalSource);
    expect(inspected.kind).toBe("ambiguous");
    expect(inspected.bodySource).toBe(originalSource);
    const provenance = JSON.parse(JSON.stringify(inspected.provenance)) as PageCssProvenance;
    expect(validatePageCssProvenance(provenance)).toBe(true);
    const result = rebuildPageCssSource("Unsaved edit\0\r\n", provenance);
    expect(result).toEqual({
      kind: "held",
      reason: "ambiguous_original",
      originalSource,
      editedBody: "Unsaved edit\0\r\n",
    });
    expect(result).not.toHaveProperty("content");
  });

  it.each([block("new"), "<!-- starlane:custom-css", `${CLOSE}bad${OPEN}`])(
    "holds newly introduced marker content (%#)",
    (editedBody) => {
      expect(rebuildPageCssSource(editedBody, { schemaVersion: 1, kind: "absent" })).toEqual({
        kind: "held",
        reason: "markers_in_edited_body",
        editedBody,
      });
    },
  );

  it.each([
    undefined,
    null,
    [],
    {},
    { schemaVersion: 2, kind: "absent" },
    { schemaVersion: 1, kind: "unknown" },
    { schemaVersion: 1, kind: "absent", rawBlock: "" },
    { schemaVersion: 1, kind: "single", rawBlock: null },
    { schemaVersion: 1, kind: "single", rawBlock: `outside${block("raw")}` },
    { schemaVersion: 1, kind: "single", rawBlock: `${block("raw")}\n` },
    { schemaVersion: 1, kind: "single", rawBlock: `${block("a")}${block("b")}` },
    { schemaVersion: 1, kind: "ambiguous", originalSource: "plain" },
    { schemaVersion: 1, kind: "ambiguous", originalSource: block("valid") },
    { schemaVersion: 1, kind: "ambiguous", originalSource: 1 },
  ])("rejects malformed or misclassified persisted provenance (%#)", (value) => {
    expect(validatePageCssProvenance(value)).toBe(false);
  });

  it("rejects invalid runtime provenance rather than emitting its bytes", () => {
    expect(() =>
      rebuildPageCssSource("body", {
        schemaVersion: 1,
        kind: "single",
        rawBlock: "not a CSS span",
      }),
    ).toThrow("Invalid page CSS provenance");
  });

  it("counts retained UTF-16 characters without normalizing or counting unrelated body", () => {
    const single: PageCssProvenance = {
      schemaVersion: 1,
      kind: "single",
      rawBlock: block("🦅\0\r\n"),
    };
    const ambiguous: PageCssProvenance = {
      schemaVersion: 1,
      kind: "ambiguous",
      originalSource: `🦅\0${OPEN}`,
    };
    expect(retainedPageCssTextLength({ schemaVersion: 1, kind: "absent" })).toBe(0);
    expect(retainedPageCssTextLength(single)).toBe(OPEN.length + CLOSE.length + 5);
    expect(retainedPageCssTextLength(ambiguous)).toBe(OPEN.length + 3);
    expect(
      retainedPageCssTextLength(
        inspectPageCssSource(`long unrelated body ${block("")}`).provenance,
      ),
    ).toBe(OPEN.length + CLOSE.length);
  });

  it("keeps approved rendering compatible while retaining dangerous oversized CSS only as data", () => {
    expect(inspectPageCssSource(block(" \r\n.notice { font-weight: 700; }\r\n ")).renderCss).toBe(
      ".notice { font-weight: 700; }",
    );
    const original = block("x".repeat(20_001) + "url(https://example.invalid/blocked)");
    const inspected = inspectPageCssSource(original);
    expect(inspected.provenance).toEqual({ schemaVersion: 1, kind: "single", rawBlock: original });
    expect(inspected.renderCss).toBe("");
    expect(validatePageCssProvenance(inspected.provenance)).toBe(true);
  });
  it.each([
    "Body <!-- starlane:custom-c",
    "Body <!-- / starlane:custom-c",
    `${OPEN}raw<!-- / starlane:custom-css broken`,
    `${block("good")}<!-- starlane:custom-c`,
    `${block("good")}<!-- /  starlane:custom`,
  ])(
    "holds conservative partial custom prefixes and persists their original (%#)",
    (originalSource) => {
      const inspected = inspectPageCssSource(originalSource);
      expect(inspected).toMatchObject({
        kind: "ambiguous",
        bodySource: originalSource,
        provenance: { schemaVersion: 1, kind: "ambiguous", originalSource },
      });
      const retained: unknown = JSON.parse(JSON.stringify(inspected.provenance));
      expect(validatePageCssProvenance(retained)).toBe(true);
      if (!validatePageCssProvenance(retained)) throw new Error("fixture provenance");
      expect(rebuildPageCssSource("Edited body", retained)).toEqual({
        kind: "held",
        reason: "ambiguous_original",
        originalSource,
        editedBody: "Edited body",
      });
      expect(
        validatePageCssProvenance({ schemaVersion: 1, kind: "single", rawBlock: originalSource }),
      ).toBe(false);
    },
  );

  it("recognizes closing-marker whitespace without relaxing the rendering sanitizer", () => {
    const rawBlock = `${OPEN}.notice {font-weight:700}<!-- /  starlane:custom-css -->`;
    const inspected = inspectPageCssSource(`Body${rawBlock}`);
    expect(inspected).toMatchObject({
      kind: "single",
      bodySource: "Body",
      provenance: { schemaVersion: 1, kind: "single", rawBlock },
    });
    expect(validatePageCssProvenance(inspected.provenance)).toBe(true);
    expect(inspected.renderCss).toBe(readCustomCss(`Body${rawBlock}`));
    expect(inspected.renderCss).toBe("");
    expect(rebuildPageCssSource("Edit", inspected.provenance)).toEqual({
      kind: "ready",
      content: `Edit\n\n${rawBlock}`,
    });
    expect(
      rebuildPageCssSource("Edit <!-- / starlane:custom-c", { schemaVersion: 1, kind: "absent" }),
    ).toEqual({
      kind: "held",
      reason: "markers_in_edited_body",
      editedBody: "Edit <!-- / starlane:custom-c",
    });
  });
});
