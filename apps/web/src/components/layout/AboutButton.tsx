import { useState } from "react";
import { Info, X } from "lucide-react";
import { DialogShell, DialogHeader, DialogBody } from "@/components/ui/dialog-shell";
import { getStarlaneRepositoryUrl, STARLANE_VERSION } from "@/lib/appMetadata";

export function AboutButton({ touchTarget = false }: { touchTarget?: boolean }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        aria-label="About Starlane Notes"
        aria-haspopup="dialog"
        aria-expanded={open}
        title="About Starlane Notes"
        className={`rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex shrink-0 items-center gap-1.5 px-2 text-xs transition-colors duration-100 ${touchTarget ? "h-11 min-h-11" : "h-8"}`}
      >
        <Info size={16} aria-hidden="true" />
        <span>v{STARLANE_VERSION}</span>
      </button>
      {open && (
        <DialogShell labelledBy="about-starlane-title" maxWidth="sm" onClose={() => setOpen(false)}>
          <DialogHeader className="flex items-center justify-between gap-3">
            <h2 id="about-starlane-title" className="text-text text-base font-semibold">About Starlane Notes</h2>
            <button type="button" onClick={() => setOpen(false)} aria-label="Close About Starlane Notes" className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex size-8 shrink-0 items-center justify-center"><X size={16} aria-hidden="true" /></button>
          </DialogHeader>
          <DialogBody>
            <p className="text-text-muted text-sm">A focused, self-hosted workspace for your notes.</p>
            <div className="border-border bg-surface-raised text-text-muted rounded-input mt-4 flex items-center justify-between border px-3 py-2 text-sm">
              <span>Version</span><span className="text-text font-medium">v{STARLANE_VERSION}</span>
            </div>
            <a href={getStarlaneRepositoryUrl()} target="_blank" rel="noreferrer" className="text-primary hover:text-text mt-3 inline-flex text-sm underline underline-offset-2">View source repository</a>
          </DialogBody>
        </DialogShell>
      )}
    </>
  );
}
