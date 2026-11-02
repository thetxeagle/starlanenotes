import {AttachmentCopyReview} from "./AttachmentCopyReview";
import {PendingUploadCleanupReview} from "./PendingUploadCleanupReview";
import {DirectUploadReview} from "./DirectUploadReview";
import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import {
  AlertCircle,
  CheckCircle2,
  CloudUpload,
  FileText,
  LoaderCircle,
  Plus,
  RefreshCw,
  Upload,
  X,
} from "lucide-react";
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
import { useListboxKeyboardNavigation } from "@/components/ui/useListboxKeyboardNavigation";
import type { AttachmentRecord, LocalAssetRecord } from "@/lib/dataClient";

export interface EditorUploadChoice {
  id: "image" | "file";
  label: string;
  description: string;
  preferImage: boolean;
}

export type EditorReusableAsset = LocalAssetRecord | AttachmentRecord;

export type EditorUploadProgress = {
  index: number;
  status: "queued" | "uploading" | "uploaded" | "failed";
  error?: string;
};

export type EditorUploadResult = EditorUploadProgress & {
  status: "uploaded" | "failed";
};

export function getEditorAssetOptionState(
  assetId: string,
  index: number,
  activeAssetId: string | null,
  selectedAssetId: string | null,
) {
  return {
    tabIndex: activeAssetId ? (activeAssetId === assetId ? 0 : -1) : index === 0 ? 0 : -1,
    selected: selectedAssetId === assetId,
  };
}

export function activateEditorAssetOption(
  assetId: string,
  setFocusedAssetId: (id: string) => void,
  setSelectedAssetId: (id: string) => void,
): void {
  setFocusedAssetId(assetId);
  setSelectedAssetId(assetId);
}

function isFileLike(value: unknown): value is File {
  if (!value || typeof value !== "object") return false;
  const file = value as Partial<File>;
  return (
    typeof file.name === "string" &&
    typeof file.size === "number" &&
    typeof file.type === "string" &&
    typeof file.arrayBuffer === "function"
  );
}

export function getEditorUploadChoices(
  maxFileMb: number,
  preferImage: boolean,
): EditorUploadChoice[] {
  const choices: EditorUploadChoice[] = [
    {
      id: "image",
      label: "Choose image",
      description: `Insert an image asset up to ${maxFileMb} MB`,
      preferImage: true,
    },
    {
      id: "file",
      label: "Choose file",
      description: `Attach a vault file up to ${maxFileMb} MB`,
      preferImage: false,
    },
  ];

  return choices.sort(
    (first, second) =>
      Number(second.preferImage === preferImage) - Number(first.preferImage === preferImage),
  );
}

export function isEditorReusableImage(asset: EditorReusableAsset): boolean {
  const mime = asset.mime_type;
  const name = getAssetName(asset);
  return Boolean(mime?.startsWith("image/")) || /\.(avif|gif|jpe?g|png|svg|webp)$/i.test(name);
}

function getAssetName(asset: EditorReusableAsset): string {
  return "original_name" in asset ? asset.original_name : asset.name;
}

function getAssetSize(asset: EditorReusableAsset): number {
  return "size_bytes" in asset ? asset.size_bytes : asset.size;
}

function getAssetUrl(asset: EditorReusableAsset): string {
  return asset.url;
}

function PendingFilePreview({ file }: { file: File }) {
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);

  useEffect(() => {
    if (!file.type.startsWith("image/")) return;
    const url = URL.createObjectURL(file);
    setPreviewUrl(url);
    return () => URL.revokeObjectURL(url);
  }, [file]);

  return previewUrl ? (
    <img
      src={previewUrl}
      alt=""
      className="rounded-input border-border size-14 shrink-0 border object-cover sm:size-16"
    />
  ) : (
    <span className="rounded-input border-border bg-surface text-text-muted flex size-14 shrink-0 items-center justify-center border sm:size-16">
      <FileText size={22} aria-hidden="true" />
    </span>
  );
}

interface EditorUploadDialogProps {
  triggerRef?: RefObject<HTMLElement | null>;
  retainedContext?: {spaceId:string;documentId:string;draftId?:string;referencedVersionIds:()=>ReadonlySet<string>;onUse:(attachment:AttachmentRecord)=>Promise<void>};
  maxFileMb: number;
  existingAssets: EditorReusableAsset[];
  existingLoading: boolean;
  existingError?: string;
  onFilesSelected: (
    files: File[],
    onProgress: (progress: EditorUploadProgress) => void,
  ) => Promise<EditorUploadResult[]>;
  onSelectExisting: (asset: EditorReusableAsset) => void;
  onRefreshExisting: () => void;
  onCancel: () => void;
}

