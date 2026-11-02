import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/lib/api";
import type { DocumentPropertyValueRecord } from "@/lib/dataClient";
import {
  acceptSynopsisInput,
  canEditContentMetadataValue,
  canEditLoadedMetadataValue,
  canEditPropertiesValue,
  getPropertyDeleteConfirmationMessage,
  getMetadataReadErrorKind,
  getMatchingTags,
  loadMetadataCollection,
  persistWithRollback,
  persistContentMetadataValue,
  PropertiesPanel,
  PROPERTIES_PANEL_ERROR_CLASS,
  PropertyValueControl,
  PropertyTypeSelect,
  shouldShowInlineTagSuggestions,
} from "./PropertiesPanel";

describe("Page metadata editing", () => {
  it("gives the original draft synchronous ownership of exact synopsis typing without saving", () => {
    const proposal = "  Local summary\r\n😀\n";
    const owned: string[] = [];
    acceptSynopsisInput(proposal, true, true, (value) => {
      owned.push(value);
    });
    expect(owned).toEqual([proposal]);
  });

  it("refuses unsupported or stale draft ownership before accepting a synopsis proposal", () => {
    const change = vi.fn();
    expect(() => acceptSynopsisInput("local", false, true, change)).toThrow("editable draft");
    expect(change).not.toHaveBeenCalled();
    expect(() => acceptSynopsisInput("local", true, true)).toThrow("unavailable");
    const stale = vi.fn(() => {
      throw new Error("Original draft changed");
    });
    expect(() => acceptSynopsisInput("local", true, true, stale)).toThrow("Original draft changed");
  });

  it("keeps standalone synopsis editing available without a draft callback", () => {
    expect(() => acceptSynopsisInput("", true, false)).not.toThrow();
  });

  it("requires editability, an unlocked page, and a page-draft callback when draft-managed", () => {
    expect(canEditContentMetadataValue(true, false, true, true)).toBe(true);
    expect(canEditContentMetadataValue(false, false, true, true)).toBe(false);
    expect(canEditContentMetadataValue(true, true, true, true)).toBe(false);
    expect(canEditContentMetadataValue(true, false, true, false)).toBe(false);
  });

  it("uses the supplied page-draft callback without invoking the standalone body updater", async () => {
    const onPageDraftUpdate = vi.fn(async () => undefined);
    const onStandaloneUpdate = vi.fn(async () => undefined);

    await persistContentMetadataValue(
      "Updated summary",
      true,
      onPageDraftUpdate,
      onStandaloneUpdate,
    );

    expect(onPageDraftUpdate).toHaveBeenCalledWith("Updated summary");
    expect(onStandaloneUpdate).not.toHaveBeenCalled();
  });

  it("preserves the standalone metadata update path when no page-draft controls are supplied", async () => {
    const onPageDraftUpdate = vi.fn(async () => undefined);
    const onStandaloneUpdate = vi.fn(async () => undefined);

    await persistContentMetadataValue(
      "Standalone summary",
      false,
      onPageDraftUpdate,
      onStandaloneUpdate,
    );

    expect(onStandaloneUpdate).toHaveBeenCalledWith("Standalone summary");
    expect(onPageDraftUpdate).not.toHaveBeenCalled();
  });

  it("refuses draft-managed saves without a callback instead of falling back to published content", async () => {
    const onStandaloneUpdate = vi.fn(async () => undefined);

    await expect(
      persistContentMetadataValue("Updated summary", true, undefined, onStandaloneUpdate),
    ).rejects.toThrow("Page metadata updates require an editable draft callback.");
    expect(onStandaloneUpdate).not.toHaveBeenCalled();
  });

  it("disables synopsis writes while retaining read-only CSS recovery on locked readers", () => {
    const html = renderToStaticMarkup(
      <PropertiesPanel
        spaceId="space-1"
        docId="doc-1"
        title="Page"
        locked
        contentMetadataEditable={false}
        synopsis="Draft summary"
        pageCssSource={{
          spaceId: "space-1",
          documentId: "doc-1",
          draftId: "draft-1",
          provenance: {
            schemaVersion: 1,
            kind: "single",
            rawBlock: "<!-- starlane:custom-css --><!-- /starlane:custom-css -->",
          },
        }}
      />,
    );
    expect(html).toMatch(/<textarea[^>]*aria-label="Page synopsis"[^>]*disabled=""/);
    expect(html).toContain("Open an editable draft to change the page summary.");
    expect(html).toContain("View retained CSS");
    const opener = html.match(/<button[^>]*>View retained CSS<\/button>/)?.[0];
    expect(opener).toBeDefined();
    expect(opener).not.toContain('disabled=""');
    for (const retired of [
      "Add custom CSS",
      "Edit custom CSS",
      "Save CSS",
      "Indent CSS",
      "Outdent CSS",
      "Custom page CSS",
    ])
      expect(html).not.toContain(retired);
  });

  it.each([
    undefined,
    {
      spaceId: "other-space",
      documentId: "doc-1",
      provenance: {
        schemaVersion: 1 as const,
        kind: "single" as const,
        rawBlock: "<!-- starlane:custom-css -->private<!-- /starlane:custom-css -->",
      },
    },
    {
      spaceId: "space-1",
      documentId: "other-page",
      provenance: {
        schemaVersion: 1 as const,
        kind: "single" as const,
        rawBlock: "<!-- starlane:custom-css -->private<!-- /starlane:custom-css -->",
      },
    },
    {
      spaceId: "space-1",
      documentId: "doc-1",
      provenance: { schemaVersion: 1 as const, kind: "absent" as const },
    },
  ])("omits CSS recovery when exact current source is unavailable (%#)", (pageCssSource) => {
    const html = renderToStaticMarkup(
      <PropertiesPanel
        spaceId="space-1"
        docId="doc-1"
        title="Page"
        synopsis="Summary"
        pageCssSource={pageCssSource}
      />,
    );
    expect(html).not.toContain("View retained CSS");
    expect(html).not.toContain("Add custom CSS");
    expect(html).not.toContain("private");
  });
});

