import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import {
  filterSlashPathOptions,
  getNextSlashPathActiveIndex,
  SlashPathPicker,
  splitSlashPathSegments,
  type SlashPathPickerOption,
} from "./SlashPathPicker";

const options: SlashPathPickerOption[] = [
  {
    id: "root",
    label: "Personal Space / Root",
    description: "Top level",
    icon: "folder",
  },
  {
    id: "folder-launch",
    label: "Personal Space / Projects / Launch",
    description: "Folder",
    icon: "folder",
  },
  {
    id: "doc-roadmap",
    label: "Personal Space / Projects / Roadmap",
    description: "Child page",
    icon: "document",
  },
];

describe("SlashPathPicker path matching", () => {
  it("splits slash paths into trimmed segments", () => {
    expect(splitSlashPathSegments(" Personal Space / Projects / Roadmap ")).toEqual([
      "Personal Space",
      "Projects",
      "Roadmap",
    ]);
    expect(splitSlashPathSegments("///")).toEqual([]);
  });

  it("matches ordered slash-token segments across full destination labels", () => {
    expect(filterSlashPathOptions(options, "personal / proj").map((option) => option.id)).toEqual([
      "folder-launch",
      "doc-roadmap",
    ]);
    expect(filterSlashPathOptions(options, "proj / road").map((option) => option.id)).toEqual([
      "doc-roadmap",
    ]);
    expect(filterSlashPathOptions(options, "road / proj")).toEqual([]);
  });

  it("wraps keyboard navigation across available options", () => {
    expect(getNextSlashPathActiveIndex(0, 3, "next")).toBe(1);
    expect(getNextSlashPathActiveIndex(2, 3, "next")).toBe(0);
    expect(getNextSlashPathActiveIndex(0, 3, "previous")).toBe(2);
    expect(getNextSlashPathActiveIndex(0, 0, "next")).toBe(-1);
  });
});

describe("SlashPathPicker rendering", () => {
  it("renders a mobile-friendly combobox and touch-sized results", () => {
    const html = renderToStaticMarkup(
      <SlashPathPicker
        id="destination-picker"
        value="Personal / Proj"
        options={options}
        selectedOptionId="doc-roadmap"
        placeholder="Personal Space / Folder / Page"
        ariaLabel="Search destination"
        onInputChange={vi.fn()}
        onSelect={vi.fn()}
      />,
    );

    expect(html).toContain('role="combobox"');
    expect(html).toContain('aria-autocomplete="list"');
    expect(html).toContain("calc(100dvh-14rem)");
    expect(html).toContain("min-h-11");
    expect(html).toContain("Personal Space / Projects / Roadmap");
  });
});
