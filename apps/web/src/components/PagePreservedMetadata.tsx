import { useContext, useEffect, useId, useRef, useState } from "react";
import type {
  ServerMovePreservedEvidence,
  ServerMovePreservedIndex,
} from "@starlane-notes/shared-types";
import { AuthContext } from "@/hooks/useAuth";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { capturePreservedMetadataContext, reviewedServerMoves } from "@/lib/reviewedServerMoves";
import { LocalPagePreservedMetadata } from "./LocalPagePreservedMetadata";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";

/** Defense in depth for direct renderer callers: select only the authorized projection. */
export function projectPreservedViewerGrant(value: unknown) {
  if (value === null) return null;
  if (!value || typeof value !== "object" || Array.isArray(value)) return {recipient:"Unavailable recipient"};
  const row = value as Record<string, unknown>;
  const recipient = row.recipient && typeof row.recipient === "object" && !Array.isArray(row.recipient)
    ? row.recipient as Record<string, unknown> : null;
  return {
    ...Object.fromEntries(["grant_id", "document_id", "space_id", "revision"].filter(k => typeof row[k] === "string").map(k => [k,row[k]])),
    role: row.role === "viewer" ? "viewer" : "unavailable",
    scope: row.scope === "page" ? "page" : "unavailable",
    recipient: recipient?.available === true && typeof recipient.display_name === "string"
      ? recipient.display_name : "Unavailable recipient",
  };
}
/** Historical values remain escaped text, never active properties or executable links. */
export function PreservedMetadataEvidence({ evidence }: { evidence: ServerMovePreservedEvidence }) {
  return (
    <div className="space-y-2">
      <p className="text-text-muted text-xs">
        Historical move metadata. Read-only; restoration is unavailable. Expected values describe
        the move outcome, not current page values.
      </p>
      {!evidence.entries.length && (
        <p className="text-text-muted text-sm">
          No preserved entries are available for this operation.
        </p>
      )}
      {evidence.entries.map((entry, index) => (
        <details
          key={`${entry.family}/${entry.source_id}/${index}`}
          className="border-border rounded-input border p-2"
        >
          <summary className="focus-ring cursor-pointer text-sm">
            {entry.family.replace(/_/g, " ")} — {entry.disposition}
          </summary>
          <dl className="mt-2 space-y-2 text-xs">
            <div>
              <dt className="font-medium">Original</dt>
              <dd>
                <pre className="text-text-muted [overflow-wrap:anywhere] break-words whitespace-pre-wrap">
                  {JSON.stringify(entry.family === "page_viewer_grant" ? projectPreservedViewerGrant(entry.original) : entry.original, null, 2)}
                </pre>
              </dd>
            </div>
            <div>
              <dt className="font-medium">Expected active at move time</dt>
              <dd>
                <pre className="text-text-muted [overflow-wrap:anywhere] break-words whitespace-pre-wrap">
                  {JSON.stringify(entry.family === "page_viewer_grant" ? projectPreservedViewerGrant(entry.expected_active) : entry.expected_active, null, 2)}
                </pre>
              </dd>
            </div>
          </dl>
        </details>
      ))}
    </div>
  );
}

export function PagePreservedMetadata(props: { spaceId: string; documentId: string }) {
  return getCurrentLaunchMode() !== "server"
    ? <LocalPagePreservedMetadata {...props} />
    : <ServerPagePreservedMetadata {...props} />;
}

