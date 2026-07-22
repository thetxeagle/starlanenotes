import { useCallback, useEffect, useState } from "react";
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
  Square,
  Text,
  Trash2,
  Workflow,
  X,
} from "lucide-react";
import { emitGraphDirty } from "@/lib/graphEvents";
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
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";

export const PROPERTIES_PANEL_ERROR_CLASS =
  "rounded-input border border-danger/30 bg-danger/10 px-2 py-1.5 text-[11px] leading-4 text-danger";

export function getPropertyDeleteConfirmationMessage(name: string): string {
  const trimmed = name.trim();
  return trimmed
    ? `Delete property "${trimmed}" from this space? Existing values will be removed from documents.`
    : "Delete this property from the space? Existing values will be removed from documents.";
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
const SYNOPSIS_UPDATED_EVENT = "starlane:synopsis-updated";

interface PropertiesPanelProps {
  spaceId: string;
  docId: string;
  title: string;
  icon?: string;
  coverImage?: string;
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
  createdBy,
  lastUpdatedBy,
  createdAt,
  updatedAt,
  locked,
  isPublic,
}: PropertiesPanelProps) {
  const [expanded, setExpanded] = useState(true);
  const [properties, setProperties] = useState<DocumentPropertyValueRecord[]>([]);
  const [relationDocuments, setRelationDocuments] = useState<DocumentRecord[]>([]);
  const [userDirectory, setUserDirectory] = useState<UserDirectoryRecord[]>([]);
  const [tags, setTags] = useState<TagRecord[]>([]);
  const [availableTags, setAvailableTags] = useState<TagRecord[]>([]);
  const [workflowTemplates, setWorkflowTemplates] = useState<WorkflowTemplateRecord[]>([]);
  const [documentWorkflow, setDocumentWorkflow] = useState<DocumentWorkflowRecord | null>(null);
  const [workflowEvents, setWorkflowEvents] = useState<DocumentWorkflowEventRecord[]>([]);
  const [selectedWorkflowTemplateId, setSelectedWorkflowTemplateId] = useState("");
  const [workflowBusy, setWorkflowBusy] = useState(false);
  const [workflowError, setWorkflowError] = useState("");
  const [synopsis, setSynopsis] = useState("");
  const [documentContent, setDocumentContent] = useState("");
  const [newPropertyName, setNewPropertyName] = useState("");
  const [newPropertyType, setNewPropertyType] = useState("text");
  const [newPropertyOptions, setNewPropertyOptions] = useState("");
  const [propertyComposerOpen, setPropertyComposerOpen] = useState(false);
  const [newTagName, setNewTagName] = useState("");
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

  const reloadPropertiesAndTags = useCallback(async () => {
    const [
      nextProperties,
      nextTags,
      nextAvailableTags,
      nextDocuments,
      nextUsers,
      nextWorkflowTemplates,
      nextDocumentWorkflow,
      nextWorkflowEvents,
    ] = await Promise.all([
      dataClient.listDocumentProperties(spaceId, docId).catch(() => []),
      dataClient.listDocumentTags(spaceId, docId).catch(() => []),
      dataClient.listTags(spaceId).catch(() => []),
      dataClient.listDocuments(spaceId).catch(() => []),
      dataClient.listUserDirectory().catch(() => []),
      dataClient.listWorkflowTemplates(spaceId).catch(() => []),
      dataClient.getDocumentWorkflow(spaceId, docId).catch(() => null),
      dataClient.listDocumentWorkflowEvents(spaceId, docId).catch(() => []),
    ]);
    setProperties(nextProperties);
    setTags(nextTags);
    setAvailableTags(nextAvailableTags);
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
    let cancelled = false;
    dataClient
      .getDocumentContent(spaceId, docId)
      .then((record) => {
        if (cancelled) return;
        setDocumentContent(record.content);
        setSynopsis(readSynopsis(record.content));
      })
      .catch(() => {
        if (!cancelled) {
          setDocumentContent("");
          setSynopsis("");
        }
      });

    return () => {
      cancelled = true;
    };
  }, [docId, spaceId]);

  const saveSynopsis = async () => {
    const nextContent = writeSynopsis(documentContent, synopsis);
    setDocumentContent(nextContent);
    await dataClient.updateDocumentContent(spaceId, docId, nextContent);
    emitGraphDirty(spaceId, "document-content-updated");
    window.dispatchEvent(
      new CustomEvent(SYNOPSIS_UPDATED_EVENT, {
        detail: { spaceId, docId, synopsis },
      }),
    );
  };

  const updateProperty = async (propertyId: string, value: string) => {
    setProperties((prev) =>
      prev.map((prop) => (prop.property_id === propertyId ? { ...prop, value } : prop)),
    );
    await dataClient.setDocumentPropertyValues(spaceId, docId, [{ property_id: propertyId, value }]);
  };

  const createProperty = async () => {
    const name = newPropertyName.trim();
    if (!name) return;
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
  };

  const beginEditProperty = (prop: DocumentPropertyValueRecord) => {
    setEditingPropertyId(prop.property_id);
    setEditingPropertyName(prop.property_name);
    setEditingPropertyType(prop.property_type);
    setEditingPropertyOptions(formatPropertyOptions(prop.options));
  };

  const savePropertyEdit = async () => {
    if (!editingPropertyId || !editingPropertyName.trim()) return;
    await dataClient.updatePropertySchema(spaceId, editingPropertyId, {
      name: editingPropertyName,
      property_type: editingPropertyType,
      options: propertyTypeUsesOptions(editingPropertyType)
        ? parsePropertyOptions(editingPropertyOptions)
        : undefined,
    });
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
    if (!deletePropertyTarget) return;
    await dataClient.deletePropertySchema(spaceId, deletePropertyTarget.id);
    setDeletePropertyTarget(null);
    await reloadPropertiesAndTags();
  };

  const moveProperty = async (index: number, direction: -1 | 1) => {
    const nextIndex = index + direction;
    if (nextIndex < 0 || nextIndex >= properties.length) return;
    const nextProperties = [...properties];
    [nextProperties[index], nextProperties[nextIndex]] = [
      nextProperties[nextIndex],
      nextProperties[index],
    ];
    setProperties(nextProperties);
    await dataClient.reorderPropertySchemas(spaceId, {
      property_ids: nextProperties.map((prop) => prop.property_id),
    });
    await reloadPropertiesAndTags();
  };

  const createAndAddTag = async () => {
    const name = newTagName.trim();
    if (!name) return;
    const existing = availableTags.find((tag) => tag.name.toLowerCase() === name.toLowerCase());
    const tag = existing ?? (await dataClient.createTag(spaceId, { name, color: TAG_COLORS[0] }));
    await dataClient.addDocumentTag(spaceId, docId, tag.id);
    setNewTagName("");
    setTags((current) => (current.some((item) => item.id === tag.id) ? current : [...current, tag]));
    setAvailableTags((current) => (current.some((item) => item.id === tag.id) ? current : [...current, tag]));
    await reloadPropertiesAndTags();
  };

  const updateTagColor = async (tag: TagRecord, color: string) => {
    await dataClient.updateTag(spaceId, tag.id, { name: tag.name, color });
    setActiveTagColorMenuId(null);
    await reloadPropertiesAndTags();
  };

  const removeTag = async (tagId: string) => {
    await dataClient.removeDocumentTag(spaceId, docId, tagId);
    await reloadPropertiesAndTags();
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
    if (!editingTagId || !editingTagName.trim()) return;
    await dataClient.updateTag(spaceId, editingTagId, {
      name: editingTagName,
      color: editingTagColor,
    });
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
      setWorkflowEvents(await dataClient.listDocumentWorkflowEvents(spaceId, docId).catch(() => []));
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
      setWorkflowEvents(await dataClient.listDocumentWorkflowEvents(spaceId, docId).catch(() => []));
    } catch (error) {
      setWorkflowError(error instanceof Error ? error.message : "Unable to move workflow");
    } finally {
      setWorkflowBusy(false);
    }
  };

  const unassignedTags = availableTags.filter(
    (tag) => !tags.some((assigned) => assigned.id === tag.id),
  );

  return (
    <div className="border-b border-border">
      {deletePropertyTarget && (
        <ConfirmDialog
          labelledBy="delete-property-title"
          title="Delete property"
          message={getPropertyDeleteConfirmationMessage(deletePropertyTarget.name)}
          confirmLabel="Delete"
          destructive
          onCancel={() => setDeletePropertyTarget(null)}
          onConfirm={() => void confirmDeleteProperty()}
        />
      )}
      {/* Header */}
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-1 px-4 py-2 text-sm font-medium text-text hover:bg-surface-raised"
      >
        {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        Properties
      </button>

      {expanded && (
        <div className="px-4 pb-3 text-sm">
          {/* Built-in properties */}
          <PropertyRow label="Title" value={title} />
          {icon && <PropertyRow label="Icon" value={icon} />}
          <PropertyRow label="Created Date" value={formatDate(createdAt)} />
          <PropertyRow label="Created By" value={formatUserValue(createdBy, userDirectory)} />
          <PropertyRow label="Updated By" value={formatUserValue(lastUpdatedBy, userDirectory)} />
          <PropertyRow label="Last Updated" value={formatDate(updatedAt)} />
          <PropertyRow label="Locked" value={locked ? "Yes" : "No"} />
          <PropertyRow label="Public" value={isPublic ? "Yes" : "No"} />

          <div className="mt-3 border-t border-border pt-2">
            <div className="mb-2 flex items-center gap-1.5 text-xs text-text-muted">
              <Workflow size={13} />
              <span>Workflow</span>
            </div>
            {documentWorkflow ? (
              <div className="rounded-input border border-border bg-surface-raised p-2">
                <div className="flex items-center justify-between gap-2">
                  <div className="min-w-0">
                    <div className="truncate text-sm font-medium text-text">
                      {documentWorkflow.workflow_template_name}
                    </div>
                    <div className="mt-0.5 text-xs text-text-muted">
                      {documentWorkflow.current_stage_name} / {documentWorkflow.status}
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => void disableWorkflow()}
                    disabled={workflowBusy}
                    className="rounded-button border border-border px-2 py-1 text-xs text-text-muted hover:bg-bg hover:text-text disabled:opacity-50"
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
                  <div className={`mt-2 ${PROPERTIES_PANEL_ERROR_CLASS}`}>
                    {workflowError}
                  </div>
                )}
                {workflowEvents.length > 0 && (
                  <div className="mt-3 border-t border-border pt-2">
                    <div className="mb-1 text-[11px] uppercase tracking-wide text-text-muted">
                      Recent activity
                    </div>
                    <div className="grid gap-1.5">
                      {workflowEvents.slice(0, 5).map((event) => (
                        <div key={event.id} className="text-[11px] leading-4 text-text-muted">
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
                  className="h-8 bg-bg px-2 text-xs"
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
                  className="h-8 rounded-button border border-border bg-surface-raised px-3 text-xs text-text hover:bg-bg disabled:opacity-50"
                >
                  Enable workflow
                </button>
                {workflowError && (
                  <div className={PROPERTIES_PANEL_ERROR_CLASS}>
                    {workflowError}
                  </div>
                )}
              </div>
            ) : (
              <div className="text-xs text-text-muted">No workflow templates</div>
            )}
          </div>

          <div className="mt-3 border-t border-border pt-2">
            <span className="text-xs text-text-muted">Synopsis</span>
            <p className="mt-1 text-[11px] leading-4 text-text-muted">
              Add a short description for Page View, search previews, and future publishing surfaces.
            </p>
            <Textarea
              value={synopsis}
              onChange={(event) => setSynopsis(event.target.value)}
              onBlur={() => void saveSynopsis()}
              placeholder="Add a concise page summary..."
              rows={4}
              className="mt-2 resize-none bg-surface text-sm leading-5"
            />
          </div>

          <div className="mt-3 border-t border-border pt-2">
            <span className="text-xs text-text-muted">Tags</span>
            <div className="mt-1 flex flex-wrap gap-1">
              {tags.length === 0 && <span className="text-xs text-text-muted">No tags</span>}
              {tags.map((tag) => (
                <span
                  key={tag.id}
                  className="relative inline-flex items-center gap-1 rounded-full border border-border bg-surface-raised px-2 py-0.5 text-xs text-text"
                  style={{ backgroundColor: `${tag.color}20`, color: tag.color }}
                >
                  <button
                    type="button"
                    onClick={() => setActiveTagColorMenuId((current) => (current === tag.id ? null : tag.id))}
                    className="h-3.5 w-3.5 rounded-sm border border-border"
                    style={{ backgroundColor: tag.color }}
                    aria-label={`Change ${tag.name} color`}
                    title={`Change ${tag.name} color`}
                  />
                  {activeTagColorMenuId === tag.id && (
                    <ColorSwatchMenu
                      value={tag.color}
                      onSelect={(color) => void updateTagColor(tag, color)}
                      onClose={() => setActiveTagColorMenuId(null)}
                    />
                  )}
                  <button
                    type="button"
                    onClick={() => openTagSearch(tag)}
                    className="inline-flex max-w-28 items-center gap-0.5 truncate hover:underline"
                    title={`Search ${tag.name}`}
                  >
                    <Hash size={9} />
                    <span className="truncate">{tag.name}</span>
                  </button>
                  <button type="button" onClick={() => beginEditTag(tag)} title={`Edit ${tag.name}`}>
                    <Pencil size={10} />
                  </button>
                  <button type="button" onClick={() => removeTag(tag.id)} title={`Remove ${tag.name}`}>
                    <Trash2 size={10} />
                  </button>
                </span>
              ))}
            </div>
            <div className="mt-2 grid gap-2">
              {editingTagId && (
                <div className="grid gap-2 rounded-input border border-border p-2">
                  <Input
                    value={editingTagName}
                    onChange={(event) => setEditingTagName(event.target.value)}
                    placeholder="Tag name"
                    className="h-7 text-xs"
                  />
                  <div className="flex items-center gap-1">
                    <ColorPicker value={editingTagColor} onChange={setEditingTagColor} />
                    <IconButton title="Save tag" onClick={saveTagEdit} icon={<Check size={14} />} />
                    <IconButton
                      title="Cancel tag edit"
                      onClick={() => setEditingTagId(null)}
                      icon={<X size={14} />}
                    />
                  </div>
                </div>
              )}
              {unassignedTags.length > 0 && (
                <datalist id={`available-tags-${docId}`}>
                  {unassignedTags.map((tag) => (
                    <option key={tag.id} value={tag.name} />
                  ))}
                </datalist>
              )}
              <div>
                <Input
                  value={newTagName}
                  onChange={(event) => setNewTagName(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      void createAndAddTag();
                    }
                  }}
                  list={`available-tags-${docId}`}
                  placeholder="Search or create tag"
                  className="h-10 text-sm"
                />
              </div>
            </div>
          </div>

          {/* Custom properties */}
          <div className="mt-3 border-t border-border pt-2">
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs text-text-muted">Custom properties</span>
              <IconButton
                title={propertyComposerOpen ? "Close property menu" : "Add custom property"}
                onClick={() => setPropertyComposerOpen((current) => !current)}
                icon={propertyComposerOpen ? <X size={14} /> : <Plus size={14} />}
              />
            </div>
            {properties.length === 0 && (
              <div className="mt-1 text-xs text-text-muted">No custom properties</div>
            )}
            {properties.map((prop, index) => (
              <div key={prop.property_id} className="mt-2">
                {editingPropertyId === prop.property_id ? (
                  <div className="mb-1 grid gap-1 rounded-input border border-border p-2">
                    <Input
                      value={editingPropertyName}
                      onChange={(event) => setEditingPropertyName(event.target.value)}
                      className="h-7 text-xs"
                    />
                    <div className="flex gap-1">
                      <PropertyTypeSelect
                        value={editingPropertyType}
                        onChange={setEditingPropertyType}
                      />
                      <IconButton
                        title="Save property"
                        onClick={savePropertyEdit}
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
                        onChange={(event) => setEditingPropertyOptions(event.target.value)}
                        placeholder="Options separated by commas"
                        className="h-7 text-xs"
                      />
                    )}
                  </div>
                ) : (
                  <div className="mb-0.5 grid grid-cols-[1.25rem_minmax(0,1fr)_auto] items-center gap-2">
                    <span className="flex h-5 w-5 items-center justify-center text-text-muted">
                      <PropertyTypeIcon type={prop.property_type} />
                    </span>
                    <label className="block min-w-0 truncate text-xs text-text-muted">{prop.property_name}</label>
                    <div className="flex shrink-0 items-center gap-1">
                      <button
                        type="button"
                        onClick={() => moveProperty(index, -1)}
                        disabled={index === 0}
                        className="text-text-muted hover:text-text disabled:opacity-30"
                        title={`Move ${prop.property_name} up`}
                      >
                        <ArrowUp size={12} />
                      </button>
                      <button
                        type="button"
                        onClick={() => moveProperty(index, 1)}
                        disabled={index === properties.length - 1}
                        className="text-text-muted hover:text-text disabled:opacity-30"
                        title={`Move ${prop.property_name} down`}
                      >
                        <ArrowDown size={12} />
                      </button>
                      <button
                        type="button"
                        onClick={() => beginEditProperty(prop)}
                        className="text-text-muted hover:text-text"
                        title={`Edit ${prop.property_name}`}
                      >
                        <Pencil size={12} />
                      </button>
                      <button
                        type="button"
                        onClick={() => deleteProperty(prop.property_id)}
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
                  onChange={(value) => updateProperty(prop.property_id, value)}
                />
              </div>
            ))}

            {propertyComposerOpen && (
              <div className="rounded-input border-border bg-surface-raised shadow-card mt-3 grid gap-2 border p-2">
                <Input
                  value={newPropertyName}
                  onChange={(event) => setNewPropertyName(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && !propertyTypeUsesOptions(newPropertyType)) {
                      event.preventDefault();
                      void createProperty();
                    }
                    if (event.key === "Escape") setPropertyComposerOpen(false);
                  }}
                  placeholder="Property name"
                  className="h-7 text-xs"
                />
                <div className="flex gap-1">
                  <PropertyTypeSelect
                    value={newPropertyType}
                    onChange={setNewPropertyType}
                  />
                  <IconButton title="Create property" onClick={createProperty} icon={<Check size={14} />} />
                </div>
                {propertyTypeUsesOptions(newPropertyType) && (
                  <Input
                    value={newPropertyOptions}
                    onChange={(event) => setNewPropertyOptions(event.target.value)}
                    placeholder="Options separated by commas"
                    className="h-7 text-xs"
                  />
                )}
              </div>
            )}
          </div>
        </div>
      )}
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
      className="flex h-7 w-7 items-center justify-center rounded-button border border-border text-text-muted hover:bg-surface-raised hover:text-text disabled:opacity-40"
      title={title}
    >
      {icon}
    </button>
  );
}

