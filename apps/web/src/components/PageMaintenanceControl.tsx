import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";

export type MaintenanceChoice =
  | { kind: "unassigned" }
  | { kind: "user"; user_id: string }
  | { kind: "group"; group_id: string };
export interface MaintenanceSource {
  page_updated_at: string;
  published_at: string | null;
  maintenance_user_id: string | null;
  maintenance_group_id: string | null;
}
export interface MaintenanceState {
  assertCurrentContext: () => void;
  source: MaintenanceSource;
  editable: boolean;
  targets: {
    users: Array<{ id: string; name: string; active: true }>;
    groups: Array<{ id: string; name: string; managed_space_id: string | null }>;
    groups_available: boolean;
  };
  catalogUnavailable?: boolean;
  pending?: {
    maintenance: MaintenanceChoice;
    source: MaintenanceSource;
    pending_error?: string;
  } | null;
}
export interface MaintenanceSaveResult {
  operation_id: string;
  disposition: "queued" | "applied" | "historical";
  pending_error?: string;
}
function sourceChoice(source: MaintenanceSource): MaintenanceChoice {
  if (source.maintenance_user_id) return { kind: "user", user_id: source.maintenance_user_id };
  if (source.maintenance_group_id) return { kind: "group", group_id: source.maintenance_group_id };
  return { kind: "unassigned" };
}
function choiceLabel(choice: MaintenanceChoice, page: MaintenanceState): string {
  if (choice.kind === "unassigned") return "Unassigned";
  if (choice.kind === "user")
    return (
      page.targets.users.find((user) => user.id === choice.user_id)?.name ?? "Person unavailable"
    );
  return (
    page.targets.groups.find((group) => group.id === choice.group_id)?.name ?? "Team unavailable"
  );
}
export function PageMaintenanceControl({
  load,
  save,
}: {
  load: () => Promise<MaintenanceState>;
  save: (choice: MaintenanceChoice, source: MaintenanceSource) => Promise<MaintenanceSaveResult>;
}) {
  const id = useId();
  const [page, setPage] = useState<MaintenanceState | null>(null);
  const [editing, setEditing] = useState(false);
  const [choice, setChoice] = useState<MaintenanceChoice>({ kind: "unassigned" });
  const [source, setSource] = useState<MaintenanceSource | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const mounted = useRef(false);
  const editButton = useRef<HTMLButtonElement>(null);
  const form = useRef<HTMLFormElement>(null);
  const wasEditing = useRef(false);
  useEffect(() => {
    if (wasEditing.current && !editing) editButton.current?.focus();
    if (editing) form.current?.querySelector<HTMLButtonElement>("button")?.focus();
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
        if (current) setError("Unable to load responsibility. Reopen Properties to retry.");
      });
    return () => {
      current = false;
      mounted.current = false;
    };
  }, [load]);
  async function begin() {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const next = await load();
      if (!mounted.current) return;
      setPage(next);
      if (!next.editable && !next.pending) {
        setError("This page is no longer available for responsibility changes.");
        return;
      }
      setSource(next.pending?.source ?? next.source);
      setChoice(next.pending?.maintenance ?? sourceChoice(next.source));
      setEditing(true);
    } catch {
      if (mounted.current) setError("Unable to load current responsibility. Try again.");
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!source || busy) return;
    setBusy(true);
    setError("");
    try {
      // The original pair matters even when target deletion leaves the stamp unchanged.
      page?.assertCurrentContext();
      const result = await save(choice, source);
      if (!mounted.current) return;
      if (result.disposition === "queued") {
        setNotice("Saved on this device. Responsibility is waiting for sync confirmation.");
        if (result.pending_error) {
          setError(result.pending_error);
          return;
        }
        try {
          const next = await load();
          if (mounted.current) setPage(next);
        } catch {
          if (mounted.current)
            setNotice("Saved on this device. Reopen Properties to inspect pending responsibility.");
        }
      } else {
        setNotice(
          result.disposition === "historical"
            ? "The earlier change was confirmed. Loading current responsibility."
            : "Responsibility saved.",
        );
        try {
          const next = await load();
          if (mounted.current) setPage(next);
        } catch {
          if (mounted.current)
            setNotice("Change confirmed. Reopen Properties to load current responsibility.");
        }
      }
      if (mounted.current) setEditing(false);
    } catch (reason) {
      if (mounted.current)
        setError(
          reason instanceof Error
            ? reason.message
            : "Unable to save responsibility. Your choice is retained.",
        );
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  const selected = JSON.stringify(choice);
  const options = page
    ? [
        { choice: { kind: "unassigned" } as MaintenanceChoice, label: "Unassigned" },
        ...page.targets.users.map((user) => ({
          choice: { kind: "user", user_id: user.id } as MaintenanceChoice,
          label: `Person: ${user.name}`,
        })),
        ...page.targets.groups.map((group) => ({
          choice: { kind: "group", group_id: group.id } as MaintenanceChoice,
          label: `Team: ${group.name}`,
        })),
      ]
    : [];
  if (page && !options.some((option) => JSON.stringify(option.choice) === selected))
    options.push({ choice, label: choiceLabel(choice, page) });
  return (
    <div className="border-border mt-3 border-t pt-3">
      <div className="flex min-w-0 items-center justify-between gap-2">
        <span className="text-text-muted text-xs">Responsible person or team</span>
        {!editing && (page?.editable || page?.pending) && (
          <Button
            ref={editButton}
            variant="ghost"
            size="sm"
            disabled={busy}
            onClick={() => void begin()}
          >
            {busy
              ? "Loading…"
              : page?.pending
                ? "Retry pending responsibility"
                : "Assign responsibility"}
          </Button>
        )}
      </div>
      {!editing && (
        <p className="text-text mt-1 text-sm break-words">
          {page ? choiceLabel(sourceChoice(page.source), page) : error ? "Unavailable" : "Loading…"}
        </p>
      )}
      {!editing && page?.pending && (
        <div role="status" className="text-text-muted mt-2 text-xs">
          <p>
            Pending responsibility: {choiceLabel(page.pending.maintenance, page)}. Sync confirmation
            is pending.
          </p>
          {page.pending.pending_error && (
            <p role="alert" className="text-danger mt-1">
              {page.pending.pending_error}
            </p>
          )}
        </div>
      )}
      {editing && (
        <form
          ref={form}
          onSubmit={(event) => void submit(event)}
          className="mt-2 grid gap-2"
          onKeyDown={(event) => {
            if (event.key === "Escape" && !busy) {
              event.preventDefault();
              setEditing(false);
              setError("");
            }
          }}
        >
          <label id={`${id}-label`} className="text-text-muted text-xs">
            Responsible person or team
          </label>
          <Select
            aria-labelledby={`${id}-label`}
            value={selected}
            disabled={busy}
            onChange={(event) => {
              const option = options.find(
                (candidate) => JSON.stringify(candidate.choice) === event.target.value,
              );
              if (option) setChoice(option.choice);
            }}
          >
            {options.map((option) => (
              <option key={JSON.stringify(option.choice)} value={JSON.stringify(option.choice)}>
                {option.label}
              </option>
            ))}
          </Select>
          <p className="text-text-muted text-xs">
            Responsibility does not grant access or change the draft owner.
          </p>
          {page?.catalogUnavailable && (
            <p className="text-text-muted text-xs">
              Targets are unavailable. Keep the existing assignment or choose Unassigned.
            </p>
          )}
          {page && !page.targets.groups_available && !page.catalogUnavailable && (
            <p className="text-text-muted text-xs">
              Team selection is unavailable with your current catalog access.
            </p>
          )}
          <div className="flex flex-wrap gap-2">
            <Button type="submit" size="sm" disabled={busy}>
              {busy ? "Saving…" : "Save responsibility"}
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={busy}
              onClick={() => {
                setEditing(false);
                setError("");
              }}
            >
              Cancel
            </Button>
          </div>
        </form>
      )}
      {notice && (
        <p role="status" className="text-text-muted mt-2 text-xs">
          {notice}
        </p>
      )}
      {error && (
        <p role="alert" className="text-danger mt-2 text-xs">
          {error}
        </p>
      )}
    </div>
  );
}
