export interface DocumentHeading {
  id: string;
  text: string;
  level: number;
  line: number;
  position: number;
}

function headingLabel(value: string): string {
  return value
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/(\*\*|__|`)/g, "")
    // Heading content can contain inline marks serialized as HTML. The outline
    // is plain navigation text, so never expose the source tags themselves.
    .replace(/<[^>]*>/g, "")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&nbsp;/g, " ")
    .trim();
}

export function parseDocumentHeadings(content: string): DocumentHeading[] {
  const lines = content.split("\n");
  const totalLines = lines.length;
  const headings: DocumentHeading[] = [];

  lines.forEach((line, index) => {
    const match = line.match(/^(#{1,6})\s+(.+)/);
    if (!match) return;

    headings.push({
      id: `heading-${index}`,
      text: headingLabel(match[2]),
      level: match[1].length,
      line: index,
      position: totalLines > 1 ? index / (totalLines - 1) : 0,
    });
  });

  return headings;
}
