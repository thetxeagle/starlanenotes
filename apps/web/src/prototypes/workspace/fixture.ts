/**
 * Fictional, explicitly labeled content for the isolated document-workspace prototype.
 * It describes a plausible offline-first architecture without claiming that any live system,
 * workspace, sync process, permission, or collaborator exists.
 */
export const WORKSPACE_FIXTURE_CONTENT = `> [!NOTE]
> Prototype fixture: the service names, thresholds, examples, and operational procedures below are illustrative. They do not report live system state.

## Purpose and operating model

An offline-capable workspace should make the local copy useful before a network is available. The client reads a committed snapshot, records user edits as portable document changes, and later reconciles those changes with the configured service. A document remains readable throughout that cycle; transport health is an application concern, not a property the prose should pretend to measure.

This guide describes a fictional reference design for a technical-notes workspace. It separates the durable document format from synchronization, names the boundaries where conflicts can occur, and gives operators a repeatable recovery procedure. The example assumes a single writer per local transaction and supports concurrent edits through an operation-based merge layer.

The design has four goals:

- Keep documents readable and exportable as Markdown with stable references.
- Make each local save durable before scheduling network work.
- Merge independent edits without silently discarding either author's text.
- Explain unresolved conflicts in terms a person can inspect and repair.

## System boundaries

The workspace is divided into a local content store, a change journal, a transport adapter, and a remote document service. Each boundary has a narrow responsibility. The local store owns the latest materialized document; the journal records pending operations; the adapter handles retries and authentication; the service validates and persists accepted updates.

A client should not infer that an operation reached the remote service merely because it entered the journal. The user-facing product can expose a real queue state when it has current evidence, while this static prototype fixture makes no such claim. Likewise, a local conflict marker is not proof that another person is currently editing the page.

### Local write path

The editor first serializes the current document into the canonical Markdown representation and validates the result. It then commits the content and a monotonically increasing local revision in one database transaction. Only after that transaction succeeds does it append an outbound operation to the journal.

If the database transaction fails, the application keeps the in-memory buffer and presents an actionable save error. It must not add a journal entry for content that was never durably stored. On restart, the client can replay committed journal entries in order and rebuild its pending queue.

### Remote reconciliation

For each queued operation, the adapter sends the document identifier, the base revision, and the operation payload. The service either accepts the operation and returns a new revision, reports that the base revision is stale, or rejects the request for a defined validation or authorization reason. The adapter records the response before advancing to the next dependent operation.

A stale-base response enters reconciliation. The client fetches the latest remote snapshot, applies merge rules to the local operation, and retries with the returned revision. Repeated transport failures use bounded exponential backoff with jitter; permanent validation failures remain visible for repair instead of retrying indefinitely.

## Commit and recovery sequence

A reliable write path has explicit checkpoints. Keeping them separate makes crash recovery easier to reason about and prevents an optimistic UI update from being mistaken for durable data.

1. Normalize editor state and validate portable block metadata.
2. Commit Markdown and the next local revision atomically.
3. Append a journal record that points to the committed revision.
4. Attempt remote delivery when the configured transport is available.
5. Persist the acknowledgement or reconciliation requirement.
6. Compact acknowledged journal records only after the local snapshot is verified.

A recovery routine should be safe to run more than once. Each journal operation therefore carries a stable operation identifier, and the server treats duplicate delivery of the same identifier as idempotent. The client can retry after a timeout without guessing whether the original request was accepted.

> [!TIP]
> Treat “saved on this device” and “accepted by the remote service” as separate facts. Their labels should come from the corresponding durable evidence.

## Document identity and portable links

A page title is useful to people but is not a durable key. Titles change, duplicate titles are normal in different folders, and a rename should not break references. Internal links therefore pair a readable label with the stable document identifier:

[[Retry and reconciliation|doc-sync-retry-reconciliation-7c2a]]

The label can be refreshed when a destination is renamed, but the identifier remains the routing key. When a destination is missing or outside the current workspace, preserve the original source text and show an unresolved link state. Do not rewrite it as an external URL or silently substitute another page with the same title.

For links created from a search result, retain enough context to disambiguate identical names: parent folder, page icon where present, and a short location hint. Link activation should remain an ordinary keyboard-accessible navigation action with a clear focus destination.

### Stable identifiers in practice

Use identifiers that are generated once and remain immutable for the lifetime of the page. Do not derive them from a title slug. An imported document may preserve an existing identifier when trusted metadata provides one; otherwise, create a new identifier and keep the original title and source path as import metadata.

A link resolver should distinguish three outcomes: resolved to a document in the active space, unresolved because no matching identifier is available, and unavailable because the current user or client cannot access the target. These states should not be collapsed into one generic “broken link” label, and the resolver must not reveal whether an inaccessible target exists.

## Merge behavior

Concurrent changes are easiest to preserve when the merge layer operates on stable operations instead of replacing whole-document strings. Text insertions and deletions can be integrated when their positions are transformed against intervening edits. Structural changes such as moving a block or changing a heading level need explicit operation semantics and deterministic ordering.

For example, two writers can independently edit separate paragraphs without conflict. If both replace the same sentence, the merged result should preserve both alternatives or surface a focused conflict view. A “last write wins” rule may be useful for disposable cache metadata, but it is not an acceptable default for authored prose.

### Conflict repair

A conflict record should retain the base value, the local value, and the remote value. The repair surface should show the affected block and let a person choose, combine, or defer the alternatives. Applying a resolution creates a new operation against the latest known revision; it must not erase the source alternatives before the new write is durable.

History is useful only when its boundaries are clear. A user should be able to tell whether a snapshot was created locally, accepted remotely, or restored from an export. The interface should avoid decorative claims about collaborators, permissions, or network health when the product has no current evidence for them.

## Architecture map

The diagram summarizes the data path and the recovery boundary. Arrows describe the fictional flow in this reference design, not a live topology.

![Illustrative offline workspace data flow](/workspace-prototype-architecture.svg)

The local transaction is the durability boundary. The journal is the retry boundary. The remote acknowledgement is the point at which the client may mark that particular operation as accepted. A later edit creates a new local revision and a new operation; it does not mutate the meaning of an earlier acknowledgement.

## Example retry policy

The following configuration is illustrative. A production system should derive these limits from its service contract, user expectations, and operational capacity.

\`\`\`text
journal:
  max_attempts: 8
  base_delay_ms: 750
  max_delay_ms: 60000
  jitter_ratio: 0.20
  retryable_responses:
    - network_timeout
    - service_unavailable
    - stale_base_revision
  stop_and_surface:
    - invalid_document
    - access_denied
    - unsupported_format
\`\`\`

The client should calculate each delay from the attempt number, cap it at the configured maximum, and add bounded jitter so many clients do not retry in lockstep. A stale revision is retryable only after reconciliation; sending the same stale payload repeatedly cannot make progress.

### Retry decision table

| Result | Client action | User-facing meaning |
| --- | --- | --- |
| Timeout or temporary service failure | Retry with bounded backoff | The local copy remains available; remote acceptance is unknown or pending |
| Stale base revision | Fetch, merge, then retry | A newer remote revision requires reconciliation |
| Invalid document payload | Stop and retain the local draft | The operation needs a format or content repair |
| Authorization rejection | Stop and refresh the actual session context | The service declined this operation; do not infer broader access rules |
| Accepted operation | Store acknowledgement and continue | This operation was accepted at the returned revision |

A timeout after request submission is ambiguous: the server may have committed the operation before the response was lost. Stable operation identifiers make a retry safe and let the service return the original result instead of applying the change twice.

## Operational checklist

When investigating a report that an edit did not appear on another device, start with the document itself and collect evidence in order. Avoid asking a person to repeat a destructive import or overwrite a local copy just to see whether the problem happens again.

- Confirm the page identifier and the local revision that contains the expected edit.
- Check whether a journal operation references that revision and has a stable operation ID.
- Inspect the most recent transport result and whether it was retryable or terminal.
- Compare the accepted remote revision with the snapshot visible to the second client.
- Export a copy of both divergent versions before attempting manual conflict repair.

If the local revision is absent, investigate editor serialization and local transaction errors. If the journal entry exists without an acknowledgement, investigate the transport and server response. If the service acknowledged the operation but a second client lacks it, investigate snapshot refresh, cache invalidation, and revision selection.

## Two-phase repair example

The columns below contrast the first diagnostic pass with a safe recovery pass. They are ordinary document content using the existing portable columns serialization.

<!-- starlane:columns layout=two-equal -->

### Diagnose

1. Record the document ID and local revision.
2. Export the local Markdown before changing it.
3. Inspect the journal record and last response.
4. Compare against a fresh remote snapshot.

<!-- starlane:column index=2 -->

### Recover

1. Preserve the divergent local and remote versions.
2. Reconcile only the affected blocks.
3. Save the resolution as a new operation.
4. Verify the returned revision before cleanup.


Keep the order of evidence intact. A successful local commit proves local durability; it does not prove remote delivery. An acknowledgement proves acceptance of one operation; it does not prove that another client has refreshed its view.

When the root cause is still unknown, leave the journal entry and exports in place. Premature cleanup removes the evidence needed to distinguish a serialization defect from a transport failure or a stale reader cache.

<!-- /starlane:columns -->

## Design and review notes

The reference flow is intentionally conservative: local persistence comes first, outbound delivery is idempotent, and conflict repair creates a new operation that can be audited. The same rules apply whether the configured service is reachable on a local network, over a VPN, or not reachable at all during a writing session.

The document format remains portable. Markdown carries headings, prose, lists, tables, code fences, callouts, internal links, and the explicit column boundary. Binary assets use normal asset references. Transport credentials, queue telemetry, and access decisions do not belong in the document body.

This page is sample content for evaluating reading width, heading navigation, the persistent editing toolbar, block insertion, and supporting panels. All operational details are fictional and should be replaced with verified product behavior before publication.
`;

