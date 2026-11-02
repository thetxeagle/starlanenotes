import { CircleAlert, CircleCheck, Info, Lightbulb, ShieldAlert } from "lucide-react";
import { Select } from "@/components/ui/select";

export const CALLOUT_KINDS = ["note", "tip", "important", "warning", "caution"] as const;

export type CalloutKind = (typeof CALLOUT_KINDS)[number];

const CALLOUT_DETAILS: Record<
  CalloutKind,
  { label: string; description: string; icon: typeof Info; toneClass: string }
> = {
  note: { label: "Note", description: "Helpful context", icon: Info, toneClass: "text-primary" },
  tip: { label: "Tip", description: "A useful suggestion", icon: Lightbulb, toneClass: "text-success" },
  important: { label: "Important", description: "Key information", icon: CircleCheck, toneClass: "text-primary" },
  warning: { label: "Warning", description: "Proceed carefully", icon: CircleAlert, toneClass: "text-warning" },
  caution: { label: "Caution", description: "Potentially harmful action", icon: ShieldAlert, toneClass: "text-danger" },
};

export function normalizeCalloutKind(value: string | null | undefined): CalloutKind {
  const normalized = value?.trim().toLowerCase();
  return CALLOUT_KINDS.includes(normalized as CalloutKind) ? (normalized as CalloutKind) : "note";
}

export function parseCalloutDirective(value: string): { kind: CalloutKind; content: string } | null {
  const lines = value.trim().split("\n");
  const marker = lines[0]?.match(/^>\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*$/i);
  if (!marker) return null;

  const bodyLines = lines.slice(1);
  if (bodyLines.some((line) => line.trim() && !/^>\s?/.test(line))) return null;

  return {
    kind: normalizeCalloutKind(marker[1]),
    content: bodyLines.map((line) => line.replace(/^>\s?/, "")).join("\n").trim(),
  };
}

export function serializeCalloutDirective(kind: CalloutKind, content: string): string {
  const quotedContent = content
    .trim()
    .split("\n")
    .map((line) => `> ${line}`)
    .join("\n");
  return [`> [!${kind.toUpperCase()}]`, quotedContent].filter(Boolean).join("\n");
}

export function CalloutHeader({
  kind,
  editable = false,
  onKindChange,
}: {
  kind: CalloutKind;
  editable?: boolean;
  onKindChange?: (kind: CalloutKind) => void;
}) {
  const details = CALLOUT_DETAILS[kind];
  const Icon = details.icon;

  return (
    <div className="flex items-center justify-between gap-3 border-b border-border pb-2">
      <div className="flex min-w-0 items-center gap-2">
        <Icon aria-hidden="true" size={16} className={details.toneClass} />
        <div className="min-w-0">
          <p className="text-sm font-medium text-text">{details.label}</p>
          <p className="text-xs text-text-muted">{details.description}</p>
        </div>
      </div>
      {editable ? (
        <Select
          aria-label="Callout type"
          value={kind}
          onChange={(event) => onKindChange?.(normalizeCalloutKind(event.target.value))}
          containerClassName="w-28"
          className="h-8 w-28 text-xs"
          contentEditable={false}
        >
          {CALLOUT_KINDS.map((option) => (
            <option key={option} value={option}>
              {CALLOUT_DETAILS[option].label}
            </option>
          ))}
        </Select>
      ) : null}
    </div>
  );
}