describe("Page property and tag permissions", () => {
  it("requires both role capability and an unlocked page for page value writes", () => {
    expect(canEditPropertiesValue(true, false)).toBe(true);
    expect(canEditPropertiesValue(false, false)).toBe(false);
    expect(canEditPropertiesValue(true, true)).toBe(false);
  });

  it("blocks metadata writes until the current collection has loaded", () => {
    expect(canEditLoadedMetadataValue(true, true, null)).toBe(false);
    expect(canEditLoadedMetadataValue(true, false, "permission")).toBe(false);
    expect(canEditLoadedMetadataValue(true, false, "unavailable")).toBe(false);
    expect(canEditLoadedMetadataValue(true, false, null)).toBe(true);
  });

  it("distinguishes permission denial from retryable collection failures", async () => {
    expect(getMetadataReadErrorKind(new ApiError("Denied", 403))).toBe("permission");
    expect(getMetadataReadErrorKind(new Error("Offline"))).toBe("unavailable");
    await expect(
      loadMetadataCollection(async () => {
        throw new ApiError("Denied", 403);
      }),
    ).resolves.toEqual({ items: [], error: "permission" });
    await expect(loadMetadataCollection(async () => [])).resolves.toEqual({
      items: [],
      error: null,
    });
  });

  it("rolls back a failed optimistic property write and reports an actionable error", async () => {
    const rollback = vi.fn();
    const onError = vi.fn();
    const persist = vi.fn(async () => {
      throw new ApiError("Permission denied", 403);
    });

    await expect(
      persistWithRollback(
        persist,
        rollback,
        onError,
        "Could not save this page property. The previous value was restored. Try again.",
      ),
    ).resolves.toBe(false);

    expect(rollback).toHaveBeenCalledOnce();
    expect(onError).toHaveBeenCalledWith(
      "Could not save this page property. The previous value was restored. Try again. Permission denied",
    );
  });

  it("keeps property schemas unavailable unless the caller grants schema editing", () => {
    const html = renderToStaticMarkup(
      <PropertiesPanel
        spaceId="space-1"
        docId="doc-1"
        title="Page"
        pageValuesEditable={true}
        schemaEditable={false}
      />,
    );

    expect(html).toMatch(
      /<button(?=[^>]*aria-label="Add custom property")(?=[^>]*disabled="")[^>]*>/,
    );
    expect(html).toContain("Your space role must allow schema changes to create or edit tags.");
    expect(html).toContain("with space schema-edit access to change custom properties.");
    expect(html).toContain("Loading custom properties…");
    expect(html).not.toContain("No custom properties");
    expect(html).not.toContain("No tags");
  });

  it("keeps schema deletion failures visible while confirmation remains open", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/PropertiesPanel.tsx"),
      "utf8",
    );

    expect(source).toContain("helperText={metadataWriteError || undefined}");
    expect(source).toContain("if (!propertySchemaEditable || !deletePropertyTarget) return;");
  });
});

