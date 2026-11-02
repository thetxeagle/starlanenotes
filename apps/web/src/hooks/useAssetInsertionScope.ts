import { useEffect, useRef } from "react";
import type { Editor as TipTapEditor } from "@tiptap/react";
import { api, captureOfflineServerContext, getApiBase } from "@/lib/api";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { verifyOfflineReplayContext } from "@/lib/offlineReplayOrigin";
import { getStorageSelection } from "@/lib/storageProviders";

export interface AssetInsertionSelection {
  space_id?: string;
  document_id?: string;
  draft_id?: string;
  readOnly?: boolean;
}
export type VerifyAssetInsertion = () => void;
export type CaptureAssetInsertion = () => VerifyAssetInsertion;

export class AssetInsertionScopeError extends Error {
  constructor() {
    super(
      "File insertion stopped because the original editor or workspace changed. Review any completed uploads in their original page.",
    );
    this.name = "AssetInsertionScopeError";
  }
}

function environment(): string | null {
  try {
    const launch = getCurrentLaunchMode();
    const storage = getStorageSelection();
    return JSON.stringify([launch, storage.mode, storage.provider, getApiBase(), api.getToken()]);
  } catch {
    return null;
  }
}

/** One owner per mounted editor surface. Render observes scope changes synchronously. */
export function createAssetInsertionScopeOwner() {
  let active = true;
  let lifetime = 0;
  let current: {
    editor: TipTapEditor | null;
    key: string;
    capture: CaptureAssetInsertion;
  } | null = null;
  return {
    render(editor: TipTapEditor | null, selection: AssetInsertionSelection): CaptureAssetInsertion {
      const launchEnvironment = environment();
      const key = JSON.stringify([
        selection.space_id,
        selection.document_id,
        selection.draft_id,
        selection.readOnly === true,
        editor?.isEditable,
        editor?.isDestroyed,
        launchEnvironment,
      ]);
      if (current?.editor === editor && current.key === key) return current.capture;
      const readOnly = selection.readOnly === true;
      const scope: NonNullable<typeof current> = {
        editor,
        key,
        capture: (() => {
          const epoch = lifetime;
          let valid = true;
          const verifyOwner = () => {
            if (
              !valid ||
              !active ||
              epoch !== lifetime ||
              current !== scope ||
              readOnly ||
              !editor ||
              editor.isDestroyed ||
              !editor.isEditable ||
              launchEnvironment === null ||
              environment() !== launchEnvironment
            ) {
              valid = false;
              throw new AssetInsertionScopeError();
            }
          };
          // A stale render callback may never capture the latest render's scope.
          verifyOwner();
          try {
            const context =
              getCurrentLaunchMode() === "server" ? captureOfflineServerContext() : null;
            const verify = () => {
              try {
                verifyOwner();
                if (context) verifyOfflineReplayContext(context, getApiBase(), api.getToken());
              } catch {
                valid = false;
                throw new AssetInsertionScopeError();
              }
            };
            verify();
            return verify;
          } catch {
            valid = false;
            throw new AssetInsertionScopeError();
          }
        }) as CaptureAssetInsertion,
      };
      current = scope;
      return scope.capture;
    },
    activate() {
      active = true;
    },
    dispose() {
      active = false;
      lifetime += 1;
    },
  };
}

export function useAssetInsertionScope(
  editor: TipTapEditor | null,
  selection: AssetInsertionSelection,
): CaptureAssetInsertion {
  const ownerRef = useRef<ReturnType<typeof createAssetInsertionScopeOwner> | null>(null);
  if (!ownerRef.current) ownerRef.current = createAssetInsertionScopeOwner();
  const owner = ownerRef.current;
  // Deliberately observe selection during render, not a later effect: A→B→A
  // and stale event callbacks must not acquire a new selection's authority.
  const capture = owner.render(editor, selection);
  useEffect(() => {
    owner.activate();
    return () => owner.dispose();
  }, [owner]);
  return capture;
}
