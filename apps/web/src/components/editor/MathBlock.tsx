import katex from "katex";
import "katex/dist/katex.min.css";
import { useId } from "react";
import { Textarea } from "@/components/ui/textarea";

export const MATH_BLOCK_DELIMITER = "$$";

export function parseMathBlockDirective(value: string): string | null {
  const match = value.trim().match(/^\$\$\n([\s\S]*?)\n\$\$$/);
  return match ? match[1].trim() : null;
}

export function serializeMathBlockDirective(expression: string): string {
  return `${MATH_BLOCK_DELIMITER}\n${expression.trim()}\n${MATH_BLOCK_DELIMITER}`;
}

export function renderMathExpression(expression: string): string {
  return katex.renderToString(expression.trim() || "\\text{Enter a LaTeX equation}", {
    displayMode: true,
    throwOnError: false,
    strict: "ignore",
    trust: false,
  });
}

export function MathBlockDisplay({ expression }: { expression: string }) {
  return (
    <div
      data-math-rendered="true"
      aria-label={`Rendered math: ${expression.trim() || "empty equation"}`}
      className="overflow-x-auto py-2 text-text"
      dangerouslySetInnerHTML={{ __html: renderMathExpression(expression) }}
    />
  );
}

export function MathBlockEditor({
  expression,
  onExpressionChange,
}: {
  expression: string;
  onExpressionChange: (expression: string) => void;
}) {
  const fieldId = useId();
  const helpId = `${fieldId}-help`;

  return (
    <div className="my-4 rounded-card border border-border bg-surface p-4" contentEditable={false}>
      <label htmlFor={fieldId} className="text-sm font-medium text-text">
        LaTeX equation
      </label>
      <Textarea
        id={fieldId}
        aria-describedby={helpId}
        value={expression}
        onChange={(event) => onExpressionChange(event.target.value)}
        placeholder="E = mc^2"
        rows={2}
        className="mt-2 min-h-20 resize-y font-mono"
      />
      <p id={helpId} className="mt-2 text-xs text-text-muted">
        Stored as portable $$ … $$ Markdown.
      </p>
      <MathBlockDisplay expression={expression} />
    </div>
  );
}