describe("PropertyTypeSelect", () => {
  it("owns cover-image management inside Page Details", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/PropertiesPanel.tsx"),
      "utf8",
    );

    expect(source).toContain("<CoverImage");
    expect(source).toContain("compact");
    expect(source).toContain("coverEditable && Boolean(onCoverUpdate)");
    expect(source).toContain("Open an editable draft to add a cover.");
  });

  it("omits retired page-width controls from Page Details", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/PropertiesPanel.tsx"),
      "utf8",
    );

    expect(source).not.toContain("<DocumentWidthSelector");
  });

  it("uses semantic danger tokens for workflow errors", () => {
    expect(PROPERTIES_PANEL_ERROR_CLASS).toContain("border-danger");
    expect(PROPERTIES_PANEL_ERROR_CLASS).toContain("bg-danger");
    expect(PROPERTIES_PANEL_ERROR_CLASS).toContain("text-danger");
    expect(PROPERTIES_PANEL_ERROR_CLASS).not.toContain("red-");
  });

  it("uses the shared anchored property-type picker", () => {
    const html = renderToStaticMarkup(
      <PropertyTypeSelect value="text" onChange={() => undefined} />,
    );

    expect(html).toContain('aria-label="Property type"');
    expect(html).toContain('aria-haspopup="menu"');
    expect(html).toContain("rounded-input");
    expect(html).not.toContain("<option");
  });

  it("offers an accessible zero-to-five star rating property", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/PropertiesPanel.tsx"),
      "utf8",
    );

    expect(source).toContain('["rating", "Rating"]');
    expect(source).toContain('property.property_type === "rating"');
    expect(source).toContain('role="radiogroup"');
    expect(source).toContain("aria-label={`${star} star");
  });

  it("uses clear destructive property deletion confirmation copy", () => {
    expect(getPropertyDeleteConfirmationMessage("Priority")).toBe(
      'Delete property "Priority" from this space? Existing values will be removed from documents.',
    );
    expect(getPropertyDeleteConfirmationMessage("   ")).toBe(
      "Delete this property from the space? Existing values will be removed from documents.",
    );
  });

  it("gives tag-color menus the shared overlay surface contract", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/PropertiesPanel.tsx"),
      "utf8",
    );

    expect(source).toContain("<Popover open={open} onOpenChange={onOpenChange}>");
    expect(source).toContain('role="menu"');
    expect(source).toContain('aria-label="Tag colors"');
    expect(source).toContain("tabIndex={-1}");
    expect(source).toContain('role="menuitemradio"');
    expect(source).not.toContain('aria-label="Close tag color menu"');
  });

  it("uses the shared directional menu keyboard contract for tag colors", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/PropertiesPanel.tsx"),
      "utf8",
    );

    expect(source).toContain("useMenuKeyboardNavigation(menuRef)");
    expect(source).toContain("onKeyDown={handleMenuKeyDown}");
  });

  it("filters existing tags and prioritizes prefix matches", () => {
    const tags = [
      { id: "1", name: "Planning", color: "#2563eb" },
      { id: "2", name: "Release plan", color: "#059669" },
      { id: "3", name: "Backlog", color: "#ca8a04" },
    ];

    expect(getMatchingTags(tags, "plan").map((tag) => tag.name)).toEqual([
      "Planning",
      "Release plan",
    ]);
    expect(getMatchingTags(tags, "").map((tag) => tag.name)).toEqual([
      "Backlog",
      "Planning",
      "Release plan",
    ]);
  });

  it("only opens inline suggestions for focused queries with existing matches", () => {
    const tags = [{ id: "1", name: "Planning", color: "#2563eb" }];

    expect(shouldShowInlineTagSuggestions(true, "plan", tags, false)).toBe(true);
    expect(shouldShowInlineTagSuggestions(true, "hello", [], false)).toBe(false);
    expect(shouldShowInlineTagSuggestions(false, "plan", tags, false)).toBe(false);
    expect(shouldShowInlineTagSuggestions(true, "plan", tags, true)).toBe(false);
  });

  it("uses centered dialogs and the shared property composer", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/PropertiesPanel.tsx"),
      "utf8",
    );

    expect(source).toContain('labelledBy="add-document-tag-title"');
    expect(source).toContain('labelledBy="create-property-title"');
    expect(source).toContain('role="listbox" aria-label="Existing tags"');
    expect(source).toContain("className={`focus-ring border-border bg-surface");
    expect(source).not.toContain("list={`available-tags-${docId}`}");
    expect(source).toContain('className="max-h-[calc(100dvh-8rem)] overflow-y-auto"');
    expect(source).toContain('aria-label="Property types"');
    expect(source).toContain('placeholder="Property name"');
    expect(source).toContain("sm:grid-cols-[minmax(0,1fr)_13rem]");
    expect(source).toContain('className="h-10"');
  });

  it("uses an inline tag input with create, edit, and remove affordances", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/PropertiesPanel.tsx"),
      "utf8",
    );

    expect(source).toContain('aria-label="Add or search tags"');
    expect(source).toContain('aria-label="Add tag"');
    expect(source).toContain("aria-label={`Remove ${tag.name}`}");
    expect(source).toContain("title={`Edit ${tag.name}`}");
    expect(source).not.toContain("Create “{newTagName.trim()}”");
    expect(source).toContain("shouldShowInlineTagSuggestions");
  });
});

