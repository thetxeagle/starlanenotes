import type { TemplatePresentation } from "@/lib/templates";

/** Applies document-level width metadata while preserving other frontmatter fields. */
export function applyTemplatePageWidth(content: string, presentation: TemplatePresentation): string {
  const width = presentation.page_width;
  if (!width) return content;

  const frontmatter = content.match(/^---\n([\s\S]*?)\n---\n?/);
  if (!frontmatter) return `---\npage_width: ${width}\n---\n\n${content}`;

  const lines = frontmatter[1].split("\n");
  const widthIndex = lines.findIndex((line) => /^page_width\s*:/i.test(line));
  if (widthIndex >= 0) lines[widthIndex] = `page_width: ${width}`;
  else lines.push(`page_width: ${width}`);
  return `---\n${lines.join("\n")}\n---\n\n${content.slice(frontmatter[0].length).replace(/^\n*/, "")}`;
}
