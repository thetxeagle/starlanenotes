import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { ApiError } from "@/lib/api";
import {
  SnapshotRecoveryRequests,
  BacklinksErrorState,
  formatBacklinkCount,
  formatAiDocumentContext,
  getBacklinksErrorKind,
  isRightPanelTabAvailable,
  LocalVersionHistoryNotice,
  ServerVersionHistoryNotice,
  RIGHT_PANEL_NAVIGATION_TABS,
  RIGHT_PANEL_SCROLL_CONTENT_CLASS,
} from "./RightPanel";

describe("RightPanel tab availability", () => {
  it("labels backlink counts as source-page relationships", () => {
    expect(formatBacklinkCount(1)).toBe("1 page links here");
    expect(formatBacklinkCount(3)).toBe("3 pages link here");
  });

  it("keeps tab content vertically scrollable inside bounded mobile and desktop panels", () => {
    expect(RIGHT_PANEL_SCROLL_CONTENT_CLASS).toContain("flex-1");
    expect(RIGHT_PANEL_SCROLL_CONTENT_CLASS).toContain("overflow-y-auto");
  });

  it("hides server-only panels in local mode", () => {
    expect(isRightPanelTabAvailable("properties", "local")).toBe(true);
    expect(isRightPanelTabAvailable("backlinks", "local")).toBe(true);
    expect(isRightPanelTabAvailable("history", "local")).toBe(true);
    expect(isRightPanelTabAvailable("comments", "local")).toBe(false);
    expect(isRightPanelTabAvailable("ai", "local")).toBe(false);
  });

  it("keeps comments and AI available in server mode", () => {
    expect(isRightPanelTabAvailable("comments", "server")).toBe(true);
    expect(isRightPanelTabAvailable("ai", "server")).toBe(true);
  });

  it("routes a selected draft to draft discussion without replacing page comments", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/RightPanel.tsx"),
      "utf8",
    );

    expect(source).toContain("pageDetailsControls?.draftDiscussion ? (");
    expect(source).toContain("<DraftDiscussionPanel");
    expect(source).toContain("<CommentsPanel");
    expect(source).toContain('"Draft discussion" : "Page comments"');
  });

  it("keeps contents and comments out of properties-panel navigation", () => {
    expect(RIGHT_PANEL_NAVIGATION_TABS).toEqual(["properties", "backlinks", "history", "ai"]);
    expect(RIGHT_PANEL_NAVIGATION_TABS).not.toContain("comments");
  });

  it("passes registered active-draft cover controls into Page Details", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/RightPanel.tsx"),
      "utf8",
    );

    expect(source).toContain(
      "coverImage={pageDetailsControls?.coverImage ?? resolvedDoc.cover_image}",
    );
    expect(source).toContain("coverEditable={pageDetailsControls?.coverEditable ?? false}");
    expect(source).toContain("onCoverUpload={pageDetailsControls?.onCoverUpload}");
    expect(source).toContain("onCoverUpdate={pageDetailsControls?.onCoverUpdate}");
    expect(source).toContain("pageWidth={pageDetailsControls?.pageWidth}");
    expect(source).toContain("onPageWidthChange={pageDetailsControls?.onPageWidthChange}");
    expect(source).toContain(
      "pageValuesEditable={pageDetailsControls?.pageValuesEditable ?? false}",
    );
    expect(source).toContain("schemaEditable={pageDetailsControls?.schemaEditable ?? false}");
    expect(source).toContain(
      "contentMetadataEditable={pageDetailsControls?.contentMetadataEditable ?? false}",
    );
    expect(source).toContain("synopsis={pageDetailsControls?.synopsis}");
    expect(source).toContain("pageCssSource={pageDetailsControls?.pageCssSource}");
    expect(source).toContain("onSynopsisUpdate={pageDetailsControls?.onSynopsisUpdate}");
  });

  it("distinguishes backlink loading, empty, error, and linked-page states", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/RightPanel.tsx"),
      "utf8",
    );

    expect(source).toContain('controlledActiveTab === "backlinks" && Boolean(spaceId && docId)');
    expect(source).toContain('tab.id === "backlinks" && activeTab !== "backlinks"');
    expect(source).toContain("aria-busy={backlinksLoading}");
    expect(source).toContain('role="status"');
    expect(source).toContain("Finding linked pages…");
    expect(source).toContain("No pages link here yet");
    expect(source).toContain('aria-label="Pages linking here"');
    expect(source).toContain("formatBacklinkCount(backlinks.length)");
  });
});

