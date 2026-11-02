import { PageViewerControls } from "@/components/PageViewerControls";
import { PageRestrictionControls } from "@/components/PageRestrictionControls";
import { useEffect, useId, useRef, useState } from "react";
import { dataClient } from "@/lib/dataClient";
import { capturePageMetadataLoadGuard } from "@/lib/pageMetadataLoad";
import { getApiBase } from "@/lib/api";
import { buildPageShareLink, capturePageShareTargetGuard } from "@/lib/pageAccessInspection";
import type { PageAccessInspection } from "@/lib/pageAccessInspection";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogCloseAction,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";

const sourceLabels: Record<
  PageAccessInspection["space_permission_sources"][number]["kind"],
  string
> = {
  direct_space_role: "Direct space membership",
  space_group_role: "Space group membership",
  global_group_role: "Global group membership",
  internal_space_fallback: "Internal space access",
  public_space_fallback: "Public space access",
  instance_admin: "Instance administrator access",
};
const linkReasons: Record<NonNullable<PageAccessInspection["public_link"]["reason"]>, string> = {
  not_enabled: "A public link is not enabled.",
  deployment_disabled: "Public links are disabled on this server.",
  unpublished: "The page has no published version for anonymous viewing.",
  restricted: "Page or ancestor restrictions prevent anonymous viewing.",
  missing_link: "The configured public link is unavailable.",
};
const draftLabels: Record<PageAccessInspection["selected_private_draft"]["scope"], string> = {
  none: "No accessible private draft reported.",
  owned: "An accessible draft: owned by you",
  shared_viewer: "An accessible draft: shared with you as a viewer",
  shared_editor: "An accessible draft: shared with you as an editor",
};

/** All capability text comes from the checked page projection, never its inherited source list. */
export function PageAccessReport({ report }: { report: PageAccessInspection }) {
  const actions: [keyof PageAccessInspection["capabilities"], string][] = [
    ["view", "View page"],
    ["comment", "Comment"],
    ["edit", "Edit content"],
    ["move", "Move page"],
    ["manage_lock", "Manage page lock"],
    ["enable_public_link", "Enable public link"],
    ["disable_public_link", "Disable public link"],
  ];
  return (
    <div className="space-y-5">
      <section aria-labelledby="share-your-access">
        <h3 id="share-your-access" className="text-text text-sm font-semibold">
          Your access
        </h3>
        <dl className="mt-2 space-y-1 text-sm">
          {actions.map(([key, label]) => (
            <div key={key} className="flex justify-between gap-4">
              <dt>{label}</dt>
              <dd className="text-text-muted">
                {report.capabilities[key] ? "Allowed" : "Not allowed"}
              </dd>
            </div>
          ))}
        </dl>
        {Boolean(report.page_permission_sources?.length) && <p className="text-text-muted mt-3 text-sm">Direct published-page viewer grant. This contributes page viewing only, not space membership or editing.</p>}
        <p className="text-text-muted mt-3 text-sm">Inherited space contributions</p>
        {report.space_permission_sources.length ? (
          <ul className="text-text-muted mt-1 space-y-1 text-sm">
            {Array.from(
              new Set(report.space_permission_sources.map((source) => sourceLabels[source.kind])),
            ).map((label) => (
              <li key={label}>{label}</li>
            ))}
          </ul>
        ) : (
          <p className="text-text-muted mt-1 text-sm">
            No space permission contributions reported.
          </p>
        )}
        <p className="text-text-muted mt-3 text-sm">
          {report.restrictions.local === "restricted"
            ? "This page has an access restriction."
            : "This page uses inherited access."}{" "}
          {report.restrictions.ancestor_restricted && "An ancestor also restricts access."}
        </p>
      </section>
      <section aria-labelledby="share-public-link">
        <h3 id="share-public-link" className="text-text text-sm font-semibold">
          Public link
        </h3>
        <p className="text-text-muted mt-2 text-sm">
          {report.public_link.configured
            ? "A public link is configured."
            : "No public link is configured."}{" "}
          {report.public_link.reason
            ? linkReasons[report.public_link.reason]
            : report.public_link.policy_allows_anonymous
              ? "Access policy permits anonymous viewing; published content availability is checked separately."
              : "Access policy does not permit anonymous viewing."}
        </p>
      </section>
      <section aria-labelledby="share-draft-access">
        <h3 id="share-draft-access" className="text-text text-sm font-semibold">
          Private draft access
        </h3>
        <p className="text-text-muted mt-2 text-sm">
          {draftLabels[report.selected_private_draft.scope]}
        </p>
        {report.selected_private_draft.scope !== "none" && (
          <p className="text-text-muted mt-1 text-sm">
            View: {report.selected_private_draft.view ? "allowed" : "not allowed"}. Edit:{" "}
            {report.selected_private_draft.edit ? "allowed" : "not allowed"}.
          </p>
        )}
        <p className="text-text-muted mt-2 text-sm">
          This check selects an available draft; it does not describe the draft currently open in
          the editor. Draft sharing is separate from page access and public links.
        </p>
      </section>
      <p className="text-text-muted text-sm">
        {report.sharing.guests_available
          ? "Direct page viewers are managed separately below. Guest accounts use explicit page access; this workflow does not send invitations."
          : "Direct page viewers are managed separately below; guest invitations are not available yet."}
      </p>
      <p className="text-text-muted text-xs">
        Checked {new Date(report.checked_at).toLocaleString()}. This is a point-in-time report;
        access can change.
      </p>
    </div>
  );
}