export interface WorkspaceFixtureHeading {
  id: string;
  title: string;
  level: 2 | 3;
}

export const WORKSPACE_FIXTURE_HEADINGS: WorkspaceFixtureHeading[] = [
  { id: "purpose-and-operating-model", title: "Purpose and operating model", level: 2 },
  { id: "system-boundaries", title: "System boundaries", level: 2 },
  { id: "local-write-path", title: "Local write path", level: 3 },
  { id: "remote-reconciliation", title: "Remote reconciliation", level: 3 },
  { id: "commit-and-recovery-sequence", title: "Commit and recovery sequence", level: 2 },
  {
    id: "document-identity-and-portable-links",
    title: "Document identity and portable links",
    level: 2,
  },
  { id: "stable-identifiers-in-practice", title: "Stable identifiers in practice", level: 3 },
  { id: "merge-behavior", title: "Merge behavior", level: 2 },
  { id: "conflict-repair", title: "Conflict repair", level: 3 },
  { id: "architecture-map", title: "Architecture map", level: 2 },
  { id: "example-retry-policy", title: "Example retry policy", level: 2 },
  { id: "retry-decision-table", title: "Retry decision table", level: 3 },
  { id: "operational-checklist", title: "Operational checklist", level: 2 },
  { id: "two-phase-repair-example", title: "Two-phase repair example", level: 2 },
  { id: "diagnose", title: "Diagnose", level: 3 },
  { id: "recover", title: "Recover", level: 3 },
  { id: "design-and-review-notes", title: "Design and review notes", level: 2 },
];

/** Populated example hierarchy for shell navigation; every item is fixture data. */
export const WORKSPACE_FIXTURE_TREE = [
  {
    type: "folder",
    title: "Platform Operations",
    children: [
      {
        type: "folder",
        title: "Offline Workspace and Data Lifecycle",
        children: [
          { type: "page", title: "Operating the offline workspace", selected: true },
          {
            type: "page",
            title: "Retry and reconciliation",
            id: "doc-sync-retry-reconciliation-7c2a",
          },
          { type: "page", title: "Local snapshot verification and export recovery" },
        ],
      },
      { type: "page", title: "Release readiness and rollback procedures" },
    ],
  },
  {
    type: "folder",
    title: "Engineering Handbook",
    children: [
      { type: "page", title: "Portable document format decisions" },
      { type: "page", title: "Service boundary and ownership map" },
    ],
  },
] as const;
