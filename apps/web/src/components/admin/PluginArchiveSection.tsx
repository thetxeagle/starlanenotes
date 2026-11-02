import { useState } from "react";
import { Textarea } from "@/components/ui/textarea";
import { readPluginArchive } from "@/lib/pluginArchive";

const ARCHIVE_LABELS = [
  "Install records",
  "Install audit",
  "Runtime audit",
];

export function PluginArchiveSection() {
  const [archive] = useState(() => readPluginArchive());
  return (
    <section aria-labelledby="plugin-archive-title" className="min-w-0 space-y-4">
      <div className="space-y-2">
        <h1 id="plugin-archive-title" className="text-text text-lg font-medium">
          Legacy plugin archive
        </h1>
        <p className="text-text-muted text-sm">
          Plugin execution is unavailable. Retained install records are not running integrations.
        </p>
        <p className="text-text-muted text-sm">
          These records belong to this browser profile. This view reads their original stored text
          without changing it. Select and copy any retained text to keep your own copy.
        </p>
      </div>
      {!archive.browserAvailable && (
        <p role="status" className="text-text-muted text-sm">
          Browser storage is unavailable. Retained records cannot be read here.
        </p>
      )}
      {archive.entries.map((entry, index) => {
        const label = ARCHIVE_LABELS[index];
        const id = `plugin-archive-${index}`;
        return (
          <div key={entry.key} className="min-w-0 space-y-2">
            {entry.state === "present" ? (
              <>
                <label htmlFor={id} className="text-text block text-sm font-medium">
                  {label}
                </label>
                <Textarea
                  id={id}
                  readOnly
                  spellCheck={false}
                  value={entry.raw ?? ""}
                  rows={8}
                  className="resize-y font-mono"
                  aria-describedby={entry.raw === "" ? `${id}-empty` : undefined}
                />
                {entry.raw === "" && (
                  <p id={`${id}-empty`} className="text-text-muted text-xs">
                    A stored empty string is retained for this record.
                  </p>
                )}
              </>
            ) : (
              <>
                <h2 className="text-text text-sm font-medium">{label}</h2>
                <p className="text-text-muted text-sm">
                  {entry.state === "absent"
                    ? "No stored value in this browser profile."
                    : "Storage could not be read. This does not mean the record is missing."}
                </p>
              </>
            )}
          </div>
        );
      })}
    </section>
  );
}