export function PageShareDialog({
  spaceId,
  pageId,
  accountId,
  localMode,
  onClose,
}: {
  spaceId: string;
  pageId: string;
  accountId: string | null;
  localMode: boolean;
  onClose: () => void;
}) {
  const titleId = useId();
  const descriptionId = useId();
  const [assertCurrent] = useState(() => capturePageMetadataLoadGuard());
  const originalAccount = useRef(accountId);
  const originalLocalMode = useRef(localMode);
  const targetRef = useRef({ spaceId, pageId });
  targetRef.current = { spaceId, pageId };
  const originalTarget = useRef(targetRef.current);
  const [assertTarget] = useState(() => capturePageShareTargetGuard(() => targetRef.current));
  const accountRef = useRef(accountId);
  accountRef.current = accountId;
  const sequence = useRef(0);
  const mounted = useRef(false);
  const [report, setReport] = useState<PageAccessInspection | null>(null);
  const [busy, setBusy] = useState(!localMode);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [accessRefreshVersion, setAccessRefreshVersion] = useState(0);
  const [restrictionMenuOpen, setRestrictionMenuOpen] = useState(false);
  const [viewerMenuOpen, setViewerMenuOpen] = useState(false);
  function requireOriginalContext() {
    assertCurrent();
    assertTarget();
    if (accountRef.current !== originalAccount.current)
      throw new Error("Account changed. Reopen Share page in the current account.");
  }
  function fail(reason: unknown) {
    setReport(null);
    setNotice("");
    setError(
      reason instanceof Error ? reason.message : "Unable to check access. Refresh to retry.",
    );
  }
  async function refresh() {
    setAccessRefreshVersion((version) => version + 1);
    const request = ++sequence.current;
    setReport(null);
    setNotice("");
    setError("");
    setBusy(true);
    try {
      requireOriginalContext();
      const next = await dataClient.inspectDocumentAccess(spaceId, pageId);
      requireOriginalContext();
      if (mounted.current && request === sequence.current) setReport(next);
    } catch (reason) {
      if (mounted.current && request === sequence.current) fail(reason);
    } finally {
      if (mounted.current && request === sequence.current) setBusy(false);
    }
  }
  useEffect(() => {
    mounted.current = true;
    if (!localMode) void refresh();
    return () => {
      mounted.current = false;
      sequence.current++;
    };
    // The dialog's original context intentionally remains captured until closed and reopened.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    if (
      accountId !== originalAccount.current ||
      localMode !== originalLocalMode.current ||
      spaceId !== originalTarget.current.spaceId ||
      pageId !== originalTarget.current.pageId
    ) {
      sequence.current++;
      setBusy(false);
      fail(new Error("Account or page changed. Reopen Share page in the current account."));
    }
  }, [accountId, localMode, spaceId, pageId]);
  async function copyPageLink() {
    setNotice("");
    try {
      requireOriginalContext();
      await navigator.clipboard.writeText(
        buildPageShareLink(spaceId, pageId, {
          native: "__TAURI_INTERNALS__" in window,
          apiBase: getApiBase(),
          webOrigin: window.location.origin,
        }),
      );
      requireOriginalContext();
      if (mounted.current) setNotice("Page link copied. Copying a link does not grant access.");
    } catch (reason) {
      if (mounted.current) fail(reason);
    }
  }
  return (
    <DialogShell labelledBy={titleId} describedBy={descriptionId} maxWidth="lg" onClose={onClose} closeOnEscape={!restrictionMenuOpen && !viewerMenuOpen}>
      <DialogHeader>
        <div>
          <DialogTitle id={titleId}>Share page</DialogTitle>
          <DialogDescription id={descriptionId}>
            Check your access and sharing status.
          </DialogDescription>
        </div>
        <DialogCloseAction onClick={onClose} />
      </DialogHeader>
      <DialogBody className="min-h-0 overflow-y-auto">
        {localMode ? (
          <p className="text-text-muted text-sm">
            Access inspection is available when connected to a server. Local pages do not have
            server sharing permissions.
          </p>
        ) : (
          <>
            {busy && (
              <p role="status" className="text-text-muted text-sm">
                Checking current access…
              </p>
            )}
            {error && (
              <p role="alert" className="text-danger text-sm">
                {error}
              </p>
            )}
            {report && <PageAccessReport report={report} />}
            {report && <PageViewerControls key={`${accountId ?? ""}/${spaceId}/${pageId}`}
              spaceId={spaceId} pageId={pageId} contextKey={`${accountId ?? ""}/${getApiBase()}`}
              verifyCurrent={requireOriginalContext} onMenuOpenChange={setViewerMenuOpen} guestsAvailable={report.sharing.guests_available} />}
            <PageRestrictionControls
              spaceId={spaceId} pageId={pageId}
              contextKey={`${accountId ?? ""}/${localMode}`}
              accessRefreshVersion={accessRefreshVersion}
              assertContext={requireOriginalContext}
              onMenuOpenChange={setRestrictionMenuOpen}
              onConfirmed={() => { setReport(null); setNotice(""); void refresh(); }}
            />
          </>
        )}
        {notice && (
          <p role="status" className="text-text-muted mt-3 text-sm">
            {notice}
          </p>
        )}
      </DialogBody>
      <DialogFooter>
        {!localMode && (
          <>
            <Button variant="secondary" disabled={busy} onClick={() => void refresh()}>
              Refresh access
            </Button>
            <Button disabled={busy || !report} onClick={() => void copyPageLink()}>
              Copy page link
            </Button>
          </>
        )}
      </DialogFooter>
    </DialogShell>
  );
}
