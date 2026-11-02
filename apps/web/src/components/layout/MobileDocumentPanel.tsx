import { useRef } from "react";
import { X } from "lucide-react";
import { useOverlayFocusLifecycle } from "@/components/ui/useOverlayFocusLifecycle";
import { RightPanel, type RightPanelTab } from "./RightPanel";
import type { DocumentPageDetailsControls } from "./documentPageDetails";

interface MobileDocumentPanelProps {
  open: boolean;
  onClose: () => void;
  spaceId?: string;
  docId?: string;
  activeTab: RightPanelTab;
  selectedCommentId?: string | null;
  commentPanelScope?: "all" | "inline";
  onTabChange: (tab: RightPanelTab) => void;
  pageDetailsControls?: DocumentPageDetailsControls | null;
}

export const MOBILE_DOCUMENT_PANEL_CLASS =
  "border-border bg-surface shadow-card fixed inset-x-0 bottom-0 z-40 flex h-[min(42rem,calc(100dvh-1rem))] flex-col overflow-hidden rounded-t-card border border-b-0 pb-[env(safe-area-inset-bottom)]";
export const MOBILE_DOCUMENT_PANEL_BODY_CLASS = "min-h-0 flex-1 overflow-hidden";

export function getMobileDocumentPanelLabel(
  activeTab: RightPanelTab,
  draftDiscussion = false,
): string {
  if (activeTab === "comments") return draftDiscussion ? "Draft discussion" : "Comments panel";
  if (activeTab === "properties") return "Properties panel";
  return "Document panel";
}

/** Shared mobile-web/native owner for the contextual document properties panel. */
export function MobileDocumentPanel({
  open,
  onClose,
  spaceId,
  docId,
  activeTab,
  selectedCommentId,
  commentPanelScope,
  onTabChange,
  pageDetailsControls,
}: MobileDocumentPanelProps) {
  const panelRef = useRef<HTMLElement | null>(null);
  const closeAndRestoreFocus = useOverlayFocusLifecycle({
    open,
    onClose,
    containerRef: panelRef,
  });

  if (!open) return null;

  const panelLabel = getMobileDocumentPanelLabel(
    activeTab,
    Boolean(pageDetailsControls?.draftDiscussion),
  );
  const closeLabel = `Close ${panelLabel.toLowerCase()}`;

  return (
    <>
      <button
        type="button"
        className="fixed inset-0 z-30 bg-black/40"
        aria-label={closeLabel}
        onClick={closeAndRestoreFocus}
      />
      <aside
        ref={panelRef}
        className={MOBILE_DOCUMENT_PANEL_CLASS}
        role="dialog"
        aria-modal="true"
        aria-labelledby="mobile-document-panel-title"
        tabIndex={-1}
      >
        {activeTab === "comments" && (
          <h2 id="mobile-document-panel-title" className="sr-only">
            {panelLabel}
          </h2>
        )}
        {activeTab !== "comments" && (
          <div className="border-border flex shrink-0 items-center justify-between border-b px-3 py-2">
            <h2 id="mobile-document-panel-title" className="text-text text-sm font-medium">
              {panelLabel}
            </h2>
            <button
              type="button"
              onClick={closeAndRestoreFocus}
              className="rounded-button text-text-muted active:bg-surface-raised hover:text-text flex h-11 w-11 items-center justify-center transition-colors duration-100"
              aria-label={closeLabel}
            >
              <X size={16} />
            </button>
          </div>
        )}
        <div className={MOBILE_DOCUMENT_PANEL_BODY_CLASS}>
          <RightPanel
            spaceId={spaceId}
            docId={docId}
            activeTab={activeTab}
            selectedCommentId={selectedCommentId}
            commentPanelScope={commentPanelScope}
            onTabChange={onTabChange}
            onClose={closeAndRestoreFocus}
            pageDetailsControls={pageDetailsControls}
          />
        </div>
      </aside>
    </>
  );
}
