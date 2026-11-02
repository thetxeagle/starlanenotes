import {
  NativeLocalCoverRecoveryControl,
  type NativeLocalCoverRecovery,
} from "@/components/editor/NativeLocalCoverRecoveryControl";
import { lazy, Suspense, useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import {
  ArrowDown,
  ArrowUp,
  Check,
  CheckSquare,
  ChevronDown,
  ChevronRight,
  Hash,
  Link,
  Pencil,
  Plus,
  Search,
  Square,
  Star,
  Text,
  Trash2,
  Workflow,
  X,
} from "lucide-react";
import { emitGraphDirty } from "@/lib/graphEvents";
import { capturePageMetadataLoadGuard } from "@/lib/pageMetadataLoad";
import { isApiError } from "@/lib/api";
import {
  dataClient,
  type DocumentWorkflowEventRecord,
  type DocumentWorkflowRecord,
  type DocumentRecord,
  type DocumentPropertyValueRecord,
  type TagRecord,
  type UserDirectoryRecord,
  type WorkflowTemplateRecord,
} from "@/lib/dataClient";
import {
  PageMaintenanceControl,
  type MaintenanceChoice,
  type MaintenanceSource,
} from "@/components/PageMaintenanceControl";
import { PagePreservedMetadata } from "@/components/PagePreservedMetadata";
import { PageIdentifierControl } from "@/components/PageIdentifierControl";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { DialogShell } from "@/components/ui/dialog-shell";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useMenuKeyboardNavigation } from "@/components/ui/useMenuKeyboardNavigation";
import { inspectPageCssSource, type RetainedPageCssSource } from "@/lib/pageCssProvenance";
import {
  PageCssRecoveryContent,
  selectPageCssRecoverySource,
} from "@/components/PageCssRecoveryContent";
import type { DocumentPageWidth } from "@/components/editor/DocumentWidthPicker";

const CoverImage = lazy(() =>
  import("@/components/editor/CoverImage").then((module) => ({ default: module.CoverImage })),
);
export const PROPERTIES_PANEL_ERROR_CLASS =
  "rounded-input border border-danger/30 bg-danger/10 px-2 py-1.5 text-[11px] leading-4 text-danger";

export function getPropertyDeleteConfirmationMessage(name: string): string {
  const trimmed = name.trim();
  return trimmed
    ? `Delete property "${trimmed}" from this space? Existing values will be removed from documents.`
    : "Delete this property from the space? Existing values will be removed from documents.";
}

export function canEditContentMetadataValue(
  editable: boolean,
  locked: boolean,
  pageDraftManaged: boolean,
  hasPageDraftUpdate: boolean,
): boolean {
  return editable && !locked && (!pageDraftManaged || hasPageDraftUpdate);
}

/** The original draft must own a keystroke before the panel displays it. */
export function acceptSynopsisInput(
  value: string,
  editable: boolean,
  managed: boolean,
  change?: (value: string) => void,
): void {
  if (!editable) throw new Error("Open an editable draft before changing the page summary.");
  if (managed && !change) throw new Error("Original synopsis draft is unavailable.");
  change?.(value);
}

export function canEditPropertiesValue(editable: boolean, locked: boolean): boolean {
  return editable && !locked;
}

export function canEditLoadedMetadataValue(
  editable: boolean,
  loading: boolean,
  error: "permission" | "unavailable" | null,
): boolean {
  return editable && !loading && !error;
}

export function getMetadataReadErrorKind(error: unknown): "permission" | "unavailable" {
  return isApiError(error) && error.status === 403 ? "permission" : "unavailable";
}

export async function loadMetadataCollection<T>(
  load: () => Promise<T[]>,
): Promise<{ items: T[]; error: "permission" | "unavailable" | null }> {
  try {
    return { items: await load(), error: null };
  } catch (error) {
    return { items: [], error: getMetadataReadErrorKind(error) };
  }
}

export async function persistWithRollback(
  persist: () => Promise<void>,
  rollback: () => void,
  onError: (message: string) => void,
  fallbackMessage: string,
): Promise<boolean> {
  try {
    await persist();
    return true;
  } catch (error) {
    rollback();
    const detail = error instanceof Error ? error.message : "";
    onError(detail ? `${fallbackMessage} ${detail}` : fallbackMessage);
    return false;
  }
}

export async function persistContentMetadataValue<T>(
  value: T,
  pageDraftManaged: boolean,
  onPageDraftUpdate?: (value: T) => Promise<void>,
  onStandaloneUpdate?: (value: T) => Promise<void>,
): Promise<void> {
  if (pageDraftManaged) {
    if (!onPageDraftUpdate) {
      throw new Error("Page metadata updates require an editable draft callback.");
    }
    await onPageDraftUpdate(value);
    return;
  }
  if (!onStandaloneUpdate) {
    throw new Error("Standalone page metadata update is unavailable.");
  }
  await onStandaloneUpdate(value);
}

const TAG_COLORS = [
  "#ffffff",
  "#111827",
  "#6b7280",
  "#94a3b8",
  "#2563eb",
  "#4f46e5",
  "#7c3aed",
  "#0891b2",
  "#059669",
  "#65a30d",
  "#ca8a04",
  "#ea580c",
  "#dc2626",
  "#db2777",
  "#f472b6",
  "#475569",
];

const PROPERTY_TYPE_OPTIONS = [
  ["text", "Text"],
  ["number", "Number"],
  ["date", "Date"],
  ["datetime", "Date and time"],
  ["checkbox", "Checkbox"],
  ["select", "Select"],
  ["multi_select", "Multi-select"],
  ["rating", "Rating"],
  ["user", "User"],
  ["relation", "Relation"],
  ["url", "URL"],
] as const;

const SYNOPSIS_UPDATED_EVENT = "starlane:synopsis-updated";

interface PropertiesPanelProps {
  spaceId: string;
  docId: string;
  title: string;
  icon?: string;
  coverImage?: string;
  coverEditable?: boolean;
  onCoverUpload?: (file: File) => Promise<string>;
  nativeCoverRecovery?: NativeLocalCoverRecovery;
  coverDraftId?: string;
  onCoverUpdate?: (url: string | null) => Promise<void>;
  pageWidth?: DocumentPageWidth;
  pageWidthEditable?: boolean;
  onPageWidthChange?: (width: DocumentPageWidth) => void;
  pageValuesEditable?: boolean;
  schemaEditable?: boolean;
  contentMetadataEditable?: boolean;
  synopsis?: string;
  pageCssSource?: RetainedPageCssSource;
  onSynopsisChange?: (synopsis: string) => void;
  onSynopsisCompositionChange?: (composing: boolean) => void;
  onSynopsisUpdate?: (synopsis: string) => Promise<void>;
  createdBy?: string;
  lastUpdatedBy?: string;
  createdAt?: string;
  updatedAt?: string;
  locked?: boolean;
  isPublic?: boolean;
}

