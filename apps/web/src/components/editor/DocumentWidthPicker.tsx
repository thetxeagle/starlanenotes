export const DOCUMENT_PAGE_WIDTHS = ["standard", "wide", "full"] as const;

export type DocumentPageWidth = (typeof DOCUMENT_PAGE_WIDTHS)[number];

export const DOCUMENT_PAGE_WIDTH_OPTIONS: ReadonlyArray<{
  id: DocumentPageWidth;
  label: string;
  description: string;
}> = [
  {
    id: "standard",
    label: "Standard",
    description: "Roomy reading and writing width",
  },
  {
    id: "wide",
    label: "Wide",
    description: "35% wider than standard",
  },
  {
    id: "full",
    label: "Full width",
    description: "Use the available page canvas",
  },
];

export function normalizeDocumentPageWidth(value: unknown): DocumentPageWidth {
  return DOCUMENT_PAGE_WIDTHS.includes(value as DocumentPageWidth)
    ? (value as DocumentPageWidth)
    : "standard";
}

// Legacy values remain readable; every page now uses the same fluid canvas.
export function getDocumentPageWidthClass(_width: DocumentPageWidth): string {
  return "max-w-[86.16375rem]";
}