describe("PropertyValueControl accessible names", () => {
  const renderControl = (property: DocumentPropertyValueRecord) =>
    renderToStaticMarkup(
      <PropertyValueControl
        property={property}
        relationDocuments={[]}
        userDirectory={[]}
        onChange={() => undefined}
      />,
    );

  it("names scalar values and checkboxes with the property name", () => {
    const text = renderControl({
      property_id: "property-1",
      property_name: "Project URL",
      property_type: "url",
      value: "",
    });
    const checkbox = renderControl({
      property_id: "property-2",
      property_name: "Reviewed",
      property_type: "checkbox",
      value: "false",
    });

    expect(text).toContain('aria-label="Project URL"');
    expect(checkbox).toContain('aria-label="Reviewed"');
  });

  it("adds property context to multi-select option names without changing visible labels", () => {
    const html = renderControl({
      property_id: "property-3",
      property_name: "Environment",
      property_type: "multi_select",
      options: ["Staging"],
      value: "[]",
    });

    expect(html).toContain('aria-label="Environment: Staging"');
    expect(html).toContain(">Staging</label>");
  });

  it("disables property value controls when page values are not editable", () => {
    const html = renderToStaticMarkup(
      <PropertyValueControl
        property={{
          property_id: "property-4",
          property_name: "Reviewed",
          property_type: "checkbox",
          value: "false",
        }}
        relationDocuments={[]}
        userDirectory={[]}
        editable={false}
        onChange={() => undefined}
      />,
    );

    expect(html).toContain('type="checkbox"');
    expect(html).toContain('disabled=""');
  });
});
