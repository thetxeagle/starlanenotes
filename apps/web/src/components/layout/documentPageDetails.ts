import type { NativeLocalCoverRecovery } from "@/components/editor/NativeLocalCoverRecoveryControl";
import { createContext } from "react";
import type { RetainedPageCssSource } from "@/lib/pageCssProvenance";
import type { DocumentPageWidth } from "@/components/editor/DocumentWidthPicker";
import type { DraftDiscussionTarget } from "@/lib/draftDiscussion";
import type { DraftDiscussionAnchor } from "@/lib/draftDiscussion";

export interface DocumentPageDetailsControls {
  draftDiscussion?: {
    target: DraftDiscussionTarget;
    currentUserId: string;
    pendingAnchor?: DraftDiscussionAnchor | null;
    onAnchorConsumed?: () => void;
  };
  coverImage?: string;
  coverDraftId?: string;
  nativeCoverRecovery?: NativeLocalCoverRecovery;
  coverEditable: boolean;
  onCoverUpload: (file: File) => Promise<string>;
  onCoverUpdate: (url: string | null) => Promise<void>;
  pageWidth: DocumentPageWidth;
  pageWidthEditable: boolean;
  onPageWidthChange: (width: DocumentPageWidth) => void;
  pageValuesEditable: boolean;
  schemaEditable: boolean;
  contentMetadataEditable: boolean;
  synopsis: string;
  pageCssSource?: RetainedPageCssSource;
  onSynopsisChange?: (synopsis: string) => void;
  onSynopsisCompositionChange?: (composing: boolean) => void;
  onSynopsisUpdate?: (synopsis: string) => Promise<void>;
}

export type RegisterDocumentPageDetails = (controls: DocumentPageDetailsControls | null) => void;

export const DocumentPageDetailsRegistrationContext = createContext<RegisterDocumentPageDetails>(
  () => undefined,
);