export function PropertiesPanel({
  spaceId,
  docId,
  title,
  icon,
  coverImage,
  coverEditable = false,
  onCoverUpload,
  nativeCoverRecovery,
  coverDraftId,
  onCoverUpdate,
  pageValuesEditable = true,
  schemaEditable = true,
  contentMetadataEditable = true,
  synopsis: synopsisValue,
  pageCssSource,
  onSynopsisUpdate,
  onSynopsisChange,
  onSynopsisCompositionChange,
  createdBy,
  lastUpdatedBy,
  createdAt,
  updatedAt,
  locked,
  isPublic,
}: PropertiesPanelProps) {
  const loadMaintenance = useCallback(async () => {
    const requireSameSession = capturePageMetadataLoadGuard();
    const document = await dataClient.getDocument(spaceId, docId);
    const pending = await dataClient.getPendingDocumentMaintenance(spaceId, docId);
    if (!document.updated_at)
      throw new Error("Page revision is unavailable for responsibility editing");
    let catalogUnavailable = false;
    const targets = await dataClient.getDocumentMaintenanceTargets(spaceId, docId).catch(() => {
      catalogUnavailable = true;
      return { users: [], groups: [], groups_available: false };
    });
    requireSameSession();
    return {
      assertCurrentContext: requireSameSession,
      source: {
        page_updated_at: document.updated_at,
        published_at: document.published_at ?? null,
        maintenance_user_id: document.maintenance_user_id ?? null,
        maintenance_group_id: document.maintenance_group_id ?? null,
      },
      pending,
      targets,
      catalogUnavailable,
      editable: !document.locked && document.maintenance_edit_allowed === true,
    };
  }, [spaceId, docId]);
  const saveMaintenance = useCallback(
    (choice: MaintenanceChoice, source: MaintenanceSource) =>
      dataClient.setDocumentMaintenance(spaceId, docId, choice, source),
    [spaceId, docId],
  );
  const loadIdentifier = useCallback(async () => {
    const document = await dataClient.getDocument(spaceId, docId);
    const pending = await dataClient.getPendingDocumentIdentifier(spaceId, docId);
    if (!document.updated_at)
      throw new Error("Page revision is unavailable for identifier editing");
    return {
      source: {
        page_updated_at: document.updated_at,
        published_at: document.published_at ?? null,
        human_identifier: document.human_identifier ?? null,
      },
      pending,
      editable: !document.locked && document.identifier_edit_allowed === true,
    };
  }, [spaceId, docId]);
  const saveIdentifier = useCallback(
    (
      value: string | null,
      source: {
        page_updated_at: string;
        published_at: string | null;
        human_identifier: string | null;
      },
    ) => dataClient.setDocumentIdentifier(spaceId, docId, value, source),
    [spaceId, docId],
  );
  const [expanded, setExpanded] = useState(true);
  const [properties, setProperties] = useState<DocumentPropertyValueRecord[]>([]);
  const [propertiesLoading, setPropertiesLoading] = useState(true);
  const [propertiesLoadError, setPropertiesLoadError] = useState<
    "permission" | "unavailable" | null
  >(null);
  const [relationDocuments, setRelationDocuments] = useState<DocumentRecord[]>([]);
  const [userDirectory, setUserDirectory] = useState<UserDirectoryRecord[]>([]);
  const [tags, setTags] = useState<TagRecord[]>([]);
  const [availableTags, setAvailableTags] = useState<TagRecord[]>([]);
  const [tagsLoading, setTagsLoading] = useState(true);
  const [tagsLoadError, setTagsLoadError] = useState<"permission" | "unavailable" | null>(null);
  const [workflowTemplates, setWorkflowTemplates] = useState<WorkflowTemplateRecord[]>([]);
  const [documentWorkflow, setDocumentWorkflow] = useState<DocumentWorkflowRecord | null>(null);
  const [workflowEvents, setWorkflowEvents] = useState<DocumentWorkflowEventRecord[]>([]);
  const [selectedWorkflowTemplateId, setSelectedWorkflowTemplateId] = useState("");
  const [workflowBusy, setWorkflowBusy] = useState(false);
  const [workflowError, setWorkflowError] = useState("");
  const [synopsis, setSynopsis] = useState("");
  const synopsisComposing = useRef(false);
  useEffect(
    () => () => {
      if (!synopsisComposing.current) return;
      synopsisComposing.current = false;
      try {
        onSynopsisCompositionChange?.(false);
      } catch {
        /* The original selection has already closed. */
      }
    },
    [onSynopsisCompositionChange],
  );
  const [synopsisBusy, setSynopsisBusy] = useState(false);
  const [synopsisError, setSynopsisError] = useState("");
  const [documentContent, setDocumentContent] = useState("");
  const [fallbackCssSource, setFallbackCssSource] = useState<RetainedPageCssSource>();
  const [cssReview, setCssReview] = useState<{ source: RetainedPageCssSource; key: string } | null>(
    null,
  );
  const cssSourceTriggerRef = useRef<HTMLButtonElement | null>(null);
  const [newPropertyName, setNewPropertyName] = useState("");
  const [newPropertyType, setNewPropertyType] = useState("text");
  const [newPropertyOptions, setNewPropertyOptions] = useState("");
  const [propertyComposerOpen, setPropertyComposerOpen] = useState(false);
  const [propertyComposerBusy, setPropertyComposerBusy] = useState(false);
  const [propertyComposerError, setPropertyComposerError] = useState("");
  const [newTagName, setNewTagName] = useState("");
  const [tagInputFocused, setTagInputFocused] = useState(false);
  const [tagComposerOpen, setTagComposerOpen] = useState(false);
  const [tagComposerBusy, setTagComposerBusy] = useState(false);
  const [tagComposerError, setTagComposerError] = useState("");
  const [activeTagColorMenuId, setActiveTagColorMenuId] = useState<string | null>(null);
  const [editingPropertyId, setEditingPropertyId] = useState<string | null>(null);
  const [editingPropertyName, setEditingPropertyName] = useState("");
  const [editingPropertyType, setEditingPropertyType] = useState("text");
  const [editingPropertyOptions, setEditingPropertyOptions] = useState("");
  const [deletePropertyTarget, setDeletePropertyTarget] = useState<{
    id: string;
    name: string;
  } | null>(null);
  const [editingTagId, setEditingTagId] = useState<string | null>(null);
  const [editingTagName, setEditingTagName] = useState("");
  const [editingTagColor, setEditingTagColor] = useState(TAG_COLORS[0]);
  const [metadataWriteError, setMetadataWriteError] = useState("");
  const pageDraftManaged =
    synopsisValue !== undefined || pageCssSource !== undefined || onSynopsisUpdate !== undefined;
  const contentMetadataBusy = synopsisBusy;
  const synopsisEditable = canEditContentMetadataValue(
    contentMetadataEditable && !contentMetadataBusy,
    Boolean(locked),
    pageDraftManaged,
    Boolean(onSynopsisUpdate && onSynopsisChange),
  );
  const cssSource = selectPageCssRecoverySource(
    pageDraftManaged ? pageCssSource : fallbackCssSource,
    spaceId,
    docId,
  );
  const cssSourceKey = cssSource ? JSON.stringify(cssSource) : "";
  useEffect(() => {
    setCssReview(null);
  }, [cssSourceKey]);
  const pageMetadataEditable = canEditPropertiesValue(pageValuesEditable, Boolean(locked));
  const schemaMetadataEditable = canEditPropertiesValue(schemaEditable, Boolean(locked));
  const pagePropertyValuesEditable = canEditLoadedMetadataValue(
    pageMetadataEditable,
    propertiesLoading,
    propertiesLoadError,
  );
  const propertySchemaEditable = canEditLoadedMetadataValue(
    schemaMetadataEditable,
    propertiesLoading,
    propertiesLoadError,
  );
  const pageTagMembershipEditable = canEditLoadedMetadataValue(
    pageMetadataEditable,
    tagsLoading,
    tagsLoadError,
  );
  const tagSchemaEditable = canEditLoadedMetadataValue(
    schemaMetadataEditable,
    tagsLoading,
    tagsLoadError,
  );

  const reloadPropertiesAndTags = useCallback(async () => {
    setPropertiesLoading(true);
    setTagsLoading(true);
    const [
      propertiesResult,
      tagsResult,
      availableTagsResult,
      nextDocuments,
      nextUsers,
      nextWorkflowTemplates,
      nextDocumentWorkflow,
      nextWorkflowEvents,
    ] = await Promise.all([
      loadMetadataCollection(() => dataClient.listDocumentProperties(spaceId, docId)),
      loadMetadataCollection(() => dataClient.listDocumentTags(spaceId, docId)),
      loadMetadataCollection(() => dataClient.listTags(spaceId)),
      dataClient.listDocuments(spaceId).catch(() => []),
      dataClient.listUserDirectory().catch(() => []),
      dataClient.listWorkflowTemplates(spaceId).catch(() => []),
      dataClient.getDocumentWorkflow(spaceId, docId).catch(() => null),
      dataClient.listDocumentWorkflowEvents(spaceId, docId).catch(() => []),
    ]);
    if (!propertiesResult.error) setProperties(propertiesResult.items);
    if (!tagsResult.error) setTags(tagsResult.items);
    if (!availableTagsResult.error) setAvailableTags(availableTagsResult.items);
    setPropertiesLoadError(propertiesResult.error);
    setTagsLoadError(tagsResult.error ?? availableTagsResult.error);
    setPropertiesLoading(false);
    setTagsLoading(false);
    setRelationDocuments(nextDocuments.filter((doc) => doc.id !== docId));
    setUserDirectory(nextUsers);
    setWorkflowTemplates(nextWorkflowTemplates);
    setDocumentWorkflow(nextDocumentWorkflow);
    setWorkflowEvents(nextWorkflowEvents);
    setSelectedWorkflowTemplateId(
      nextDocumentWorkflow?.workflow_template_id ?? nextWorkflowTemplates[0]?.id ?? "",
    );
  }, [docId, spaceId]);

  useEffect(() => {
    void reloadPropertiesAndTags();
  }, [reloadPropertiesAndTags]);

  useEffect(() => {
    if (pageDraftManaged) {
      setDocumentContent("");
      setSynopsis(synopsisValue ?? "");
      setFallbackCssSource(undefined);
      return;
    }
    let cancelled = false;
    setFallbackCssSource(undefined);
    const requireSameSession = capturePageMetadataLoadGuard();
    dataClient
      .getDocumentContent(spaceId, docId)
      .then((record) => {
        if (cancelled) return;
        requireSameSession();
        if (record.id !== docId) throw new Error("Page source changed during loading");
        setDocumentContent(record.content);
        setSynopsis(readSynopsis(record.content));
        setFallbackCssSource({
          spaceId,
          documentId: docId,
          provenance: inspectPageCssSource(record.content).provenance,
        });
      })
      .catch(() => {
        if (!cancelled) {
          setDocumentContent("");
          setSynopsis("");
          setFallbackCssSource(undefined);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [docId, pageDraftManaged, spaceId, synopsisValue]);

  const saveSynopsis = async () => {
    if (!synopsisEditable) return;
    setSynopsisBusy(true);
    setSynopsisError("");
    try {
      await persistContentMetadataValue(
        synopsis,
        pageDraftManaged,
        onSynopsisUpdate,
        async (nextSynopsis) => {
          const nextContent = writeSynopsis(documentContent, nextSynopsis);
          await dataClient.updateDocumentContent(spaceId, docId, nextContent);
          setDocumentContent(nextContent);
          emitGraphDirty(spaceId, "document-content-updated");
          // Managed drafts already own every accepted keystroke. Broadcasting
          // an older blur result would overwrite newer owned input after ACK.
          window.dispatchEvent(
            new CustomEvent(SYNOPSIS_UPDATED_EVENT, {
              detail: { spaceId, docId, synopsis: nextSynopsis },
            }),
          );
        },
      );
    } catch (error) {
      setSynopsisError(error instanceof Error ? error.message : "Unable to save page summary.");
    } finally {
      setSynopsisBusy(false);
    }
  };

  const updateProperty = async (propertyId: string, value: string) => {
    if (!pagePropertyValuesEditable) return;
    const previousValue = properties.find((prop) => prop.property_id === propertyId)?.value ?? null;
    setMetadataWriteError("");
    setProperties((prev) =>
      prev.map((prop) => (prop.property_id === propertyId ? { ...prop, value } : prop)),
    );
    await persistWithRollback(
      () =>
        dataClient.setDocumentPropertyValues(spaceId, docId, [{ property_id: propertyId, value }]),
      () =>
        setProperties((current) =>
          current.map((prop) =>
            prop.property_id === propertyId && prop.value === value
              ? { ...prop, value: previousValue }
              : prop,
          ),
        ),
      setMetadataWriteError,
      "Could not save this page property. The previous value was restored. Try again.",
    );
  };

  const createProperty = async () => {
    if (!propertySchemaEditable) return;
    const name = newPropertyName.trim();
    if (!name) return;
    setPropertyComposerBusy(true);
    setPropertyComposerError("");
    try {
      await dataClient.createPropertySchema(spaceId, {
        name,
        property_type: newPropertyType,
        options: propertyTypeUsesOptions(newPropertyType)
          ? parsePropertyOptions(newPropertyOptions)
          : undefined,
      });
      setNewPropertyName("");
      setNewPropertyType("text");
      setNewPropertyOptions("");
      setPropertyComposerOpen(false);
      await reloadPropertiesAndTags();
    } catch (error) {
      setPropertyComposerError(
        error instanceof Error ? error.message : "Unable to create this property.",
      );
    } finally {
      setPropertyComposerBusy(false);
    }
  };

  const beginEditProperty = (prop: DocumentPropertyValueRecord) => {
    setEditingPropertyId(prop.property_id);
    setEditingPropertyName(prop.property_name);
    setEditingPropertyType(prop.property_type);
    setEditingPropertyOptions(formatPropertyOptions(prop.options));
  };

  const savePropertyEdit = async () => {
    if (!propertySchemaEditable || !editingPropertyId || !editingPropertyName.trim()) return;
    setMetadataWriteError("");
    try {
      await dataClient.updatePropertySchema(spaceId, editingPropertyId, {
        name: editingPropertyName,
        property_type: editingPropertyType,
        options: propertyTypeUsesOptions(editingPropertyType)
          ? parsePropertyOptions(editingPropertyOptions)
          : undefined,
      });
    } catch (error) {
      setMetadataWriteError(
        error instanceof Error
          ? `Could not update this property. ${error.message}`
          : "Could not update this property. Try again.",
      );
      return;
    }
    setEditingPropertyId(null);
    setEditingPropertyName("");
    setEditingPropertyType("text");
    setEditingPropertyOptions("");
    await reloadPropertiesAndTags();
  };

  const deleteProperty = (propertyId: string) => {
    const property = properties.find((prop) => prop.property_id === propertyId);
    setDeletePropertyTarget({ id: propertyId, name: property?.property_name ?? "" });
  };

  const confirmDeleteProperty = async () => {
    if (!propertySchemaEditable || !deletePropertyTarget) return;
    setMetadataWriteError("");
    try {
      await dataClient.deletePropertySchema(spaceId, deletePropertyTarget.id);
    } catch (error) {
      setMetadataWriteError(
        error instanceof Error
          ? `Could not delete this property. ${error.message}`
          : "Could not delete this property. Try again.",
      );
      return;
    }
    setDeletePropertyTarget(null);
    await reloadPropertiesAndTags();
  };

  const moveProperty = async (index: number, direction: -1 | 1) => {
    if (!propertySchemaEditable) return;
    const nextIndex = index + direction;
    if (nextIndex < 0 || nextIndex >= properties.length) return;
    const nextProperties = [...properties];
    [nextProperties[index], nextProperties[nextIndex]] = [
      nextProperties[nextIndex],
      nextProperties[index],
    ];
    setMetadataWriteError("");
    const previousProperties = properties;
    setProperties(nextProperties);
    await persistWithRollback(
      () =>
        dataClient.reorderPropertySchemas(spaceId, {
          property_ids: nextProperties.map((prop) => prop.property_id),
        }),
      () => setProperties(previousProperties),
      setMetadataWriteError,
      "Could not reorder properties. The previous order was restored. Try again.",
    );
    await reloadPropertiesAndTags();
  };

  const addTag = async (selectedTag?: TagRecord) => {
    if (!pageTagMembershipEditable || (!selectedTag && !tagSchemaEditable)) return;
    const name = newTagName.trim();
    if (!selectedTag && !name) return;
    setTagComposerBusy(true);
    setTagComposerError("");
    try {
      const existing =
        selectedTag ?? availableTags.find((tag) => tag.name.toLowerCase() === name.toLowerCase());
      const tag = existing ?? (await dataClient.createTag(spaceId, { name, color: TAG_COLORS[0] }));
      await dataClient.addDocumentTag(spaceId, docId, tag.id);
      setNewTagName("");
      setTagComposerOpen(false);
      setTagInputFocused(false);
      setTags((current) =>
        current.some((item) => item.id === tag.id) ? current : [...current, tag],
      );
      setAvailableTags((current) =>
        current.some((item) => item.id === tag.id) ? current : [...current, tag],
      );
      await reloadPropertiesAndTags();
    } catch (error) {
      setTagComposerError(error instanceof Error ? error.message : "Unable to add this tag.");
    } finally {
      setTagComposerBusy(false);
    }
  };

  const updateTagColor = async (tag: TagRecord, color: string) => {
    if (!tagSchemaEditable) return;
    setMetadataWriteError("");
    try {
      await dataClient.updateTag(spaceId, tag.id, { name: tag.name, color });
      setActiveTagColorMenuId(null);
      await reloadPropertiesAndTags();
    } catch (error) {
      setMetadataWriteError(
        error instanceof Error
          ? `Could not update this tag. ${error.message}`
          : "Could not update this tag. Try again.",
      );
    }
  };

  const removeTag = async (tagId: string) => {
    if (!pageTagMembershipEditable) return;
    setMetadataWriteError("");
    try {
      await dataClient.removeDocumentTag(spaceId, docId, tagId);
      await reloadPropertiesAndTags();
    } catch (error) {
      setMetadataWriteError(
        error instanceof Error
          ? `Could not remove this page tag. ${error.message}`
          : "Could not remove this page tag. Try again.",
      );
    }
  };

  const openTagSearch = (tag: TagRecord) => {
    document.dispatchEvent(
      new CustomEvent("starlane:open-search", {
        detail: { tag: tag.name, spaceId },
      }),
    );
  };

  const beginEditTag = (tag: TagRecord) => {
    setEditingTagId(tag.id);
    setEditingTagName(tag.name);
    setEditingTagColor(tag.color);
  };

  const saveTagEdit = async () => {
    if (!tagSchemaEditable || !editingTagId || !editingTagName.trim()) return;
    setMetadataWriteError("");
    try {
      await dataClient.updateTag(spaceId, editingTagId, {
        name: editingTagName,
        color: editingTagColor,
      });
    } catch (error) {
      setMetadataWriteError(
        error instanceof Error
          ? `Could not update this tag. ${error.message}`
          : "Could not update this tag. Try again.",
      );
      return;
    }
    setEditingTagId(null);
    setEditingTagName("");
    setEditingTagColor(TAG_COLORS[0]);
    await reloadPropertiesAndTags();
  };

  const enableWorkflow = async () => {
    if (!selectedWorkflowTemplateId) return;
    setWorkflowBusy(true);
    setWorkflowError("");
    try {
      const workflow = await dataClient.enableDocumentWorkflow(spaceId, docId, {
        template_id: selectedWorkflowTemplateId,
      });
      setDocumentWorkflow(workflow);
      setSelectedWorkflowTemplateId(workflow.workflow_template_id);
      setWorkflowEvents(
        await dataClient.listDocumentWorkflowEvents(spaceId, docId).catch(() => []),
      );
    } catch (error) {
      setWorkflowError(error instanceof Error ? error.message : "Unable to enable workflow");
    } finally {
      setWorkflowBusy(false);
    }
  };

  const disableWorkflow = async () => {
    setWorkflowBusy(true);
    setWorkflowError("");
    try {
      await dataClient.disableDocumentWorkflow(spaceId, docId);
      setDocumentWorkflow(null);
      setWorkflowEvents([]);
    } catch (error) {
      setWorkflowError(error instanceof Error ? error.message : "Unable to disable workflow");
    } finally {
      setWorkflowBusy(false);
    }
  };

  const transitionWorkflow = async (stageId: string) => {
    if (!documentWorkflow || stageId === documentWorkflow.current_stage_id) return;
    setWorkflowBusy(true);
    setWorkflowError("");
    try {
      const workflow = await dataClient.transitionDocumentWorkflow(spaceId, docId, {
        stage_id: stageId,
      });
      setDocumentWorkflow(workflow);
      setWorkflowEvents(
        await dataClient.listDocumentWorkflowEvents(spaceId, docId).catch(() => []),
      );
    } catch (error) {
      setWorkflowError(error instanceof Error ? error.message : "Unable to move workflow");
    } finally {
      setWorkflowBusy(false);
    }
  };

  const unassignedTags = availableTags.filter(
    (tag) => !tags.some((assigned) => assigned.id === tag.id),
  );
  const matchingTags = getMatchingTags(unassignedTags, newTagName);

  return (
    <div className="border-border border-b">
      {deletePropertyTarget && (
        <ConfirmDialog
          labelledBy="delete-property-title"
          title="Delete property"
          message={getPropertyDeleteConfirmationMessage(deletePropertyTarget.name)}
          confirmLabel="Delete"
          destructive
          helperText={metadataWriteError || undefined}
          onCancel={() => setDeletePropertyTarget(null)}
          onConfirm={() => void confirmDeleteProperty()}
        />
      )}
      {cssReview && cssSource && cssReview.key === cssSourceKey && (
        <PageCssRecoveryContent source={cssReview.source} onClose={() => setCssReview(null)} />
      )}
      {tagComposerOpen && (
        <DialogShell
          labelledBy="add-document-tag-title"
          maxWidth="sm"
          onClose={() => {
            if (tagComposerBusy) return;
            setTagComposerOpen(false);
            setNewTagName("");
            setTagComposerError("");
          }}
        >
          <div className="border-border flex items-start justify-between gap-4 border-b px-5 py-4">
            <div>
              <h2 id="add-document-tag-title" className="text-text text-base font-semibold">
                Add tag
              </h2>
              <p className="text-text-muted mt-1 text-sm">
                Find an existing space tag or create a new one.
              </p>
            </div>
            <button
              type="button"
              aria-label="Close add tag dialog"
              className="focus-ring text-text-muted hover:bg-surface-raised hover:text-text rounded-button flex h-8 w-8 shrink-0 items-center justify-center"
              onClick={() => {
                setTagComposerOpen(false);
                setNewTagName("");
                setTagComposerError("");
              }}
              disabled={tagComposerBusy}
            >
              <X size={16} />
            </button>
          </div>
          <form
            className="max-h-[calc(100dvh-8rem)] overflow-y-auto"
            onSubmit={(event) => {
              event.preventDefault();
              void addTag();
            }}
          >
            <div className="grid gap-4 px-5 py-4">
              <label htmlFor="document-tag-query" className="text-text text-sm font-medium">
                Tag
              </label>
              <div className="relative">
                <Search
                  size={16}
                  className="text-text-muted pointer-events-none absolute top-1/2 left-3 -translate-y-1/2"
                />
                <Input
                  id="document-tag-query"
                  value={newTagName}
                  onChange={(event) => {
                    setNewTagName(event.target.value);
                    setTagComposerError("");
                  }}
                  placeholder="Search or create tag"
                  className="h-10 pl-9 text-sm"
                  autoFocus
                  autoComplete="off"
                />
              </div>

              <div className="border-border bg-bg rounded-input max-h-60 overflow-y-auto border">
                {matchingTags.length > 0 ? (
                  <div role="listbox" aria-label="Existing tags" className="divide-border divide-y">
                    {matchingTags.map((tag) => (
                      <button
                        key={tag.id}
                        type="button"
                        role="option"
                        aria-selected="false"
                        className="hover:bg-surface-raised focus-ring flex min-h-11 w-full items-center gap-3 px-3 py-2 text-left"
                        onClick={() => void addTag(tag)}
                        disabled={tagComposerBusy}
                      >
                        <span
                          className="border-border h-3 w-3 shrink-0 rounded-sm border"
                          style={{ backgroundColor: tag.color }}
                        />
                        <span className="text-text min-w-0 flex-1 truncate text-sm">
                          {tag.name}
                        </span>
                        <span className="text-text-muted text-xs">Existing</span>
                      </button>
                    ))}
                  </div>
                ) : (
                  <div className="text-text-muted px-3 py-4 text-sm">
                    {newTagName.trim()
                      ? `No existing tag named "${newTagName.trim()}".`
                      : "No unassigned tags in this space."}
                  </div>
                )}
              </div>

              {tagComposerError && (
                <div role="alert" className={PROPERTIES_PANEL_ERROR_CLASS}>
                  {tagComposerError}
                </div>
              )}
            </div>
            <div className="border-border bg-bg/35 flex items-center justify-end gap-2 border-t px-5 py-3">
              <Button
                type="button"
                variant="secondary"
                onClick={() => {
                  setTagComposerOpen(false);
                  setNewTagName("");
                  setTagComposerError("");
                }}
                disabled={tagComposerBusy}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={!newTagName.trim() || tagComposerBusy}>
                {tagComposerBusy ? "Adding..." : "Add tag"}
              </Button>
            </div>
          </form>
        </DialogShell>
      )}
      {propertyComposerOpen && (
        <DialogShell
          labelledBy="create-property-title"
          maxWidth="lg"
          onClose={() => {
            if (propertyComposerBusy) return;
            setPropertyComposerOpen(false);
            setPropertyComposerError("");
          }}
        >
          <div className="border-border flex items-start justify-between gap-4 border-b px-5 py-4">
            <div>
              <h2 id="create-property-title" className="text-text text-base font-semibold">
                Create custom property
              </h2>
              <p className="text-text-muted mt-1 text-sm">
                Add structured metadata that every page in this space can use.
              </p>
            </div>
            <button
              type="button"
              aria-label="Close create property dialog"
              className="focus-ring text-text-muted hover:bg-surface-raised hover:text-text rounded-button flex h-8 w-8 shrink-0 items-center justify-center"
              onClick={() => {
                setPropertyComposerOpen(false);
                setPropertyComposerError("");
              }}
              disabled={propertyComposerBusy}
            >
              <X size={16} />
            </button>
          </div>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void createProperty();
            }}
          >
            <div className="grid gap-4 p-5">
              <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_13rem]">
                <Input
                  id="new-property-name"
                  value={newPropertyName}
                  onChange={(event) => {
                    setNewPropertyName(event.target.value);
                    setPropertyComposerError("");
                  }}
                  placeholder="Property name"
                  aria-label="Property name"
                  className="h-10"
                  autoFocus
                />
                <PropertyTypeSelect
                  value={newPropertyType}
                  onChange={setNewPropertyType}
                  className="h-10"
                />
              </div>
              {propertyTypeUsesOptions(newPropertyType) && (
                <div className="grid gap-2">
                  <label htmlFor="new-property-options" className="text-text text-sm font-medium">
                    Options
                  </label>
                  <Input
                    id="new-property-options"
                    value={newPropertyOptions}
                    onChange={(event) => setNewPropertyOptions(event.target.value)}
                    placeholder="Todo, In progress, Done"
                    className="h-10 text-sm"
                  />
                  <p className="text-text-muted text-xs">Separate each option with a comma.</p>
                </div>
              )}
              {propertyComposerError && (
                <div role="alert" className={PROPERTIES_PANEL_ERROR_CLASS}>
                  {propertyComposerError}
                </div>
              )}
            </div>
            <div className="border-border bg-bg/35 flex items-center justify-end gap-2 border-t px-5 py-3">
              <Button
                type="button"
                variant="secondary"
                onClick={() => {
                  setPropertyComposerOpen(false);
                  setPropertyComposerError("");
                }}
                disabled={propertyComposerBusy}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={!newPropertyName.trim() || propertyComposerBusy}>
                {propertyComposerBusy ? "Creating..." : "Create property"}
              </Button>
            </div>
          </form>
        </DialogShell>
      )}
      {/* Header */}
      <button
        onClick={() => setExpanded(!expanded)}
        className="text-text hover:bg-surface-raised flex w-full items-center gap-1 px-4 py-2 text-sm font-medium"
      >
        {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        Properties
      </button>

      {expanded && (
        <div className="px-4 pb-3 text-sm">
          <PagePreservedMetadata spaceId={spaceId} documentId={docId} />
          {/* Built-in properties */}
          <PropertyRow label="Title" value={title} />
          {icon && <PropertyRow label="Icon" value={icon} />}
          <PropertyRow label="Created Date" value={formatDate(createdAt)} />
          <PropertyRow label="Created By" value={formatUserValue(createdBy, userDirectory)} />
          <PropertyRow label="Updated By" value={formatUserValue(lastUpdatedBy, userDirectory)} />
          <PropertyRow label="Last Updated" value={formatDate(updatedAt)} />
          <PropertyRow label="Locked" value={locked ? "Yes" : "No"} />
          <PropertyRow label="Public" value={isPublic ? "Yes" : "No"} />
          <PageMaintenanceControl
            key={`maintenance/${spaceId}/${docId}`}
            load={loadMaintenance}
            save={saveMaintenance}
          />
          <PageIdentifierControl
            key={`${spaceId}/${docId}`}
            load={loadIdentifier}
            save={saveIdentifier}
          />

          <div className="border-border mt-3 border-t pt-3">
            <div className="text-text-muted mb-2 text-xs">Cover image</div>
            <Suspense fallback={null}>
              <CoverImage
                spaceId={spaceId}
                documentId={docId}
                src={coverImage}
                draftId={coverDraftId}
                compact
                editable={coverEditable && Boolean(onCoverUpdate)}
                showEmptyPlaceholder={coverEditable && Boolean(onCoverUpdate)}
                onUpload={onCoverUpload}
                onUpdate={onCoverUpdate ?? (() => undefined)}
              />
            </Suspense>
            {coverEditable && nativeCoverRecovery && (
              <NativeLocalCoverRecoveryControl actions={nativeCoverRecovery} />
            )}
            {!coverEditable && !coverImage && (
              <p className="text-text-muted text-xs">Open an editable draft to add a cover.</p>
            )}
          </div>

          <div className="border-border mt-3 border-t pt-2">
            <div className="text-text-muted mb-2 flex items-center gap-1.5 text-xs">
              <Workflow size={13} />
              <span>Workflow</span>
            </div>
            {documentWorkflow ? (
              <div className="rounded-input border-border bg-surface-raised border p-2">
                <div className="flex items-center justify-between gap-2">
                  <div className="min-w-0">
                    <div className="text-text truncate text-sm font-medium">
                      {documentWorkflow.workflow_template_name}
                    </div>
                    <div className="text-text-muted mt-0.5 text-xs">
                      {documentWorkflow.current_stage_name} / {documentWorkflow.status}
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => void disableWorkflow()}
                    disabled={workflowBusy}
                    className="rounded-button border-border text-text-muted hover:bg-bg hover:text-text border px-2 py-1 text-xs disabled:opacity-50"
                  >
                    Disable
                  </button>
                </div>
                <div className="mt-2 flex flex-wrap gap-1">
                  {documentWorkflow.stages.map((stage) => (
                    <button
                      type="button"
                      key={stage.id}
                      onClick={() => void transitionWorkflow(stage.id)}
                      disabled={workflowBusy || stage.id === documentWorkflow.current_stage_id}
                      className={`rounded-full border px-2 py-0.5 text-[11px] ${
                        stage.id === documentWorkflow.current_stage_id
                          ? "border-primary bg-primary/15 text-primary"
                          : "border-border text-text-muted hover:bg-bg hover:text-text"
                      }`}
                      title={`Move workflow to ${stage.name}`}
                    >
                      {stage.name}
                    </button>
                  ))}
                </div>
                {workflowError && (
                  <div className={`mt-2 ${PROPERTIES_PANEL_ERROR_CLASS}`}>{workflowError}</div>
                )}
                {workflowEvents.length > 0 && (
                  <div className="border-border mt-3 border-t pt-2">
                    <div className="text-text-muted mb-1 text-xs font-medium">Recent activity</div>
                    <div className="grid gap-1.5">
                      {workflowEvents.slice(0, 5).map((event) => (
                        <div key={event.id} className="text-text-muted text-[11px] leading-4">
                          <div>
                            {workflowEventLabel(documentWorkflow, event)}
                            <span className="text-text-faint">
                              {" "}
                              / {formatDate(event.created_at)}
                            </span>
                          </div>
                          <div className="text-text-faint">
                            {formatUserValue(event.created_by, userDirectory)}
                          </div>
                          {event.note && <div className="text-text">{event.note}</div>}
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            ) : workflowTemplates.length > 0 ? (
              <div className="grid gap-2">
                <Select
                  value={selectedWorkflowTemplateId}
                  onChange={(event) => setSelectedWorkflowTemplateId(event.target.value)}
                  className="bg-bg h-8 px-2 text-xs"
                  aria-label="Workflow template"
                >
                  {workflowTemplates.map((template) => (
                    <option key={template.id} value={template.id}>
                      {template.name}
                    </option>
                  ))}
                </Select>
                <button
                  type="button"
                  onClick={() => void enableWorkflow()}
                  disabled={!selectedWorkflowTemplateId || workflowBusy}
                  className="rounded-button border-border bg-surface-raised text-text hover:bg-bg h-8 border px-3 text-xs disabled:opacity-50"
                >
                  Enable workflow
                </button>
                {workflowError && (
                  <div className={PROPERTIES_PANEL_ERROR_CLASS}>{workflowError}</div>
                )}
              </div>
            ) : (
              <div className="text-text-muted text-xs">No workflow templates</div>
            )}
          </div>

          <div className="border-border mt-3 border-t pt-2">
            <label htmlFor="properties-panel-synopsis" className="text-text-muted text-xs">
              Synopsis
            </label>
            <p className="text-text-muted mt-1 text-[11px] leading-4">
              Add a short description for Page View, search previews, and future publishing
              surfaces.
            </p>
            <Textarea
              value={synopsis}
              id="properties-panel-synopsis"
              aria-label="Page synopsis"
              onChange={(event) => {
                try {
                  acceptSynopsisInput(
                    event.target.value,
                    synopsisEditable,
                    pageDraftManaged,
                    onSynopsisChange,
                  );
                  setSynopsis(event.target.value);
                  setSynopsisError("");
                } catch (error) {
                  setSynopsisError(
                    error instanceof Error
                      ? error.message
                      : "Original synopsis draft is unavailable.",
                  );
                }
              }}
              onCompositionStart={() => {
                try {
                  onSynopsisCompositionChange?.(true);
                  synopsisComposing.current = true;
                } catch (error) {
                  setSynopsisError(
                    error instanceof Error
                      ? error.message
                      : "Original synopsis draft is unavailable.",
                  );
                }
              }}
              onCompositionEnd={() => {
                try {
                  onSynopsisCompositionChange?.(false);
                  synopsisComposing.current = false;
                } catch (error) {
                  setSynopsisError(
                    error instanceof Error
                      ? error.message
                      : "Original synopsis draft is unavailable.",
                  );
                }
              }}
              onBlur={() => void saveSynopsis()}
              placeholder="Add a concise page summary..."
              rows={4}
              className="bg-surface mt-2 resize-none text-sm leading-5"
              disabled={!synopsisEditable}
            />
            {!synopsisEditable && (
              <p className="text-text-muted mt-1 text-[11px] leading-4">
                Open an editable draft to change the page summary.
              </p>
            )}
            {synopsisError && (
              <div role="alert" className={`${PROPERTIES_PANEL_ERROR_CLASS} mt-1`}>
                {synopsisError}
              </div>
            )}
          </div>

          <div className="border-border mt-3 border-t pt-2">
            <span className="text-text-muted text-xs">Tags</span>
            {metadataWriteError && (
              <div role="alert" className={`${PROPERTIES_PANEL_ERROR_CLASS} mt-2`}>
                {metadataWriteError}
              </div>
            )}
            <div className="mt-1 flex flex-wrap gap-1">
              {tagsLoading && (
                <span role="status" className="text-text-muted text-xs">
                  Loading page tags…
                </span>
              )}
              {!tagsLoading && !tagsLoadError && tags.length === 0 && (
                <span className="text-text-muted text-xs">No tags</span>
              )}
              {tagsLoadError && (
                <MetadataLoadError
                  collection="page tags"
                  kind={tagsLoadError}
                  onRetry={() => void reloadPropertiesAndTags()}
                />
              )}
              {tags.map((tag) => (
                <span
                  key={tag.id}
                  className="border-border bg-surface-raised text-text relative inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs"
                  style={{ backgroundColor: `${tag.color}20`, color: tag.color }}
                >
                  <ColorSwatchMenu
                    open={activeTagColorMenuId === tag.id}
                    value={tag.color}
                    onSelect={(color) => void updateTagColor(tag, color)}
                    onOpenChange={(open) => setActiveTagColorMenuId(open ? tag.id : null)}
                  >
                    <button
                      type="button"
                      disabled={!tagSchemaEditable}
                      className="border-border h-3.5 w-3.5 rounded-sm border"
                      style={{ backgroundColor: tag.color }}
                      aria-label={`Change ${tag.name} color`}
                      title={`Change ${tag.name} color`}
                    />
                  </ColorSwatchMenu>
                  <button
                    type="button"
                    onClick={() => openTagSearch(tag)}
                    className="inline-flex max-w-28 items-center gap-0.5 truncate hover:underline"
                    title={`Search ${tag.name}`}
                  >
                    <Hash size={9} />
                    <span className="truncate">{tag.name}</span>
                  </button>
                  <button
                    type="button"
                    disabled={!tagSchemaEditable}
                    onClick={() => beginEditTag(tag)}
                    title={`Edit ${tag.name}`}
                  >
                    <Pencil size={10} />
                  </button>
                  <button
                    type="button"
                    disabled={!pageTagMembershipEditable}
                    onClick={() => removeTag(tag.id)}
                    title={`Remove ${tag.name}`}
                    aria-label={`Remove ${tag.name}`}
                    className="text-text-muted hover:text-danger"
                  >
                    <X size={10} />
                  </button>
                </span>
              ))}
            </div>
            {!pageMetadataEditable && (
              <p className="text-text-muted mt-1 text-[11px] leading-4">
                Open an editable, unlocked page to change page tags.
              </p>
            )}
            {pageMetadataEditable && !schemaMetadataEditable && (
              <p className="text-text-muted mt-1 text-[11px] leading-4">
                You can use existing tags. Your space role must allow schema changes to create or
                edit tags.
              </p>
            )}
            <div className="mt-2 grid gap-2">
              {editingTagId && (
                <div className="rounded-input border-border grid gap-2 border p-2">
                  <Input
                    value={editingTagName}
                    disabled={!tagSchemaEditable}
                    onChange={(event) => setEditingTagName(event.target.value)}
                    placeholder="Tag name"
                    className="h-7 text-xs"
                  />
                  <div className="flex items-center gap-1">
                    <ColorPicker
                      value={editingTagColor}
                      onChange={setEditingTagColor}
                      disabled={!tagSchemaEditable}
                    />
                    <IconButton
                      title="Save tag"
                      onClick={saveTagEdit}
                      disabled={!tagSchemaEditable}
                      icon={<Check size={14} />}
                    />
                    <IconButton
                      title="Cancel tag edit"
                      onClick={() => setEditingTagId(null)}
                      icon={<X size={14} />}
                    />
                  </div>
                </div>
              )}
              <form
                className="border-border bg-surface rounded-input relative flex min-h-10 flex-wrap items-center gap-1.5 border px-2 py-1.5"
                onSubmit={(event) => {
                  event.preventDefault();
                  void addTag();
                }}
              >
                <Search size={14} className="text-text-muted shrink-0" aria-hidden="true" />
                <Input
                  value={newTagName}
                  disabled={!pageTagMembershipEditable}
                  onChange={(event) => {
                    setNewTagName(event.target.value);
                    setTagComposerError("");
                  }}
                  onFocus={() => setTagInputFocused(true)}
                  onBlur={() => window.setTimeout(() => setTagInputFocused(false), 120)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      void addTag();
                    }
                  }}
                  placeholder="Add or search tags"
                  className="text-text placeholder:text-text-muted/70 h-7 min-w-20 flex-1 rounded-none border-transparent bg-transparent px-1 py-1 text-xs outline-none focus:ring-0 focus:ring-offset-0"
                  aria-label="Add or search tags"
                  autoComplete="off"
                />
                <button
                  type="submit"
                  className="bg-primary text-primary-foreground focus-ring rounded-button flex h-7 w-7 shrink-0 items-center justify-center disabled:cursor-not-allowed disabled:opacity-40"
                  disabled={
                    !newTagName.trim() ||
                    tagComposerBusy ||
                    !pageTagMembershipEditable ||
                    !tagSchemaEditable
                  }
                  aria-label="Add tag"
                  title="Add tag"
                >
                  <Plus size={14} />
                </button>
                {shouldShowInlineTagSuggestions(
                  tagInputFocused,
                  newTagName,
                  matchingTags,
                  tagComposerBusy,
                ) && (
                  <div className="border-border bg-surface-raised shadow-card rounded-input absolute inset-x-0 top-[calc(100%+0.35rem)] z-30 max-h-52 overflow-y-auto border p-1">
                    {matchingTags.map((tag) => (
                      <button
                        key={tag.id}
                        type="button"
                        className="hover:bg-surface rounded-button flex min-h-9 w-full items-center gap-2 px-2 text-left text-xs"
                        onMouseDown={(event) => event.preventDefault()}
                        onClick={() => void addTag(tag)}
                        disabled={tagComposerBusy || !pageTagMembershipEditable}
                      >
                        <span
                          className="border-border h-3 w-3 rounded-sm border"
                          style={{ backgroundColor: tag.color }}
                        />
                        <span className="text-text min-w-0 flex-1 truncate">{tag.name}</span>
                        <span className="text-text-muted">Add</span>
                      </button>
                    ))}
                  </div>
                )}
              </form>
              {tagComposerError && (
                <div role="alert" className={PROPERTIES_PANEL_ERROR_CLASS}>
                  {tagComposerError}
                </div>
              )}
            </div>
          </div>

          {/* Custom properties */}
          <div className="border-border mt-3 border-t pt-2">
            <span className="text-text-muted text-xs">Custom properties</span>
            {propertiesLoading && (
              <div role="status" className="text-text-muted mt-1 text-xs">
                Loading custom properties…
              </div>
            )}
            {propertiesLoadError && (
              <MetadataLoadError
                collection="custom properties"
                kind={propertiesLoadError}
                onRetry={() => void reloadPropertiesAndTags()}
              />
            )}
            {!propertiesLoading && !propertiesLoadError && properties.length === 0 && (
              <div className="text-text-muted mt-1 text-xs">No custom properties</div>
            )}
            {properties.map((prop, index) => (
              <div key={prop.property_id} className="mt-2">
                {editingPropertyId === prop.property_id ? (
                  <div className="rounded-input border-border mb-1 grid gap-1 border p-2">
                    <Input
                      value={editingPropertyName}
                      disabled={!propertySchemaEditable}
                      onChange={(event) => setEditingPropertyName(event.target.value)}
                      className="h-7 text-xs"
                    />
                    <div className="flex gap-1">
                      <PropertyTypeSelect
                        value={editingPropertyType}
                        onChange={setEditingPropertyType}
                        disabled={!propertySchemaEditable}
                      />
                      <IconButton
                        title="Save property"
                        onClick={savePropertyEdit}
                        disabled={!propertySchemaEditable}
                        icon={<Check size={14} />}
                      />
                      <IconButton
                        title="Cancel property edit"
                        onClick={() => setEditingPropertyId(null)}
                        icon={<X size={14} />}
                      />
                    </div>
                    {propertyTypeUsesOptions(editingPropertyType) && (
                      <Input
                        value={editingPropertyOptions}
                        disabled={!propertySchemaEditable}
                        onChange={(event) => setEditingPropertyOptions(event.target.value)}
                        placeholder="Options separated by commas"
                        className="h-7 text-xs"
                      />
                    )}
                  </div>
                ) : (
                  <div className="mb-0.5 grid grid-cols-[1.25rem_minmax(0,1fr)_auto] items-center gap-2">
                    <span className="text-text-muted flex h-5 w-5 items-center justify-center">
                      <PropertyTypeIcon type={prop.property_type} />
                    </span>
                    <label className="text-text-muted block min-w-0 truncate text-xs">
                      {prop.property_name}
                    </label>
                    <div className="flex shrink-0 items-center gap-1">
                      <button
                        type="button"
                        onClick={() => moveProperty(index, -1)}
                        disabled={!propertySchemaEditable || index === 0}
                        className="text-text-muted hover:text-text disabled:opacity-30"
                        title={`Move ${prop.property_name} up`}
                      >
                        <ArrowUp size={12} />
                      </button>
                      <button
                        type="button"
                        onClick={() => moveProperty(index, 1)}
                        disabled={!propertySchemaEditable || index === properties.length - 1}
                        className="text-text-muted hover:text-text disabled:opacity-30"
                        title={`Move ${prop.property_name} down`}
                      >
                        <ArrowDown size={12} />
                      </button>
                      <button
                        type="button"
                        onClick={() => beginEditProperty(prop)}
                        disabled={!propertySchemaEditable}
                        className="text-text-muted hover:text-text"
                        title={`Edit ${prop.property_name}`}
                      >
                        <Pencil size={12} />
                      </button>
                      <button
                        type="button"
                        onClick={() => deleteProperty(prop.property_id)}
                        disabled={!propertySchemaEditable}
                        className="text-text-muted hover:text-text"
                        title={`Delete ${prop.property_name}`}
                      >
                        <Trash2 size={12} />
                      </button>
                    </div>
                  </div>
                )}
                <PropertyValueControl
                  property={prop}
                  relationDocuments={relationDocuments}
                  userDirectory={userDirectory}
                  editable={pagePropertyValuesEditable}
                  onChange={(value) => updateProperty(prop.property_id, value)}
                />
              </div>
            ))}
            <AddSurfaceButton
              label="Add custom property"
              disabled={!propertySchemaEditable}
              onClick={() => {
                setNewPropertyName("");
                setNewPropertyType("text");
                setNewPropertyOptions("");
                setPropertyComposerError("");
                setPropertyComposerOpen(true);
              }}
              className="mt-3"
            />
            {!schemaMetadataEditable && (
              <p className="text-text-muted mt-1 text-[11px] leading-4">
                Open an editable, unlocked page with space schema-edit access to change custom
                properties.
              </p>
            )}
          </div>

          {cssSource && (
            <div className="border-border mt-3 border-t pt-2">
              <span className="text-text-muted text-xs">Retained presentation</span>
              <Button
                ref={cssSourceTriggerRef}
                variant="secondary"
                size="sm"
                className="mt-2"
                onClick={() => {
                  setCssReview({ source: structuredClone(cssSource), key: cssSourceKey });
                }}
              >
                View retained CSS
              </Button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function AddSurfaceButton({
  label,
  onClick,
  disabled = false,
  className = "",
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={`focus-ring border-border bg-surface text-text-muted hover:bg-surface-raised hover:text-text rounded-input flex h-11 w-full items-center justify-center border disabled:cursor-not-allowed disabled:opacity-50 ${className}`}
      aria-label={label}
      title={label}
    >
      <Plus size={21} strokeWidth={1.75} />
    </button>
  );
}

function MetadataLoadError({
  collection,
  kind,
  onRetry,
}: {
  collection: string;
  kind: "permission" | "unavailable";
  onRetry: () => void;
}) {
  if (kind === "permission") {
    return (
      <div role="alert" className={`${PROPERTIES_PANEL_ERROR_CLASS} mt-2`}>
        You don’t have permission to view {collection}. Ask a space owner to review your access.
      </div>
    );
  }

  return (
    <div role="alert" className={`${PROPERTIES_PANEL_ERROR_CLASS} mt-2 grid gap-2`}>
      <span>Unable to load {collection}.</span>
      <Button
        type="button"
        variant="secondary"
        size="sm"
        className="justify-self-start"
        onClick={onRetry}
      >
        Retry
      </Button>
    </div>
  );
}

function IconButton({
  title,
  onClick,
  icon,
  disabled = false,
}: {
  title: string;
  onClick: () => void;
  icon: React.ReactNode;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center border disabled:opacity-40"
      title={title}
    >
      {icon}
    </button>
  );
}

export function PropertyTypeSelect({
  value,
  onChange,
  disabled = false,
  className = "h-7 text-xs",
}: {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  className?: string;
}) {
  const options = PROPERTY_TYPE_OPTIONS;
  const selected = options.find(([optionValue]) => optionValue === value) ?? options[0];

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          disabled={disabled}
          aria-label="Property type"
          aria-haspopup="menu"
          className={`focus-ring border-border bg-bg text-text hover:border-text-muted rounded-input flex min-w-0 flex-1 items-center justify-between gap-2 border px-2 text-left transition-colors duration-100 ${className}`}
        >
          <span className="truncate">{selected[1]}</span>
          <ChevronDown aria-hidden="true" className="text-text-muted shrink-0" size={14} />
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        sideOffset={4}
        className="w-[min(14rem,calc(100vw-2rem))] p-1"
        role="menu"
        aria-label="Property types"
        onPointerDown={(event) => event.stopPropagation()}
        onClick={(event) => event.stopPropagation()}
      >
        {options.map(([optionValue, label]) => (
          <button
            key={optionValue}
            type="button"
            role="menuitemradio"
            aria-checked={optionValue === value}
            className={`focus-ring rounded-button flex min-h-9 w-full items-center justify-between gap-3 px-2.5 text-left text-sm transition-colors duration-100 ${optionValue === value ? "bg-primary/15 text-primary" : "text-text-muted hover:bg-surface-raised hover:text-text"}`}
            onClick={() => onChange(optionValue)}
          >
            <span>{label}</span>
            {optionValue === value ? <Check aria-hidden="true" size={14} /> : null}
          </button>
        ))}
      </PopoverContent>
    </Popover>
  );
}

export function getMatchingTags(tags: TagRecord[], query: string): TagRecord[] {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  return [...tags]
    .filter((tag) => !normalizedQuery || tag.name.toLocaleLowerCase().includes(normalizedQuery))
    .sort((left, right) => {
      const leftName = left.name.toLocaleLowerCase();
      const rightName = right.name.toLocaleLowerCase();
      const leftStarts = normalizedQuery ? leftName.startsWith(normalizedQuery) : false;
      const rightStarts = normalizedQuery ? rightName.startsWith(normalizedQuery) : false;
      if (leftStarts !== rightStarts) return leftStarts ? -1 : 1;
      return left.name.localeCompare(right.name);
    });
}

export function shouldShowInlineTagSuggestions(
  inputFocused: boolean,
  query: string,
  matchingTags: TagRecord[],
  busy: boolean,
): boolean {
  return inputFocused && !busy && query.trim().length > 0 && matchingTags.length > 0;
}

export function PropertyValueControl({
  property,
  relationDocuments,
  userDirectory,
  onChange,
  editable = true,
}: {
  property: DocumentPropertyValueRecord;
  relationDocuments: DocumentRecord[];
  userDirectory: UserDirectoryRecord[];
  onChange: (value: string) => void;
  editable?: boolean;
}) {
  const options = getPropertyOptions(property.options);
  if (property.property_type === "checkbox") {
    return (
      <input
        type="checkbox"
        disabled={!editable}
        checked={property.value === "true"}
        onChange={(event) => onChange(String(event.target.checked))}
        aria-label={property.property_name}
        className="border-border accent-primary h-4 w-4 rounded"
      />
    );
  }

  if (property.property_type === "rating") {
    const rating = Math.max(0, Math.min(5, Number.parseInt(property.value || "0", 10) || 0));
    return (
      <div
        className="flex items-center gap-0.5"
        role="radiogroup"
        aria-label={property.property_name}
      >
        {Array.from({ length: 5 }, (_, index) => {
          const star = index + 1;
          const active = star <= rating;
          return (
            <button
              key={star}
              type="button"
              disabled={!editable}
              role="radio"
              aria-label={`${star} star${star === 1 ? "" : "s"}`}
              aria-checked={star === rating}
              className="focus-ring rounded-button text-primary hover:text-primary p-0.5 transition-colors duration-100"
              onClick={() => onChange(String(star === rating ? 0 : star))}
            >
              <Star aria-hidden="true" size={16} fill={active ? "currentColor" : "none"} />
            </button>
          );
        })}
      </div>
    );
  }

  if (property.property_type === "select") {
    return (
      <Select
        value={property.value || ""}
        disabled={!editable}
        onChange={(event) => onChange(event.target.value)}
        className="bg-bg h-7 px-2 text-xs"
        aria-label={property.property_name}
      >
        <option value="">None</option>
        {options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </Select>
    );
  }

  if (property.property_type === "multi_select") {
    const selected = new Set(parseMultiSelectValue(property.value));
    return (
      <div className="flex flex-wrap gap-1">
        {options.length === 0 && <span className="text-text-muted text-xs">No options</span>}
        {options.map((option) => {
          const checked = selected.has(option);
          return (
            <label
              key={option}
              className="border-border text-text inline-flex items-center gap-1 rounded border px-2 py-0.5 text-xs"
            >
              <input
                type="checkbox"
                disabled={!editable}
                checked={checked}
                aria-label={`${property.property_name}: ${option}`}
                onChange={(event) => {
                  const next = new Set(selected);
                  if (event.target.checked) {
                    next.add(option);
                  } else {
                    next.delete(option);
                  }
                  onChange(JSON.stringify(Array.from(next)));
                }}
                className="border-border accent-primary h-3 w-3 rounded"
              />
              {option}
            </label>
          );
        })}
      </div>
    );
  }

  if (property.property_type === "relation") {
    return (
      <Select
        value={property.value || ""}
        disabled={!editable}
        onChange={(event) => onChange(event.target.value)}
        className="bg-bg h-7 px-2 text-xs"
        aria-label={property.property_name}
      >
        <option value="">None</option>
        {relationDocuments.map((doc) => (
          <option key={doc.id} value={doc.id}>
            {doc.title}
          </option>
        ))}
      </Select>
    );
  }

  if (property.property_type === "user" && userDirectory.length > 0) {
    return (
      <Select
        value={property.value || ""}
        disabled={!editable}
        onChange={(event) => onChange(event.target.value)}
        className="bg-bg h-7 px-2 text-xs"
        aria-label={property.property_name}
      >
        <option value="">None</option>
        {userDirectory.map((user) => (
          <option key={user.id} value={user.id}>
            {user.display_name} ({user.email})
          </option>
        ))}
      </Select>
    );
  }

  return (
    <Input
      type={
        property.property_type === "number"
          ? "number"
          : property.property_type === "date"
            ? "date"
            : "text"
      }
      value={property.value || ""}
      disabled={!editable}
      onChange={(event) => onChange(event.target.value)}
      aria-label={property.property_name}
      className="h-7 text-xs"
    />
  );
}

function propertyTypeUsesOptions(type: string): boolean {
  return type === "select" || type === "multi_select";
}

function parsePropertyOptions(value: string): string[] {
  return value
    .split(",")
    .map((option) => option.trim())
    .filter(Boolean);
}

function formatPropertyOptions(value: unknown): string {
  return getPropertyOptions(value).join(", ");
}

function getPropertyOptions(value: unknown): string[] {
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  if (
    value &&
    typeof value === "object" &&
    Array.isArray((value as { options?: unknown }).options)
  ) {
    return (value as { options: unknown[] }).options.map(String).filter(Boolean);
  }
  return [];
}

function parseMultiSelectValue(value: string | null): string[] {
  if (!value) return [];
  try {
    const parsed = JSON.parse(value) as unknown;
    return Array.isArray(parsed) ? parsed.map(String) : [];
  } catch {
    return value
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean);
  }
}

function ColorPicker({
  value,
  onChange,
  disabled = false,
}: {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex flex-wrap items-center gap-1">
      {TAG_COLORS.map((color) => (
        <button
          key={color}
          type="button"
          disabled={disabled}
          onClick={() => onChange(color)}
          className={`h-5 w-5 rounded border ${value === color ? "border-text" : "border-border"}`}
          style={{ backgroundColor: color }}
          title={`Use ${color}`}
        />
      ))}
      <input
        type="color"
        disabled={disabled}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="border-border h-5 w-7 rounded border bg-transparent"
        aria-label="Custom tag color"
        title="Custom tag color"
      />
    </div>
  );
}

export function ColorSwatchMenu({
  open,
  value,
  onSelect,
  onOpenChange,
  children,
}: {
  open: boolean;
  value: string;
  onSelect: (value: string) => void;
  onOpenChange: (open: boolean) => void;
  children: ReactNode;
}) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const handleMenuKeyDown = useMenuKeyboardNavigation(menuRef);

  const selectAndClose = (color: string) => {
    onSelect(color);
    onOpenChange(false);
  };

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent align="start" side="bottom" sideOffset={4} className="w-36 p-1">
        <div
          ref={menuRef}
          role="menu"
          aria-label="Tag colors"
          tabIndex={-1}
          onKeyDown={handleMenuKeyDown}
          className="grid grid-cols-5 gap-1"
        >
          {TAG_COLORS.map((color) => (
            <button
              key={color}
              type="button"
              role="menuitemradio"
              aria-checked={value === color}
              onClick={() => selectAndClose(color)}
              className={`h-6 w-6 rounded-sm border ${
                value === color ? "border-text" : "border-border"
              }`}
              style={{ backgroundColor: color }}
              title={`Use ${color}`}
            />
          ))}
          <div className="border-border col-span-5 flex items-center gap-1 border-t pt-1">
            <input
              type="color"
              value={customTagColorFromValue(value)}
              onChange={(event) => selectAndClose(event.target.value)}
              className="border-border h-6 w-8 rounded border bg-transparent"
              aria-label="Custom tag color"
              title="Custom tag color"
            />
            <span className="text-text-muted text-[10px]">Custom</span>
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
}

function PropertyRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between py-1">
      <span className="text-text-muted text-xs">{label}</span>
      <span className="text-text max-w-[60%] truncate text-xs">{value}</span>
    </div>
  );
}

function PropertyTypeIcon({ type }: { type: string }) {
  switch (type) {
    case "checkbox":
      return <CheckSquare size={13} />;
    case "number":
      return <Hash size={13} />;
    case "url":
      return <Link size={13} />;
    case "select":
    case "multi_select":
      return <ChevronDown size={13} />;
    case "rating":
      return <Star size={13} />;
    case "user":
      return <Pencil size={13} />;
    case "relation":
      return <ArrowRightIcon />;
    case "date":
    case "datetime":
      return <Square size={13} />;
    case "text":
    default:
      return <Text size={13} />;
  }
}

function ArrowRightIcon() {
  return <span className="text-[13px] leading-none">→</span>;
}

function formatUserValue(value: string | undefined, directory: UserDirectoryRecord[]): string {
  if (!value) return "Unknown";
  const user = directory.find(
    (entry) => entry.id === value || entry.email === value || entry.display_name === value,
  );
  return user?.display_name || value;
}

function workflowStageName(workflow: DocumentWorkflowRecord, stageId: string): string {
  return workflow.stages.find((stage) => stage.id === stageId)?.name ?? "Unknown stage";
}

function workflowEventLabel(
  workflow: DocumentWorkflowRecord,
  event: DocumentWorkflowEventRecord,
): string {
  const toStage = workflowStageName(workflow, event.to_stage_id);
  if (event.action === "enable") return `Enabled at ${toStage}`;
  if (event.action === "disable") return "Workflow disabled";
  if (event.action === "delegate") return `Delegated at ${toStage}`;
  if (event.action === "escalate") return `Escalated at ${toStage}`;
  if (event.from_stage_id) {
    return `Moved from ${workflowStageName(workflow, event.from_stage_id)} to ${toStage}`;
  }
  return `Moved to ${toStage}`;
}

function readSynopsis(content: string): string {
  const match = content.match(/^---\n([\s\S]*?)\n---\n?/);
  if (!match) return "";
  const line = match[1]
    .split(/\r?\n/)
    .find((entry) => /^(synopsis|excerpt|description):/i.test(entry));
  if (!line) return "";
  const value = line.replace(/^[^:]+:\s*/, "").trim();
  if (
    (value.startsWith('"') && value.endsWith('"')) ||
    (value.startsWith("'") && value.endsWith("'"))
  ) {
    return value.slice(1, -1).replace(/\\"/g, '"');
  }
  return value;
}

function writeSynopsis(content: string, synopsis: string): string {
  const trimmed = synopsis.trim();
  const escaped = trimmed.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  const match = content.match(/^---\n([\s\S]*?)\n---\n?/);

  if (!match) {
    return trimmed ? `---\nsynopsis: "${escaped}"\n---\n\n${content.trimStart()}` : content;
  }

  const lines = match[1].split(/\r?\n/);
  const filtered = lines.filter((entry) => !/^(synopsis|excerpt|description):/i.test(entry));

  if (trimmed) {
    const nextLine = `synopsis: "${escaped}"`;
    filtered.unshift(nextLine);
  }

  const body = content.slice(match[0].length).trimStart();
  return filtered.length > 0 ? `---\n${filtered.join("\n")}\n---\n\n${body}` : body;
}

function customTagColorFromValue(value: string): string {
  return /^#[0-9a-f]{6}$/i.test(value) ? value : "#2563eb";
}

function formatDate(iso?: string): string {
  if (!iso) return "Unknown";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "Unknown";
  return date.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
