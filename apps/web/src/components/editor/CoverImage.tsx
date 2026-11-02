import { previewOriginalNativeLocalCover } from "@/lib/nativeLocalCover";
import { getStorageSelection } from "@/lib/storageProviders";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { captureOfflineServerContext, getApiBase, api } from "@/lib/api";
import { verifyOfflineReplayContext } from "@/lib/offlineReplayOrigin";
import { coverAttachmentReferences } from "@/lib/publishedCover";
import { verifyPortableAttachmentReference } from "@/lib/portableAttachmentReferences";
import { fetchExactAttachmentVersion } from "@/lib/nativeAttachmentProduct";
import { dataClient } from "@/lib/dataClient";
import { releaseBrowserAttachmentImage } from "@/lib/browserAttachmentImage";
import { useEffect, useMemo, useRef, useState, type DragEvent } from "react";
import { FileImage, ImagePlus, Upload, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { Input } from "@/components/ui/input";

interface CoverImageProps {
  spaceId?: string;
  documentId?: string;
  draftId?: string;
  src?: string;
  onUpdate: (url: string | null) => void | Promise<void>;
  onUpload?: (file: File) => Promise<string>;
  showEmptyPlaceholder?: boolean;
  editable?: boolean;
  compact?: boolean;
}

const MAX_COVER_IMAGE_BYTES = 30 * 1024 * 1024;
const COVER_IMAGE_TYPES = new Set(["image/gif", "image/jpeg", "image/png", "image/webp"]);

export function validateCoverImageFile(file: File): string | null {
  if (!COVER_IMAGE_TYPES.has(file.type)) {
    return "Choose a PNG, JPEG, WebP, or GIF image.";
  }
  if (file.size > MAX_COVER_IMAGE_BYTES) {
    return "Cover images must be 30 MB or smaller.";
  }
  return null;
}

function formatCoverImageSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function CoverImage({
  spaceId,
  documentId, draftId, src,
  onUpdate,
  onUpload,
  showEmptyPlaceholder = true,
  editable = true,
  compact = false,
}: CoverImageProps) {
  const [preview, setPreview] = useState<{identity:object;url:string|null;error:string|null}|null>(null);
  const browser=typeof window!=="undefined",mode=getCurrentLaunchMode();
  const storage=browser?JSON.stringify(getStorageSelection()):null,base=browser?getApiBase():null;
  const token=browser?api.getToken():null,refresh=browser?api.getRefreshToken():null;
  const diskToken=browser?localStorage.getItem("access_token"):null;
  const localEpoch=browser?localStorage.getItem("starlane_native_archive_local_session_v1"):null;
  const serverScope=browser?localStorage.getItem("starlane_server_cache_v1:session-scope"):null;
  // An observation belongs to this full render selection. Token/context inputs
  // invalidate it without placing credentials in the stored preview identity.
  const previewIdentity=useMemo(()=>({src,spaceId,documentId,draftId,mode,storage,base,authenticated:Boolean(token),diskAuthenticated:Boolean(diskToken),refreshPresent:Boolean(refresh),localEpoch,serverScope}),[src,spaceId,documentId,draftId,mode,storage,base,token,diskToken,refresh,localEpoch,serverScope]);
  const nativeAssetReference=Boolean(src?.startsWith("asset://")&&getCurrentLaunchMode()==="local");
  useEffect(() => {
    let cancelled=false, owned:string|null=null;
    if (!src || (getCurrentLaunchMode()==="local"&&!src.startsWith("starlane-local-attachment://")&&!nativeAssetReference)) {setPreview(null);return;}
    const load=async()=>{
      if(src.startsWith("starlane-local-attachment://")){
        setPreview({identity:previewIdentity,url:null,error:null});if(!spaceId||!documentId)throw new Error("Exact local cover page unavailable");
        const url=await previewOriginalNativeLocalCover(src,spaceId,documentId);if(!cancelled)setPreview({identity:previewIdentity,url,error:null});return;
      }
      if(nativeAssetReference){
        setPreview({identity:previewIdentity,url:null,error:null});if(!spaceId||!documentId)throw new Error("Exact imported cover page unavailable");
        const url=await dataClient.previewNativeLocalCoverReference(src,spaceId,documentId,draftId);if(!cancelled)setPreview({identity:previewIdentity,url,error:null});return;
      }
      const refs=coverAttachmentReferences(src,getApiBase());
      if (!refs.size) {setPreview(null);return;}
      setPreview({identity:previewIdentity,url:null,error:null});
      if (!spaceId || !documentId) throw new Error("Exact cover page unavailable");
      const context=captureOfflineServerContext(), id=refs.get(src)!;
      const selected=await fetchExactAttachmentVersion({space_id:spaceId,document_id:documentId},id);
      verifyOfflineReplayContext(context,getApiBase(),api.getToken());
      verifyPortableAttachmentReference(src,selected.rawVersion,context.origin.serverBase);
      const url=await dataClient.previewPinnedDocumentAttachmentVersionImage(spaceId,documentId,id);
      owned=url;verifyOfflineReplayContext(context,getApiBase(),api.getToken());
      if (!cancelled) setPreview({identity:previewIdentity,url,error:null});
      else releaseBrowserAttachmentImage(url);
    };
    void load().catch(error=>{if(owned)releaseBrowserAttachmentImage(owned);if(!cancelled)setPreview({identity:previewIdentity,url:null,error:error instanceof Error?error.message:"Cover preview unavailable"});});
    return()=>{cancelled=true;if(owned)releaseBrowserAttachmentImage(owned);};
  },[src,spaceId,documentId,draftId,nativeAssetReference,previewIdentity]);
  const managed=Boolean(src && (nativeAssetReference || src.startsWith("starlane-local-attachment://") || ((src.startsWith("starlane-attachment://") || src.includes("/api/v1/files/")) && getCurrentLaunchMode()!=="local")));
  const imageSrc=managed ? preview?.identity===previewIdentity ? preview?.url : null : src;
  const previewError=preview?.identity===previewIdentity ? preview?.error : null;
  const [editing, setEditing] = useState(false);
  const [nextUrl, setNextUrl] = useState("");
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [dragging, setDragging] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const closeEditor = () => {
    if (saving) return;
    setEditing(false);
    setNextUrl("");
    setSelectedFile(null);
    setDragging(false);
    setError(null);
  };

  const chooseFile = (file?: File) => {
    if (!file) return;
    const validationError = validateCoverImageFile(file);
    setError(validationError);
    if (validationError) {
      setSelectedFile(null);
      return;
    }
    setSelectedFile(file);
    setNextUrl("");
  };

  const saveCover = async () => {
    const trimmed = nextUrl.trim();
    if (!selectedFile && !trimmed) return;
    setSaving(true);
    setError(null);
    try {
      const nextCover = selectedFile ? await onUpload?.(selectedFile) : trimmed;
      if (!nextCover) {
        throw new Error("Image upload is unavailable.");
      }
      await onUpdate(nextCover);
      setSaving(false);
      closeEditor();
    } catch (saveError) {
      setSaving(false);
      setError(saveError instanceof Error ? saveError.message : "Could not save this cover image.");
    }
  };

  if (!src) {
    if (!editable || !showEmptyPlaceholder) return null;

    return (
      <div className="group mb-2">
        {!editing && (
          <button
            type="button"
            onClick={() => setEditing(true)}
            className="text-text-muted hover:text-text flex h-8 items-center gap-1 text-xs transition-colors duration-100"
          >
            <ImagePlus size={14} />
            Add cover
          </button>
        )}
        {editing && (
          <DialogShell
            labelledBy="cover-image-dialog-title"
            describedBy="cover-image-dialog-description"
            maxWidth="md"
            onClose={closeEditor}
          >
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void saveCover();
              }}
            >
              <DialogHeader className="px-5 py-4">
                <div>
                  <DialogTitle id="cover-image-dialog-title">Add cover image</DialogTitle>
                  <DialogDescription id="cover-image-dialog-description">
                    Upload an image or use an existing image URL.
                  </DialogDescription>
                </div>
              </DialogHeader>

              <DialogBody className="flex flex-col gap-4 p-5">
                <input
                  ref={fileInputRef}
                  type="file"
                  accept="image/png,image/jpeg,image/webp,image/gif"
                  className="sr-only"
                  aria-label="Choose cover image"
                  onChange={(event) => {
                    chooseFile(event.target.files?.[0]);
                    event.target.value = "";
                  }}
                />
                <button
                  type="button"
                  className={`rounded-card flex min-h-36 w-full flex-col items-center justify-center gap-2 border border-dashed px-5 py-6 text-center transition-colors duration-100 ${
                    dragging
                      ? "border-primary bg-surface-raised"
                      : "border-border bg-surface hover:border-primary hover:bg-surface-raised"
                  }`}
                  onClick={() => fileInputRef.current?.click()}
                  onDragEnter={(event) => {
                    event.preventDefault();
                    setDragging(true);
                  }}
                  onDragOver={(event) => event.preventDefault()}
                  onDragLeave={(event) => {
                    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
                      setDragging(false);
                    }
                  }}
                  onDrop={(event: DragEvent<HTMLButtonElement>) => {
                    event.preventDefault();
                    setDragging(false);
                    chooseFile(event.dataTransfer.files?.[0]);
                  }}
                >
                  <span className="rounded-button border-border bg-surface-raised text-primary flex size-9 items-center justify-center border">
                    {selectedFile ? <FileImage size={18} /> : <Upload size={18} />}
                  </span>
                  {selectedFile ? (
                    <>
                      <span className="text-text max-w-full truncate text-sm font-medium">
                        {selectedFile.name}
                      </span>
                      <span className="text-text-muted text-xs">
                        {formatCoverImageSize(selectedFile.size)} · Click to choose another image
                      </span>
                    </>
                  ) : (
                    <>
                      <span className="text-text text-sm font-medium">
                        Click to upload or drag an image here
                      </span>
                      <span className="text-text-muted text-xs">
                        PNG, JPEG, WebP, or GIF · 30 MB maximum
                      </span>
                    </>
                  )}
                </button>

                <div className="flex items-center gap-3" aria-hidden="true">
                  <span className="bg-border h-px flex-1" />
                  <span className="text-text-muted text-xs">or</span>
                  <span className="bg-border h-px flex-1" />
                </div>

                <div className="flex flex-col gap-1.5">
                  <label htmlFor="cover-image-url" className="text-text text-xs font-medium">
                    Image URL
                  </label>
                  <Input
                    id="cover-image-url"
                    value={nextUrl}
                    onChange={(event) => {
                      setNextUrl(event.target.value);
                      if (event.target.value) setSelectedFile(null);
                    }}
                    placeholder="https://example.com/cover.jpg"
                    aria-describedby={error ? "cover-image-error" : undefined}
                    aria-invalid={Boolean(error)}
                    autoComplete="off"
                  />
                </div>

                {error && (
                  <p id="cover-image-error" role="alert" className="text-danger text-sm">
                    {error}
                  </p>
                )}
              </DialogBody>

              <DialogFooter className="px-5 py-4">
                <Button
                  type="button"
                  variant="secondary"
                  size="sm"
                  onClick={closeEditor}
                  disabled={saving}
                >
                  Cancel
                </Button>
                <Button
                  type="submit"
                  size="sm"
                  disabled={saving || (!selectedFile && !nextUrl.trim())}
                >
                  {saving ? "Saving…" : "Save cover"}
                </Button>
              </DialogFooter>
            </form>
          </DialogShell>
        )}
      </div>
    );
  }

  return (
    <div
      className={`rounded-card group relative w-full overflow-hidden ${compact ? "h-28" : "mb-6 h-48"}`}
    >
      {imageSrc ? <img src={imageSrc} alt="Cover" className="h-full w-full object-cover" /> :
        <p className="text-text-muted flex h-full items-center justify-center p-4 text-sm" role={previewError ? "alert" : "status"}>{previewError ?? "Preparing exact cover preview…"}</p>}

      {editable && (
        <button
          onClick={() => onUpdate(null)}
          className="rounded-button absolute top-2 right-2 flex h-6 w-6 items-center justify-center bg-black/50 text-white opacity-0 transition-opacity duration-100 group-hover:opacity-100 focus-visible:opacity-100"
          aria-label="Remove cover"
        >
          <X size={14} />
        </button>
      )}
    </div>
  );
}
