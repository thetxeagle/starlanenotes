import { describe, expect, it } from "vitest";
import {
  PROPERTY_TYPES,
  validatePropertySchema,
  validatePropertySchemaChange,
  validatePropertyValue,
} from "./propertyValidation";
const schema = (property_type: string, options?: unknown) => ({
  name: "Field",
  property_type,
  options,
});
describe("property wire validation parity", () => {
  it("preserves unset semantics and finite numeric wire formats", () => {
    for (const type of PROPERTY_TYPES)
      for (const value of [null, ""])
        expect(() => validatePropertyValue(schema(type), value)).not.toThrow();
    for (const value of [".5", "01", "-0.5", "1e3", "+1", "1."])
      expect(() => validatePropertyValue(schema("number"), value)).not.toThrow();
    for (const value of ["NaN", "inf", "Infinity", "1e999", " 1", "1 ", "0x10", "\t1"])
      expect(() => validatePropertyValue(schema("number"), value)).toThrow();
  });
  it("validates dates, datetime inputs, semantic flags and safe URLs", () => {
    for (const [type, value] of [
      ["date", "2026-10-07"],
      ["datetime", "2026-10-07T13:30"],
      ["datetime", "2026-10-07T13:30:00Z"],
      ["checkbox", "false"],
      ["rating", "0"],
      ["url", "https://example.test/a"],
    ])
      expect(() => validatePropertyValue(schema(type), value)).not.toThrow();
    for (const [type, value] of [
      ["date", "2026-02-30"],
      ["date", "2026-7-6"],
      ["datetime", "2026-02-30T13:30"],
      ["datetime", "2026-10-07T24:00"],
      ["checkbox", "1"],
      ["rating", "1.0"],
      ["url", "javascript:alert(1)"],
    ])
      expect(() => validatePropertyValue(schema(type), value)).toThrow();
  });
  it("validates select options and duplicate-free multi-select batches", () => {
    for (const options of [["A", "B"], { options: ["A", "B"] }]) {
      expect(() => validatePropertyValue(schema("select", options), "A")).not.toThrow();
      expect(() =>
        validatePropertyValue(schema("multi_select", options), '["A","B"]'),
      ).not.toThrow();
      for (const value of ['["A","A"]', '["C"]', "[1]", "A"])
        expect(() => validatePropertyValue(schema("multi_select", options), value)).toThrow();
    }
    for (const options of [["A", "A"], [""], [1], {}])
      expect(() => validatePropertySchema(schema("select", options))).toThrow();
  });
  it("requires deliberate conversion for every nonempty type change", () => {
    expect(() => validatePropertySchemaChange(schema("text"), schema("number"), ["01"])).toThrow(
      "conversion",
    );
    expect(() =>
      validatePropertySchemaChange(schema("text"), schema("number"), [null, ""]),
    ).not.toThrow();
  });
});
