import { FileText } from "lucide-react";
import { ContentIdentityIcon } from "@/components/ContentIdentityIcon";

export const DOCUMENT_IDENTITY_ICON_CLASS =
  "flex h-4 w-4 shrink-0 items-center justify-center text-sm leading-none";

interface DocumentIdentityIconProps {
  icon?: string;
  className?: string;
}

/**
 * A document has one visual identity everywhere: its stored emoji when set,
 * otherwise the ordinary document glyph. Keep the emoji in the icon lane so
 * list rows never duplicate it inside the title.
 */
export function DocumentIdentityIcon({ icon, className = "" }: DocumentIdentityIconProps) {
  return (
    <span className={`${DOCUMENT_IDENTITY_ICON_CLASS} ${className}`.trim()} aria-hidden="true">
      <ContentIdentityIcon
        value={icon}
        size={16}
        fallback={<FileText size={16} className="text-text-muted shrink-0" aria-hidden="true" />}
      />
    </span>
  );
}