export function EditorUploadDialog({
  triggerRef,
  retainedContext,
  maxFileMb,
  existingAssets,
  existingLoading,
  existingError,
  onFilesSelected,
  onSelectExisting,
  onRefreshExisting,
  onCancel,
}: EditorUploadDialogProps) {
  const [tab, setTab] = useState<"upload" | "existing">("upload");
  const [pendingFiles, setPendingFiles] = useState<File[]>([]);
  const [isDragging, setIsDragging] = useState(false);
  const [uploadStates, setUploadStates] = useState<Record<number, EditorUploadProgress>>({});
  const [uploadSummary, setUploadSummary] = useState<string | null>(null);
  const [isUploading, setIsUploading] = useState(false);
  const [selectedAssetId, setSelectedAssetId] = useState<string | null>(null);
  const [focusedAssetId, setFocusedAssetId] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const existingAssetsListRef = useRef<HTMLDivElement | null>(null);
  const handleExistingAssetsKeyDown = useListboxKeyboardNavigation(existingAssetsListRef);

  const selectedAsset = existingAssets.find((asset) => asset.id === selectedAssetId) ?? null;
  const activeAssetId = existingAssets.some((asset) => asset.id === focusedAssetId)
    ? focusedAssetId
    : selectedAsset?.id ?? existingAssets[0]?.id ?? null;
  const uploadComplete =
    pendingFiles.length > 0 &&
    pendingFiles.every((_, index) => uploadStates[index]?.status === "uploaded");
  const uploadHasFailures = Object.values(uploadStates).some((state) => state.status === "failed");

  const addFiles = (files: FileList | File[]) => {
    if (isUploading) return;
    const nextFiles = Array.from(files).filter(isFileLike);
    if (nextFiles.length === 0) return;
    setPendingFiles((current) => (uploadComplete ? nextFiles : [...current, ...nextFiles]));
    setUploadStates({});
    setUploadSummary(null);
  };

  const closeDialog = useCallback(() => {
    if (!isUploading) onCancel();
  }, [isUploading, onCancel]);

  const uploadFiles = async () => {
    if (isUploading || pendingFiles.length === 0) return;

    setIsUploading(true);
    setUploadSummary(null);
    setUploadStates(
      Object.fromEntries(
        pendingFiles.map((_, index) => [index, { index, status: "queued" as const }]),
      ),
    );

    try {
      const results = await onFilesSelected(pendingFiles, (progress) => {
        setUploadStates((current) => ({ ...current, [progress.index]: progress }));
      });
      const failedResults = results.filter((result) => result.status === "failed");
      if (failedResults.length === 0 && results.length === pendingFiles.length) {
        setUploadStates(Object.fromEntries(results.map((result) => [result.index, result])));
        setUploadSummary(
          `${results.length} ${results.length === 1 ? "file was" : "files were"} added to the page.`,
        );
        return;
      }

      const failedFiles = failedResults.map((result) => pendingFiles[result.index]).filter(Boolean);
      setPendingFiles(failedFiles);
      setUploadStates(
        Object.fromEntries(failedResults.map((result, index) => [index, { ...result, index }])),
      );
      const uploadedCount = results.length - failedResults.length;
      setUploadSummary(
        uploadedCount > 0
          ? `${uploadedCount} ${uploadedCount === 1 ? "file was" : "files were"} added. Retry the failed ${failedResults.length === 1 ? "file" : "files"}.`
          : "Upload failed. Review the error below and try again.",
      );
    } catch (error) {
      const message = error instanceof Error ? error.message : "The upload request failed.";
      setUploadStates(
        Object.fromEntries(
          pendingFiles.map((_, index) => [
            index,
            { index, status: "failed" as const, error: message },
          ]),
        ),
      );
      setUploadSummary("Upload failed. Review the error below and try again.");
    } finally {
      setIsUploading(false);
    }
  };

  return (
    <DialogShell
      labelledBy="editor-upload-title"
      describedBy="editor-upload-description"
      maxWidth="settings"
      onClose={closeDialog}
      triggerRef={triggerRef}
    >
      <div
        className="flex max-h-[calc(100dvh-2rem)] min-h-[min(32rem,calc(100dvh-2rem-2px))] flex-col sm:min-h-[min(36rem,calc(100dvh-2rem-2px))]"
        data-starlane-file-drop-boundary="true"
      >
        <DialogHeader className="px-5 py-4 sm:px-6 sm:py-5">
          <div>
            <DialogTitle id="editor-upload-title" className="text-xl sm:text-2xl">
              Upload an image or file
            </DialogTitle>
            <DialogDescription id="editor-upload-description" className="mt-2">
              Upload something to add to the page, or choose an existing asset in this space.
            </DialogDescription>
          </div>
          <DialogCloseAction
            label="Close upload dialog"
            disabled={isUploading}
            onClick={closeDialog}
          />
        </DialogHeader>

        <DialogBody className="flex min-h-0 flex-1 flex-col p-5 sm:p-6">
        {retainedContext?<><DirectUploadReview {...retainedContext}/>{retainedContext.draftId?<AttachmentCopyReview {...retainedContext} draftId={retainedContext.draftId}/>:null}<PendingUploadCleanupReview spaceId={retainedContext.spaceId} documentId={retainedContext.documentId}/></>:null}
          <div
            className="border-border bg-surface-raised flex border-b px-2"
            role="tablist"
            aria-label="File source"
          >
            {(["upload", "existing"] as const).map((nextTab) => (
              <button
                key={nextTab}
                type="button"
                role="tab"
                aria-selected={tab === nextTab}
                disabled={isUploading}
                onClick={() => {
                  setTab(nextTab);
                  setSelectedAssetId(null);
                  setFocusedAssetId(null);
                }}
                className={`focus-ring -mb-px min-h-11 border-b-2 px-4 py-2 text-sm font-medium transition-colors duration-100 ${
                  tab === nextTab
                    ? "border-primary text-primary"
                    : "text-text-muted hover:text-text border-transparent"
                }`}
              >
                {nextTab === "upload" ? "Upload" : "Library"}
              </button>
            ))}
          </div>

          <div className="border-border bg-surface-raised min-h-0 flex-1 overflow-y-auto border border-t-0 p-4 sm:p-5">
          {tab === "upload" ? (
            <div className="flex h-full min-h-64 flex-col">
              <p role="status" aria-live="polite" className="sr-only">
                {isUploading
                  ? `Uploading ${pendingFiles.length} ${pendingFiles.length === 1 ? "file" : "files"}.`
                  : uploadSummary || ""}
              </p>
              {pendingFiles.length === 0 ? (
                <div
                  className={`focus-ring rounded-input relative flex min-h-64 flex-1 flex-col items-center justify-center border border-dashed px-4 py-8 text-center transition-colors duration-100 ${
                    isDragging
                      ? "border-primary bg-primary/10"
                      : "border-border bg-surface hover:border-primary hover:bg-primary/5"
                  }`}
                  onDragEnter={(event) => {
                    event.preventDefault();
                    if (isUploading) return;
                    setIsDragging(true);
                  }}
                  onDragOver={(event) => event.preventDefault()}
                  onDragLeave={() => setIsDragging(false)}
                  onDrop={(event) => {
                    event.preventDefault();
                    setIsDragging(false);
                    if (isUploading) return;
                    addFiles(event.dataTransfer.files);
                  }}
                  aria-busy={isUploading}
                >
                  <input
                    ref={inputRef}
                    id="editor-upload-file-input"
                    type="file"
                    multiple
                    disabled={isUploading}
                    aria-label="Browse for files"
                    className="absolute inset-0 z-10 size-full cursor-pointer opacity-0"
                    onChange={(event) => {
                      if (event.target.files) addFiles(event.target.files);
                      event.target.value = "";
                    }}
                  />
                  <CloudUpload
                    className="text-primary pointer-events-none mb-4 size-12"
                    strokeWidth={1.7}
                    aria-hidden="true"
                  />
                  <span className="text-text pointer-events-none text-base font-medium">
                    {isDragging ? "Drop files here" : "Drop files here or click to browse"}
                  </span>
                  <span className="text-text-muted pointer-events-none mt-2 text-xs">
                    Any file type · Up to {maxFileMb} MB each
                  </span>
                </div>
              ) : (
                <div className="space-y-3" aria-label="Selected files">
                  {pendingFiles.map((file, index) => {
                    const uploadState = uploadStates[index];
                    return (
                      <div
                        key={`${file.name}-${file.lastModified}-${index}`}
                        className="rounded-input border-border bg-surface flex items-center gap-3 border p-3"
                      >
                        <PendingFilePreview file={file} />
                        <span className="min-w-0 flex-1">
                          <span className="flex items-center justify-between gap-3">
                            <span className="text-text block truncate text-sm font-medium">
                              {file.name}
                            </span>
                            <span className="text-text-muted shrink-0 text-xs">
                              {formatFileSize(file.size)}
                            </span>
                          </span>
                          {uploadState ? <UploadStateIndicator state={uploadState} /> : null}
                        </span>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          disabled={isUploading}
                          aria-label={`Remove ${file.name}`}
                          onClick={() =>
                            setPendingFiles((current) =>
                              current.filter((_, item) => item !== index),
                            )
                          }
                        >
                          <X size={15} aria-hidden="true" />
                        </Button>
                      </div>
                    );
                  })}
                  {uploadSummary ? (
                    <p
                      role={uploadHasFailures ? "alert" : "status"}
                      className={`rounded-input border px-3 py-2 text-xs ${
                        uploadHasFailures
                          ? "border-danger/30 bg-danger/10 text-danger"
                          : "border-success/30 bg-success/10 text-success"
                      }`}
                    >
                      {uploadSummary}
                    </p>
                  ) : null}
                </div>
              )}
            </div>
          ) : (
            <div className="flex h-full min-h-64 flex-col">
              <div className="mb-4 flex items-center justify-between gap-3">
                <p className="text-text-muted text-xs">
                  Choose a file or image already uploaded to this space.
                </p>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  aria-label="Refresh existing assets"
                  onClick={onRefreshExisting}
                >
                  <RefreshCw size={15} aria-hidden="true" />
                </Button>
              </div>
              {existingLoading ? (
                <div className="text-text-muted flex min-h-64 flex-1 items-center justify-center text-sm">
                  <LoaderCircle className="mr-2 animate-spin" size={16} aria-hidden="true" />{" "}
                  Loading assets…
                </div>
              ) : existingError ? (
                <div className="rounded-input border-danger/30 bg-danger/10 text-danger border p-4 text-sm">
                  {existingError}
                </div>
              ) : existingAssets.length === 0 ? (
                <div className="rounded-input border-border bg-surface flex min-h-64 flex-1 flex-col items-center justify-center border border-dashed text-center">
                  <FileText className="text-text-muted mb-2" size={24} aria-hidden="true" />
                  <p className="text-text text-sm">No existing assets yet.</p>
                  <p className="text-text-muted mt-1 text-xs">
                    Upload the first one from the Upload tab.
                  </p>
                </div>
              ) : (
                <div
                  ref={existingAssetsListRef}
                  className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4"
                  role="listbox"
                  aria-label="Existing space assets"
                  onKeyDown={handleExistingAssetsKeyDown}
                >
                  {existingAssets.map((asset, index) => {
                    const image = isEditorReusableImage(asset);
                    const name = getAssetName(asset);
                    const optionState = getEditorAssetOptionState(
                      asset.id,
                      index,
                      activeAssetId,
                      selectedAssetId,
                    );
                    return (
                      <button
                        key={asset.id}
                        type="button"
                        role="option"
                        aria-selected={optionState.selected}
                        tabIndex={optionState.tabIndex}
                        aria-label={`Use ${name}`}
                        className={`focus-ring rounded-input bg-surface hover:border-primary group relative overflow-hidden border text-left transition-colors duration-100 ${
                          optionState.selected
                            ? "border-primary ring-primary/30 ring-2"
                            : "border-border"
                        }`}
                        onFocus={() => setFocusedAssetId(asset.id)}
                        onClick={() =>
                          activateEditorAssetOption(
                            asset.id,
                            setFocusedAssetId,
                            setSelectedAssetId,
                          )
                        }
                      >
                        <div className="bg-surface-raised flex aspect-[4/3] items-center justify-center">
                          {image ? (
                            <img
                              src={getAssetUrl(asset)}
                              alt=""
                              className="size-full object-cover"
                            />
                          ) : (
                            <FileText className="text-text-muted" size={28} aria-hidden="true" />
                          )}
                        </div>
                        {optionState.selected ? (
                          <span className="bg-primary absolute right-2 top-2 flex size-6 items-center justify-center rounded-full text-white">
                            <CheckCircle2 size={15} aria-hidden="true" />
                          </span>
                        ) : null}
                        <div className="p-2">
                          <p className="text-text truncate text-xs">{name}</p>
                          <p className="text-text-muted mt-0.5 text-[11px]">
                            {formatFileSize(getAssetSize(asset))}
                          </p>
                        </div>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          )}
          </div>
        </DialogBody>

        <DialogFooter className="flex-col-reverse gap-3 px-5 py-4 sm:flex-row sm:items-center sm:justify-between sm:px-6">
          <Button type="button" variant="secondary" disabled={isUploading} onClick={closeDialog}>
            {isUploading ? "Uploading…" : uploadComplete ? "Close" : "Cancel"}
          </Button>
          <div className="flex w-full gap-2 sm:w-auto sm:justify-end">
            {tab === "upload" && pendingFiles.length > 0 ? (
              <>
                <input
                  ref={inputRef}
                  id="editor-upload-more-input"
                  type="file"
                  multiple
                  disabled={isUploading}
                  aria-label="Add more files"
                  className="sr-only"
                  onChange={(event) => {
                    if (event.target.files) addFiles(event.target.files);
                    event.target.value = "";
                  }}
                />
                <label
                  htmlFor="editor-upload-more-input"
                  className={`rounded-button focus-ring border-border bg-surface text-text hover:bg-surface-raised inline-flex h-8 flex-1 cursor-pointer items-center justify-center gap-2 border px-4 text-sm font-medium transition-colors duration-100 sm:flex-none ${
                    isUploading ? "pointer-events-none opacity-50" : ""
                  }`}
                >
                  <Plus size={15} aria-hidden="true" />
                  Add more
                </label>
              </>
            ) : null}
            {tab === "upload" ? (
              <Button
                type="button"
                className="flex-1 sm:flex-none"
                disabled={isUploading || pendingFiles.length === 0}
                onClick={uploadComplete ? closeDialog : uploadFiles}
              >
                {isUploading ? (
                  <LoaderCircle
                    className="animate-spin"
                    size={15}
                    data-icon="inline-start"
                    aria-hidden="true"
                  />
                ) : uploadComplete ? (
                  <CheckCircle2 size={15} data-icon="inline-start" aria-hidden="true" />
                ) : (
                  <Upload size={15} data-icon="inline-start" aria-hidden="true" />
                )}
                {isUploading
                  ? `Uploading ${pendingFiles.length} ${pendingFiles.length === 1 ? "file" : "files"}…`
                  : uploadComplete
                    ? "Done"
                    : uploadStates[0]?.status === "failed"
                      ? `Retry ${pendingFiles.length === 1 ? "file" : "files"}`
                      : `Upload${pendingFiles.length > 0 ? ` ${pendingFiles.length}` : ""}`}
              </Button>
            ) : (
              <Button
                type="button"
                className="flex-1 sm:flex-none"
                disabled={!selectedAsset}
                onClick={() => {
                  if (selectedAsset) onSelectExisting(selectedAsset);
                }}
              >
                Select
              </Button>
            )}
          </div>
        </DialogFooter>
      </div>
    </DialogShell>
  );
}

function UploadStateIndicator({ state }: { state: EditorUploadProgress }) {
  const failed = state.status === "failed";
  const uploaded = state.status === "uploaded";
  const uploading = state.status === "uploading";
  const label = failed
    ? state.error || "Upload failed"
    : uploaded
      ? "Complete"
      : uploading
        ? "Uploading…"
        : "Waiting to upload";

  return (
    <span className="mt-2 block">
      <span
        className={`flex items-center gap-1.5 text-xs ${
          failed
            ? "text-danger"
            : uploaded
              ? "text-success"
              : uploading
                ? "text-primary"
                : "text-text-muted"
        }`}
      >
        {failed ? (
          <AlertCircle size={13} aria-hidden="true" />
        ) : uploaded ? (
          <CheckCircle2 size={13} aria-hidden="true" />
        ) : uploading ? (
          <LoaderCircle className="animate-spin" size={13} aria-hidden="true" />
        ) : null}
        {label}
      </span>
      {uploading || uploaded ? (
        <span
          className="bg-primary/20 mt-2 block h-1.5 overflow-hidden rounded-full"
          aria-hidden="true"
        >
          <span
            className={`bg-primary block h-full w-full rounded-full ${uploading ? "animate-pulse opacity-60" : ""}`}
          />
        </span>
      ) : null}
    </span>
  );
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
