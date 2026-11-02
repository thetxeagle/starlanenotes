import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

export interface IdentifierPageSource {
  page_updated_at: string;
  published_at: string | null;
  human_identifier: string | null;
}
export interface IdentifierPageState {
  source: IdentifierPageSource;
  editable: boolean;
  pending?: {
    human_identifier: string | null;
    source: IdentifierPageSource;
    pending_error?: string;
  } | null;
}
export interface IdentifierSaveResult {
  operation_id: string;
  pending_error?: string;
  disposition: "queued" | "applied" | "historical";
}

export function PageIdentifierControl({
  load,
  save,
}: {
  load: () => Promise<IdentifierPageState>;
  save: (value: string | null, source: IdentifierPageSource) => Promise<IdentifierSaveResult>;
}) {
  const inputId = useId();
  const [page, setPage] = useState<IdentifierPageState | null>(null);
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [editSource, setEditSource] = useState<IdentifierPageSource | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const mounted = useRef(false);
  const editButton = useRef<HTMLButtonElement>(null);
  const wasEditing = useRef(false);
  useEffect(() => {
    if (wasEditing.current && !editing) editButton.current?.focus();
    wasEditing.current = editing;
  }, [editing]);
  useEffect(() => {
    mounted.current = true;
    let current = true;
    void load()
      .then((next) => {
        if (current) setPage(next);
      })
      .catch(() => {
        if (current) setError("Unable to load the identifier. Reopen Properties to retry.");
      });
    return () => {
      current = false;
      mounted.current = false;
    };
  }, [load]);

  async function startEdit() {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const next = await load();
      if (!mounted.current) return;
      setPage(next);
      if (!next.editable && !next.pending) {
        setError("This page is no longer available for identifier changes.");
        return;
      }
      setEditSource(next.pending?.source ?? next.source);
      setValue(
        next.pending ? (next.pending.human_identifier ?? "") : (next.source.human_identifier ?? ""),
      );
      setEditing(true);
    } catch {
      if (mounted.current) setError("Unable to load the current identifier. Try again.");
    } finally {
      if (mounted.current) setBusy(false);
    }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!page || !editSource || busy) return;
    setBusy(true);
    setError("");
    try {
      // Retain the source captured when editing began. A later revision must
      // conflict rather than silently becoming this edit's base.
      const result = await save(value.trim() || null, editSource);
      if (!mounted.current) return;
      if (result.disposition === "queued") {
        setNotice(
          `Saved on this device. Pending identifier: ${value.trim() || "Not set"}. Waiting for sync confirmation.`,
        );
        if (result.pending_error) {
          setError(result.pending_error);
          return;
        }
      } else {
        setNotice(
          result.disposition === "historical"
            ? "The earlier change was already confirmed. Loading the current identifier."
            : "Identifier saved.",
        );
        try {
          const next = await load();
          if (mounted.current) setPage(next);
        } catch {
          if (mounted.current)
            setNotice("Change confirmed. Reopen Properties to load the current identifier.");
        }
      }
      if (mounted.current) {
        setEditing(false);
        editButton.current?.focus();
      }
    } catch (reason) {
      if (mounted.current)
        setError(
          reason instanceof Error
            ? reason.message
            : "Unable to save the identifier. Your entry is retained.",
        );
    } finally {
      if (mounted.current) setBusy(false);
    }
  }

  return (
    <div className="border-border mt-3 border-t pt-3">
      <div className="flex min-w-0 items-center justify-between gap-2">
        <span className="text-text-muted text-xs">Readable identifier</span>
        {!editing && (page?.editable || page?.pending) && (
          <Button
            ref={editButton}
            variant="ghost"
            size="sm"
            disabled={busy}
            onClick={() => void startEdit()}
          >
            {busy ? "Loading…" : page?.pending ? "Retry pending identifier" : "Edit identifier"}
          </Button>
        )}
      </div>
      {!editing && (
        <p className="text-text mt-1 text-sm break-all">
          {page ? (page.source.human_identifier ?? "Not set") : error ? "Unavailable" : "Loading…"}
        </p>
      )}
      {!editing && page?.pending && (
        <div className="text-text-muted mt-2 text-xs" role="status">
          <p>
            Pending identifier: {page.pending.human_identifier ?? "Not set"}. Saved on this device;
            sync confirmation is pending.
          </p>
          {page.pending.pending_error && (
            <p className="text-danger mt-1" role="alert">
              {page.pending.pending_error}
            </p>
          )}
        </div>
      )}
      {editing && (
        <form onSubmit={(event) => void submit(event)} className="mt-2 grid gap-2">
          <label htmlFor={inputId} className="text-text-muted text-xs">
            Readable identifier
          </label>
          <Input
            id={inputId}
            autoFocus
            value={value}
            disabled={busy}
            maxLength={64}
            onChange={(event) => setValue(event.target.value)}
            aria-describedby={`${inputId}-help`}
            onKeyDown={(event) => {
              if (event.key === "Escape" && !busy) {
                event.preventDefault();
                setEditing(false);
                setError("");
                editButton.current?.focus();
              }
            }}
          />
          <p id={`${inputId}-help`} className="text-text-muted text-xs">
            Up to 64 letters, numbers, dots, underscores or hyphens. Unique within this space. Leave
            blank to clear.
          </p>
          <div className="flex flex-wrap justify-end gap-2">
            <Button
              variant="secondary"
              size="sm"
              disabled={busy}
              onClick={() => {
                setEditing(false);
                setError("");
                editButton.current?.focus();
              }}
            >
              Cancel
            </Button>
            <Button type="submit" size="sm" disabled={busy}>
              {busy ? "Saving…" : "Save identifier"}
            </Button>
          </div>
        </form>
      )}
      {error && (
        <p role="alert" className="text-danger mt-2 text-xs break-words">
          {error}
        </p>
      )}
      {notice && (
        <p role="status" className="text-text-muted mt-2 text-xs">
          {notice}
        </p>
      )}
    </div>
  );
}
