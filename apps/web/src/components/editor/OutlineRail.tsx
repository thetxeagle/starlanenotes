import { useCallback, useEffect, useRef, useState } from "react";
import { parseDocumentHeadings, type DocumentHeading } from "@/lib/documentOutline";

interface OutlineRailProps {
  content: string;
  editorRef?: React.RefObject<HTMLElement>;
}

export function OutlineRail({ content }: OutlineRailProps) {
  const [headings, setHeadings] = useState<DocumentHeading[]>([]);
  const [activeIndex] = useState(0);
  const [hovered, setHovered] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout>>();

  // Parse headings from markdown content
  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);

    debounceRef.current = setTimeout(() => {
      setHeadings(parseDocumentHeadings(content));
    }, 300);

    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [content]);

  const scrollToHeading = useCallback(
    (heading: DocumentHeading) => {
      // Find the line in the editor and scroll to it
      const lines = content.split("\n");
      const lineIndex = lines.findIndex(
        (line) => line.match(/^#{1,6}\s+/) && line.includes(heading.text),
      );

      if (lineIndex >= 0) {
        // Approximate scroll position based on line number
        const editor = document.querySelector("textarea");
        if (editor) {
          const lineHeight = parseInt(getComputedStyle(editor).lineHeight) || 22;
          editor.scrollTop = lineIndex * lineHeight;
          // Place cursor at the heading
          let charPos = 0;
          for (let i = 0; i < lineIndex; i++) {
            charPos += lines[i].length + 1;
          }
          editor.setSelectionRange(charPos, charPos);
          editor.focus();
        }
      }

      setHovered(false);
    },
    [content],
  );

  // Don't render if fewer than 2 headings
  if (headings.length < 2) return null;

  // Tick mark widths by heading level
  const tickWidth = (level: number): number => {
    switch (level) {
      case 1: return 12;
      case 2: return 9;
      case 3: return 7;
      default: return 5;
    }
  };

  return (
    <div
      className="absolute right-0 top-0 z-10 h-full w-3"
      style={{ pointerEvents: "none" }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {/* Tick marks */}
      <div className="relative h-full" style={{ pointerEvents: "auto" }}>
        {headings.map((h, i) => (
          <div
            key={h.id}
            className="absolute right-0 cursor-pointer"
            style={{
              top: `${h.position * 100}%`,
              width: `${tickWidth(h.level)}px`,
              height: i === activeIndex ? "4px" : "2px",
              backgroundColor: i === activeIndex ? "var(--primary)" : "var(--text-muted)",
              opacity: i === activeIndex ? 1 : 0.3,
              borderRadius: "1px",
            }}
            onClick={() => scrollToHeading(h)}
          />
        ))}
      </div>

      {/* Flyout panel on hover */}
      {hovered && (
        <div
          className="absolute right-4 top-0 w-48 rounded-input border border-border bg-surface p-2 shadow-card"
          style={{
            pointerEvents: "auto",
            opacity: 1,
            transition: "opacity 100ms ease",
          }}
          onMouseLeave={() => setHovered(false)}
        >
          {headings.map((h, i) => (
            <button
              key={h.id}
              onClick={() => scrollToHeading(h)}
              className={`block w-full truncate rounded-button px-1 py-0.5 text-left text-xs transition-colors duration-100 ${
                i === activeIndex
                  ? "font-medium text-text"
                  : "text-text-muted hover:text-text"
              }`}
              style={{
                paddingLeft: `${(h.level - 1) * 12 + 4}px`,
              }}
            >
              {h.text}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