function ServerPagePreservedMetadata({
  spaceId,
  documentId,
}: {
  spaceId: string;
  documentId: string;
}) {
  const accountId = useContext(AuthContext)?.user?.id ?? null;
  const local = getCurrentLaunchMode() !== "server";
  const current = useRef({ spaceId, documentId, accountId });
  current.current = { spaceId, documentId, accountId };
  const context = useRef<{ assertCurrent: () => void } | null>(null);
  const generation = useRef(0);
  const selection = useRef("");
  const [expanded, setExpanded] = useState(false);
  const [index, setIndex] = useState<ServerMovePreservedIndex | null>(null);
  const [selected, setSelected] = useState("");
  const [evidence, setEvidence] = useState<ServerMovePreservedEvidence | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const selectId = useId();
  useEffect(() => {
    generation.current++;
    context.current = null;
    selection.current = "";
    setExpanded(false);
    setIndex(null);
    setEvidence(null);
    setSelected("");
    setError("");
    setBusy(false);
  }, [spaceId, documentId, accountId, local]);
  useEffect(
    () => () => {
      generation.current++;
    },
    [],
  );

  function clearFailure(reason: unknown) {
    setIndex(null);
    setEvidence(null);
    setSelected("");
    selection.current = "";
    setError(
      reason instanceof Error
        ? reason.message
        : "Preserved metadata is unavailable. Inspect again.",
    );
  }
  function capture() {
    return capturePreservedMetadataContext(() => current.current);
  }
  async function loadIndex(more = false) {
    const sequence = ++generation.current;
    setBusy(true);
    setError("");
    setEvidence(null);
    setSelected("");
    selection.current = "";
    try {
      if (local) throw new Error("Server inspection is unavailable in local mode.");
      if (!more) {
        context.current?.assertCurrent();
        context.current ??= capture();
        setIndex(null);
      }
      const guard = context.current;
      if (!guard) throw new Error("Reopen preserved metadata.");
      guard.assertCurrent();
      const next = await reviewedServerMoves.preservedIndex(
        spaceId,
        documentId,
        more ? (index?.next_cursor ?? undefined) : undefined,
      );
      guard.assertCurrent();
      if (sequence !== generation.current) return;
      if (more && index) {
        const existing = new Set(index.operations.map((row) => row.operation_id));
        if (next.operations.some((row) => existing.has(row.operation_id)))
          throw new Error("Move history changed. Refresh the operation list.");
        setIndex({ ...next, operations: [...index.operations, ...next.operations] });
      } else setIndex(next);
    } catch (reason) {
      if (sequence === generation.current) clearFailure(reason);
    } finally {
      if (sequence === generation.current) setBusy(false);
    }
  }
  async function choose(operation: string) {
    const sequence = ++generation.current;
    selection.current = operation;
    setSelected(operation);
    setEvidence(null);
    setError("");
    setBusy(true);
    try {
      context.current?.assertCurrent();
      if (!context.current || !index?.operations.some((row) => row.operation_id === operation))
        throw new Error("Select an operation from a fresh list.");
      const next = await reviewedServerMoves.preservedEvidence(spaceId, documentId, operation);
      context.current.assertCurrent();
      if (sequence === generation.current && selection.current === operation) setEvidence(next);
    } catch (reason) {
      if (sequence === generation.current) clearFailure(reason);
    } finally {
      if (sequence === generation.current) setBusy(false);
    }
  }
  return (
    <section className="border-border mt-3 border-t pt-3" aria-label="Preserved metadata">
      <Button
        variant="ghost"
        size="sm"
        aria-expanded={expanded}
        onClick={() => {
          if (expanded) {
            generation.current++;
            context.current = null;
            selection.current = "";
            setBusy(false);
            setExpanded(false);
            setIndex(null);
            setSelected("");
            setEvidence(null);
            setError("");
          } else {
            setExpanded(true);
            void loadIndex();
          }
        }}
      >
        Preserved metadata
      </Button>
      {expanded && (
        <div className="mt-2 space-y-3">
          <p className="text-text-muted text-xs">
            Inspect evidence preserved by a past move. This does not restore metadata or enable
            moves.
          </p>
          {local ? (
            <p className="text-text-muted text-sm">
              Server inspection is unavailable in local mode.
            </p>
          ) : (
            <>
              <Button
                variant="secondary"
                size="sm"
                disabled={busy}
                onClick={() => void loadIndex()}
              >
                Refresh operations
              </Button>
              {busy && (
                <p role="status" className="text-text-muted text-sm">
                  Loading preserved metadata…
                </p>
              )}
              {error && (
                <p role="alert" className="text-destructive text-sm">
                  {error}
                </p>
              )}
              {index && (
                <>
                  <p className="text-text-muted text-xs">
                    Checked {index.checked_at}. Point-in-time inspection.
                  </p>
                  {!index.operations.length ? (
                    <p className="text-text-muted text-sm">
                      No preserved move metadata is available for this page.
                    </p>
                  ) : (
                    <>
                      <label htmlFor={selectId} className="text-text block text-xs">
                        Move operation
                      </label>
                      <Select
                        id={selectId}
                        value={selected}
                        disabled={busy}
                        onChange={(event) => {
                          if (event.target.value) void choose(event.target.value);
                        }}
                      >
                        <option value="">Select an operation</option>
                        {index.operations.map((row) => (
                          <option key={row.operation_id} value={row.operation_id}>
                            {`${row.created_at} — ${row.operation_id}`}
                          </option>
                        ))}
                      </Select>
                      {index.next_cursor ? (
                        <Button
                          variant="secondary"
                          size="sm"
                          disabled={busy}
                          onClick={() => void loadIndex(true)}
                        >
                          Load more operations
                        </Button>
                      ) : (
                        <p className="text-text-muted text-xs">
                          End of currently available operations.
                        </p>
                      )}
                    </>
                  )}
                </>
              )}
              {evidence && <PreservedMetadataEvidence evidence={evidence} />}
            </>
          )}
        </div>
      )}
    </section>
  );
}