describe("RightPanel backlink errors", () => {
  it("maps a 403 to a permission state while keeping other failures retryable", () => {
    expect(getBacklinksErrorKind(new ApiError("Forbidden", 403))).toBe("permission");
    expect(getBacklinksErrorKind(new ApiError("Unavailable", 503))).toBe("unavailable");
    expect(getBacklinksErrorKind(new Error("Network unavailable"))).toBe("unavailable");
  });

  it("does not offer retry for permission denial, but does for transient failures", () => {
    const onRetry = () => {};
    const permissionMarkup = renderToStaticMarkup(
      <BacklinksErrorState kind="permission" onRetry={onRetry} />,
    );
    const unavailableMarkup = renderToStaticMarkup(
      <BacklinksErrorState kind="unavailable" onRetry={onRetry} />,
    );

    expect(permissionMarkup).toContain("You don’t have permission to view backlinks");
    expect(permissionMarkup).toContain("Ask a space owner to review your access.");
    expect(permissionMarkup).not.toContain("Retry");
    expect(unavailableMarkup).toContain("Unable to load backlinks.");
    expect(unavailableMarkup).toContain("Retry");
  });
});

describe("RightPanel local history", () => {
  it("states version history is unavailable and keeps workflow-event guidance distinct", () => {
    const emptyMarkup = renderToStaticMarkup(<LocalVersionHistoryNotice />);
    const populatedMarkup = renderToStaticMarkup(
      <LocalVersionHistoryNotice workflowEventsAvailable />,
    );

    expect(emptyMarkup).toContain(
      "Page version history and restore aren’t available in local vaults.",
    );
    expect(emptyMarkup).toContain("Workflow events will appear here when recorded.");
    expect(populatedMarkup).toContain(
      "Page version history and restore aren’t available in local vaults.",
    );
    expect(populatedMarkup).not.toContain("Workflow events will appear here when recorded.");
  });

  it("states server records do not support page-content restoration", () => {
    const markup = renderToStaticMarkup(<ServerVersionHistoryNotice />);
    expect(markup).toContain("Version records are available here");
    expect(markup).toContain("restoring page content is not supported yet");
  });
});

describe("AI document context", () => {
  it("includes a title and bounds oversized document content", () => {
    expect(formatAiDocumentContext("Project brief", "Body")).toBe("# Project brief\n\nBody");
    const context = formatAiDocumentContext("", "x".repeat(24_001));
    expect(context).toContain("# Untitled document");
    expect(context).toContain("[Document context truncated]");
    expect(context).toHaveLength(
      24_000 + "# Untitled document\n\n".length + "\n\n[Document context truncated]".length,
    );
  });
});

describe("RightPanel history recovery limits", () => {
  it("does not offer content restore for version records", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/layout/RightPanel.tsx"),
      "utf8",
    );

    expect(source).not.toContain("restoreDocumentVersion");
    expect(source).not.toContain(">Restore</button>");
    expect(source).toContain("<ServerVersionHistoryNotice />");
  });
});


describe("retained snapshot recovery controls", () => {
  const intent = {operation_id:"original-uuid",space_id:"space / route",document_id:"page?route",source:{draft:{title:"Saved source"}}} as import("@/lib/publicationSnapshotRecovery").SnapshotRecoveryIntent;
  const render = (state:import("@/lib/publicationSnapshotRecovery").SnapshotRecoveryRecord["state"]) => renderToStaticMarkup(<SnapshotRecoveryRequests records={[{schema_version:1,intent,state,...(state === "confirmed" ? {receipt:{draft:{id:"target / id"}} as import("@/lib/publicationSnapshotRecovery").SnapshotRecoveryReceipt} : {})}]} busy={false} onReview={() => {}} onCancel={() => {}} onRetry={() => {}} />);
  it("offers cancellation only for an undispatched prepared original review", () => {
    expect(render("prepared")).toContain("Review original request");
    expect(render("prepared")).toContain("Cancel prepared request");
    for (const state of ["unknown","dispatched","confirmed","rejected"] as const) expect(render(state)).not.toContain("Cancel prepared request");
  });
  it("offers original retry only for held dispatched outcomes, without replacement-copy controls", () => {
    for (const state of ["unknown","dispatched"] as const) { expect(render(state)).toContain("Retry original recovery"); expect(render(state)).toContain("No replacement copy or publication"); }
    expect(render("rejected")).toContain("No copy was created");
    expect(render("rejected")).not.toContain("Retry original recovery");
  });
  it("uses an escaped canonical new-tab target without activating the current editor", () => {
    const html = render("confirmed");
    expect(html).toContain('href="/spaces/space%20%2F%20route/docs/page%3Froute?draft=target%20%2F%20id"');
    expect(html).toContain('target="_blank"'); expect(html).toContain('rel="noopener noreferrer"');
    expect(html).toContain("Open recovered draft in a new tab");
  });
});
