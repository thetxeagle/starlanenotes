export interface DocumentHeading {
  id: string;
  text: string;
  level: number;
  line: number;
  position: number;
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
      text: match[2].trim(),
      level: match[1].length,
      line: index,
      position: totalLines > 1 ? index / (totalLines - 1) : 0,
    });
  });

  return headings;
}