export function PropertyTypeSelect({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <Select
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className="h-7 min-w-0 flex-1 bg-bg px-2 text-xs"
      aria-label="Property type"
    >
      <option value="text">Text</option>
      <option value="number">Number</option>
      <option value="date">Date</option>
      <option value="datetime">Date and time</option>
      <option value="checkbox">Checkbox</option>
      <option value="select">Select</option>
      <option value="multi_select">Multi-select</option>
      <option value="user">User</option>
      <option value="relation">Relation</option>
      <option value="url">URL</option>
    </Select>
  );
}

function PropertyValueControl({
  property,
  relationDocuments,
  userDirectory,
  onChange,
}: {
  property: DocumentPropertyValueRecord;
  relationDocuments: DocumentRecord[];
  userDirectory: UserDirectoryRecord[];
  onChange: (value: string) => void;
}) {
  const options = getPropertyOptions(property.options);
  if (property.property_type === "checkbox") {
    return (
      <input
        type="checkbox"
        checked={property.value === "true"}
        onChange={(event) => onChange(String(event.target.checked))}
        className="h-4 w-4 rounded border-border accent-primary"
      />
    );
  }

  if (property.property_type === "select") {
    return (
      <Select
        value={property.value || ""}
        onChange={(event) => onChange(event.target.value)}
        className="h-7 bg-bg px-2 text-xs"
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
        {options.length === 0 && <span className="text-xs text-text-muted">No options</span>}
        {options.map((option) => {
          const checked = selected.has(option);
          return (
            <label
              key={option}
              className="inline-flex items-center gap-1 rounded border border-border px-2 py-0.5 text-xs text-text"
            >
              <input
                type="checkbox"
                checked={checked}
                onChange={(event) => {
                  const next = new Set(selected);
                  if (event.target.checked) {
                    next.add(option);
                  } else {
                    next.delete(option);
                  }
                  onChange(JSON.stringify(Array.from(next)));
                }}
                className="h-3 w-3 rounded border-border accent-primary"
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
        onChange={(event) => onChange(event.target.value)}
        className="h-7 bg-bg px-2 text-xs"
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
        onChange={(event) => onChange(event.target.value)}
        className="h-7 bg-bg px-2 text-xs"
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
      type={property.property_type === "number" ? "number" : property.property_type === "date" ? "date" : "text"}
      value={property.value || ""}
      onChange={(event) => onChange(event.target.value)}
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
  if (value && typeof value === "object" && Array.isArray((value as { options?: unknown }).options)) {
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
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-1">
      {TAG_COLORS.map((color) => (
        <button
          key={color}
          type="button"
          onClick={() => onChange(color)}
          className={`h-5 w-5 rounded border ${
            value === color ? "border-text" : "border-border"
          }`}
          style={{ backgroundColor: color }}
          title={`Use ${color}`}
        />
      ))}
      <input
        type="color"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="h-5 w-7 rounded border border-border bg-transparent"
        aria-label="Custom tag color"
        title="Custom tag color"
      />
    </div>
  );
}

function ColorSwatchMenu({
  value,
  onSelect,
  onClose,
}: {
  value: string;
  onSelect: (value: string) => void;
  onClose: () => void;
}) {
  return (
    <>
      <button
        type="button"
        aria-label="Close tag color menu"
        className="fixed inset-0 z-20 cursor-default"
        onClick={onClose}
      />
      <div
        role="menu"
        aria-label="Tag colors"
        className="rounded-input border-border bg-surface-raised shadow-card absolute left-0 top-6 z-30 grid w-36 grid-cols-5 gap-1 border p-1"
      >
        {TAG_COLORS.map((color) => (
          <button
            key={color}
            type="button"
            role="menuitemradio"
            aria-checked={value === color}
            onClick={() => onSelect(color)}
            className={`h-6 w-6 rounded-sm border ${
              value === color ? "border-text" : "border-border"
            }`}
            style={{ backgroundColor: color }}
            title={`Use ${color}`}
          />
        ))}
        <div className="col-span-5 flex items-center gap-1 border-t border-border pt-1">
          <input
            type="color"
            value={customTagColorFromValue(value)}
            onChange={(event) => onSelect(event.target.value)}
            className="h-6 w-8 rounded border border-border bg-transparent"
            aria-label="Custom tag color"
            title="Custom tag color"
          />
          <span className="text-[10px] text-text-muted">Custom</span>
        </div>
      </div>
    </>
  );
}

function PropertyRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between py-1">
      <span className="text-xs text-text-muted">{label}</span>
      <span className="max-w-[60%] truncate text-xs text-text">{value}</span>
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
