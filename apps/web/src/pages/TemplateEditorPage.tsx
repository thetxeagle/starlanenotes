import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { Check, ChevronDown, Code2, Copy, Eye, History, Pencil, Plus, Save, Search, SlidersHorizontal, TestTubeDiagonal, Trash2, X } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";
import { Editor } from "@/components/editor/Editor";
import { DocumentContents } from "@/components/editor/DocumentContents";
import { MarkdownPreview } from "@/components/editor/MarkdownPreview";
import { Button } from "@/components/ui/button";
import { DialogShell, DialogHeader, DialogTitle, DialogCloseAction, DialogBody } from "@/components/ui/dialog-shell";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Select } from "@/components/ui/select";
import { PageIcon } from "@/components/editor/PageIcon";
import { CoverImage } from "@/components/editor/CoverImage";
import { Breadcrumb } from "@/components/editor/Breadcrumb";
import {
  getDocumentPageWidthClass,
  normalizeDocumentPageWidth,
  type DocumentPageWidth,
} from "@/components/editor/DocumentWidthPicker";
import { useAutoSave } from "@/hooks/useAutoSave";
import { useAuth } from "@/hooks/useAuth";
import { emitTransientSystemNotification } from "@/lib/systemNotifications";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import { useRuntimePlatform } from "@/hooks/useRuntimePlatform";
import {
  dataClient,
  inLocalMode,
  type PropertySchemaRecord,
  type TagRecord,
  type TemplateDraftRecord,
  type TemplateInput,
  type TemplateRecord,
  type TemplateRevisionRecord,
} from "@/lib/dataClient";
import {
  getTemplateEditorPath,
  templatePresentationAsRecord,
  templatePropertiesAsRecord,
  withTemplatePresentation,
} from "@/lib/templates";
import {
  TEMPLATE_EXPERIENCE_KEY,
  getTemplateFieldOptions,
  getTemplateExperience,
  normalizeTemplateExperience,
  resolveTemplateExperience,
  type TemplateExperienceConfig,
  type TemplateFieldDefinition,
} from "@/lib/templateExperience";
import { PropertyTypeSelect, getMatchingTags } from "@/components/PropertiesPanel";
import {
  CONTROL_CHROME_ACTION_CLASS,
  CONTROL_CHROME_ACTIVE_ACTION_CLASS,
  CONTROL_CHROME_RAIL_CLASS,
} from "@/components/ui/control-chrome";

export interface TemplateEditorDraft {
  name: string;
  description: string;
  content: string;
  tags: string;
  properties: string;
  icon?: string;
  coverImage?: string;
  customCss?: string;
  pageWidth?: DocumentPageWidth;
  templateExperience?: TemplateExperienceConfig;
  reservedMetadata?: Record<string, unknown>;
}

export interface StoredTemplateEditorDraft {
  draft: Partial<TemplateEditorDraft>;
  savedAt: string;
}

export function setTemplatePropertyDefault(
  properties: Record<string, unknown>,
  name: string,
  value: string,
  preserveEmpty = false,
): Record<string, unknown> {
  const next = { ...properties };
  if (value.trim() || preserveEmpty) next[name] = value;
  else delete next[name];
  return next;
}

export const EMPTY_TEMPLATE_EDITOR_DRAFT: TemplateEditorDraft = {
  name: "Untitled template",
  description: "",
  content: "",
  tags: "",
  properties: "",
  icon: "",
  coverImage: "",
  customCss: "",
  pageWidth: "standard",
  templateExperience: { schema_version: 1, fields: [] },
  reservedMetadata: {},
};

export function buildTemplateEditorInput(draft: TemplateEditorDraft): TemplateInput {
  const propertyText = draft.properties.trim();
  let properties: unknown;
  if (propertyText) {
    try {
      properties = JSON.parse(propertyText);
    } catch {
      throw new Error("Template properties must be valid JSON.");
    }
    if (!properties || Array.isArray(properties) || typeof properties !== "object") {
      throw new Error("Template properties must be a JSON object.");
    }
  }

  const propertiesRecord = propertyText ? (properties as Record<string, unknown>) : undefined;
  const presentation = {
    icon: draft.icon ?? "",
    cover_image: draft.coverImage ?? "",
    custom_css: draft.customCss ?? "",
    ...(draft.pageWidth && draft.pageWidth !== "standard" ? { page_width: draft.pageWidth } : {}),
  };
  const metadata = { ...(draft.reservedMetadata ?? {}) };
  const experience = draft.templateExperience ?? { schema_version: 1 as const, fields: [] };
  const hasExperience = experience.fields.length > 0 || Boolean(experience.title?.length);
  const hasStoredExperience = Object.prototype.hasOwnProperty.call(metadata, TEMPLATE_EXPERIENCE_KEY);
  const storedExperience = metadata[TEMPLATE_EXPERIENCE_KEY];
  const existingVersion = storedExperience && typeof storedExperience === "object" && !Array.isArray(storedExperience)
    ? (storedExperience as Record<string, unknown>).schema_version
    : undefined;
  if (!hasStoredExperience || existingVersion === 1) {
    const normalizedExperience = normalizeTemplateExperience(experience);
    if (normalizedExperience.fields.length !== experience.fields.length ||
      (experience.title?.length ?? 0) !== (normalizedExperience.title?.length ?? 0)) {
      throw new Error("Every template field needs a unique ID, label, supported type, and valid title binding.");
    }
    const mergedExperience = mergeTemplateExperienceMetadata(storedExperience, experience);
    const hasOpaqueExperienceData = mergedExperience.fields.length > 0 || Boolean(mergedExperience.title?.length) ||
      Object.keys(mergedExperience).some((key) => key !== "schema_version" && key !== "fields" && key !== "title");
    if (hasExperience || hasOpaqueExperienceData) metadata[TEMPLATE_EXPERIENCE_KEY] = mergedExperience;
    else delete metadata[TEMPLATE_EXPERIENCE_KEY];
  }
  const storedProperties = withTemplatePresentation({ ...(propertiesRecord ?? {}), ...metadata }, presentation);

  return {
    name: draft.name.trim(),
    description: draft.description.trim(),
    content: draft.content,
    tags: Array.from(
      new Set(
        draft.tags
          .split(",")
          .map((tag) => tag.trim())
          .filter(Boolean),
      ),
    ),
    properties: storedProperties,
  };
}

function mergeTemplateExperienceMetadata(stored: unknown, edited: TemplateExperienceConfig): TemplateExperienceConfig & Record<string, unknown> {
  const previous = stored && typeof stored === "object" && !Array.isArray(stored)
    ? stored as Record<string, unknown>
    : {};
  const normalizedStored = getTemplateExperience({ [TEMPLATE_EXPERIENCE_KEY]: previous });
  const normalizedIds = new Set(normalizedStored.fields.map((field) => field.id));
  const storedFieldsById = new Map<string, Record<string, unknown>>();
  if (Array.isArray(previous.fields)) {
    for (const field of previous.fields) {
      if (field && typeof field === "object" && !Array.isArray(field)) {
        const record = field as Record<string, unknown>;
        if (typeof record.id === "string" && normalizedIds.has(record.id)) storedFieldsById.set(record.id, record);
      }
    }
  }
  const unknownFields = Array.isArray(previous.fields)
    ? previous.fields.filter((field) => {
        if (!field || typeof field !== "object" || Array.isArray(field)) return true;
        const record = field as Record<string, unknown>;
        return typeof record.id !== "string" || !normalizedIds.has(record.id);
      })
    : [];
  return {
    ...previous,
    ...edited,
    fields: [...unknownFields, ...edited.fields.map((field) => ({ ...storedFieldsById.get(field.id), ...field }))],
  };
}

function templateTitlePrefix(config: TemplateExperienceConfig): string {
  const prefix: string[] = [];
  for (const part of config.title ?? []) {
    if (part.kind !== "text") break;
    prefix.push(part.value);
  }
  return prefix.join("");
}

function templateTitleFieldIds(config: TemplateExperienceConfig): string[] {
  return (config.title ?? []).flatMap((part) => part.kind === "field" ? [part.field_id] : []);
}

function buildTemplateTitle(prefix: string, fieldIds: string[]): TemplateExperienceConfig["title"] {
  const title: NonNullable<TemplateExperienceConfig["title"]> = [];
  if (prefix) title.push({ kind: "text", value: prefix });
  fieldIds.forEach((fieldId, index) => {
    if (index > 0) title.push({ kind: "text", value: " — " });
    title.push({ kind: "field", field_id: fieldId });
  });
  return title.length ? title : undefined;
}

export function getTemplateEditorDraft(template: TemplateRecord): TemplateEditorDraft {
  const presentation = templatePresentationAsRecord(template.properties);
  const rawProperties = template.properties && typeof template.properties === "object" && !Array.isArray(template.properties)
    ? template.properties as Record<string, unknown>
    : {};
  const reservedMetadata = Object.fromEntries(
    Object.entries(rawProperties).filter(([key]) => key.startsWith("__starlane_")),
  );
  return {
    name: template.name,
    description: template.description ?? "",
    content: template.content,
    tags: template.tags.join(", "),
    properties: templatePropertiesAsRecord(template.properties)
      ? JSON.stringify(templatePropertiesAsRecord(template.properties), null, 2)
      : "",
    icon: presentation.icon ?? "",
    coverImage: presentation.cover_image ?? "",
    customCss: presentation.custom_css ?? "",
    pageWidth: normalizeDocumentPageWidth(presentation.page_width),
    templateExperience: getTemplateExperience(template),
    reservedMetadata,
  };
}

export async function hydrateTemplateEditorDraft(
  template: TemplateRecord,
  localDraft: StoredTemplateEditorDraft | null,
  loadServerDraft: () => Promise<TemplateDraftRecord>,
): Promise<{
  templateDraft: TemplateDraftRecord;
  draft: TemplateEditorDraft;
  lastAutoSavedAt: string | null;
}> {
  const templateDraft = await loadServerDraft();
  const serverDraft = getTemplateEditorDraft({
    ...template,
    ...templateDraft.snapshot,
    id: template.id,
    space_id: template.space_id,
    created_at: template.created_at,
    updated_at: templateDraft.updated_at,
    tags: templateDraft.snapshot.tags ?? [],
  });
  const localReferenceTime =
    templateDraft.last_autosaved_at ?? templateDraft.updated_at ?? template.updated_at;
  const localIsNewer = Boolean(
    localDraft && Date.parse(localDraft.savedAt) > Date.parse(localReferenceTime),
  );
  const localValues = localDraft
    ? Object.fromEntries(
        Object.entries(localDraft.draft).filter(([, value]) => value !== undefined),
      )
    : {};

  return {
    templateDraft,
    draft: localIsNewer ? { ...serverDraft, ...localValues } : serverDraft,
    lastAutoSavedAt: localIsNewer
      ? localDraft!.savedAt
      : (templateDraft.last_autosaved_at ?? null),
  };
}

export function TemplateEditorPage() {
  const { spaceId, templateId } = useParams<{ spaceId: string; templateId?: string }>();
  const navigate = useNavigate();
  const { isMobileRuntime, isMobileViewport } = useRuntimePlatform();
  const mobileLayout = isMobileRuntime || isMobileViewport;
  const [draft, setDraft] = useState<TemplateEditorDraft>(EMPTY_TEMPLATE_EDITOR_DRAFT);
  const [spaceName, setSpaceName] = useState("Space");
  const [availableTemplates, setAvailableTemplates] = useState<TemplateRecord[]>([]);
  const [availableTags, setAvailableTags] = useState<TagRecord[]>([]);
  const [propertySchemas, setPropertySchemas] = useState<PropertySchemaRecord[]>([]);
  const [tagPickerOpen, setTagPickerOpen] = useState(false);
  const [propertyPickerOpen, setPropertyPickerOpen] = useState(false);
  const [customCssDialogOpen, setCustomCssDialogOpen] = useState(false);
  const [newTagName, setNewTagName] = useState("");
  const [tagComposerBusy, setTagComposerBusy] = useState(false);
  const [tagComposerError, setTagComposerError] = useState("");
  const [tagInputFocused, setTagInputFocused] = useState(false);
  const [newPropertyName, setNewPropertyName] = useState("");
  const [newPropertyType, setNewPropertyType] = useState("text");
  const [newPropertyOptions, setNewPropertyOptions] = useState("");
  const [propertyComposerError, setPropertyComposerError] = useState("");
  const [propertyComposerBusy, setPropertyComposerBusy] = useState(false);
  const [loading, setLoading] = useState(Boolean(templateId));
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [viewMode, setViewMode] = useState<"edit" | "preview" | "source">("edit");
  const [templateDetailsOpen, setTemplateDetailsOpen] = useState(false);
  const [templateTestOpen, setTemplateTestOpen] = useState(false);
  const [revisionsOpen, setRevisionsOpen] = useState(false);
  const [revisions, setRevisions] = useState<TemplateRevisionRecord[]>([]);
  const [revisionBusy, setRevisionBusy] = useState(false);
  const [templateDraft, setTemplateDraft] = useState<TemplateDraftRecord | null>(null);
  const [lockLost, setLockLost] = useState(false);
  const [pageWidth, setPageWidth] = useState<DocumentPageWidth>("standard");
  const [lastAutoSavedAt, setLastAutoSavedAt] = useState<string | null>(null);
  const [desktopToolbarHost, setDesktopToolbarHost] = useState<HTMLDivElement | null>(null);
  const documentBodyRef = useRef<HTMLElement | null>(null);
  const contentsTriggerRef = useRef<HTMLButtonElement | null>(null);
  const retainedCssTriggerRef = useRef<HTMLButtonElement | null>(null);
  const retainedCssTextRef = useRef<HTMLTextAreaElement | null>(null);
  const recoveryWasOpenRef = useRef(false);
  const [headingCount, setHeadingCount] = useState(0);
  const [contentsOpen, setContentsOpen] = useState(() => window.matchMedia("(min-width: 1440px)").matches);
  const draftRef = useRef(draft);
  const lastSaveNotificationAtRef = useRef(0);
  const { user } = useAuth();

  useEffect(() => {
    draftRef.current = draft;
  }, [draft]);

  useEffect(() => {
    const returningFromRecovery = recoveryWasOpenRef.current && !customCssDialogOpen;
    recoveryWasOpenRef.current = customCssDialogOpen;
    if (!templateDetailsOpen || tagPickerOpen || propertyPickerOpen ||
      (!customCssDialogOpen && !returningFromRecovery)) return;
    const frame = window.requestAnimationFrame(() => {
      const target = customCssDialogOpen ? retainedCssTextRef.current : retainedCssTriggerRef.current;
      if (target?.isConnected) target.focus({ preventScroll: true });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [customCssDialogOpen, templateDetailsOpen, tagPickerOpen, propertyPickerOpen]);

  const templateDraftStorageKey = spaceId && templateId
    ? `starlane:template-draft:${spaceId}:${templateId}`
    : null;
  const saveTemplateDraftLocally = async (content: string) => {
    if (lockLost) throw new Error("This template is read-only because the editing lock is no longer yours.");
    const nextDraft = { ...draftRef.current, content };
    if (templateDraft && spaceId && templateId) {
      const updated = await dataClient.updateTemplateDraft(
        spaceId,
        templateId,
        templateDraft.id,
        buildTemplateEditorInput(nextDraft),
      );
      setTemplateDraft(updated);
      setLastAutoSavedAt(updated.last_autosaved_at ?? updated.updated_at);
      return;
    }
    if (!inLocalMode() && spaceId && templateId) {
      const opened = await dataClient.createTemplateDraft(spaceId, templateId);
      const updated = await dataClient.updateTemplateDraft(
        spaceId,
        templateId,
        opened.id,
        buildTemplateEditorInput(nextDraft),
      );
      setTemplateDraft(updated);
      setLastAutoSavedAt(updated.last_autosaved_at ?? updated.updated_at);
      return;
    }
    if (!templateDraftStorageKey || typeof window === "undefined") return;
    const savedAt = new Date().toISOString();
    window.localStorage.setItem(
      templateDraftStorageKey,
      JSON.stringify({ ...draftRef.current, content, pageWidth: draftRef.current.pageWidth ?? pageWidth, savedAt }),
    );
    setLastAutoSavedAt(savedAt);
  };
  const { status: autosaveStatus, scheduleAutoSave, saveNow: saveTemplateDraftNow } = useAutoSave(
    saveTemplateDraftLocally,
  );

  useEffect(() => {
    if (!spaceId) return;
    setLoading(Boolean(templateId));
    setError("");
    void Promise.all([
      dataClient.getSpace(spaceId),
      dataClient.listTemplates(spaceId),
      dataClient.listTags(spaceId),
      dataClient.listPropertySchemas(spaceId),
      templateId ? dataClient.getTemplate(spaceId, templateId) : Promise.resolve(null),
    ])
      .then(async ([space, templates, tags, schemas, template]) => {
        setSpaceName(space.name);
        setAvailableTemplates(templates.filter((item) => item.id !== templateId));
        setAvailableTags(tags);
        setPropertySchemas(schemas);
        const nextDraft = template ? getTemplateEditorDraft(template) : EMPTY_TEMPLATE_EDITOR_DRAFT;
        setTemplateDraft(null);
        setLockLost(false);
        const localDraft = templateId && typeof window !== "undefined" && templateDraftStorageKey
          ? readStoredTemplateDraft(templateDraftStorageKey)
          : null;
        let hydratedDraft = nextDraft;
        let lastAutoSavedAt = localDraft?.savedAt ?? null;
        if (templateId && template) {
          try {
            const hydration = await hydrateTemplateEditorDraft(
              template,
              localDraft,
              () => dataClient.createTemplateDraft(spaceId, templateId),
            );
            setTemplateDraft(hydration.templateDraft);
            hydratedDraft = hydration.draft;
            lastAutoSavedAt = hydration.lastAutoSavedAt;
          } catch (cause) {
            setError(cause instanceof Error ? cause.message : "Unable to open template draft.");
            if (
              localDraft &&
              Date.parse(localDraft.savedAt) > Date.parse(template.updated_at)
            ) {
              hydratedDraft = { ...nextDraft, ...localDraft.draft };
            }
          }
        }
        setDraft(hydratedDraft);
        draftRef.current = hydratedDraft;
        setPageWidth(hydratedDraft.pageWidth ?? "standard");
        setLastAutoSavedAt(lastAutoSavedAt);
        setDirty(false);
      })
      .catch((cause) => {
        setError(cause instanceof Error ? cause.message : "Unable to load template.");
      })
      .finally(() => setLoading(false));
  }, [spaceId, templateId, templateDraftStorageKey]);

  useEffect(() => {
    if (inLocalMode() || !spaceId || !templateId || !templateDraft?.id || !user?.id || lockLost) return;
    let cancelled = false;
    const verifyTemplateLock = async () => {
      try {
        const current = await dataClient.getTemplateDraft(spaceId, templateId, templateDraft.id);
        const expiresAt = current.lock_expires_at ? Date.parse(current.lock_expires_at) : NaN;
        const ownsLock = current.lock_owner_id === user.id && Number.isFinite(expiresAt) && expiresAt > Date.now();
        if (cancelled) return;
        if (!ownsLock) {
          setLockLost(true);
          setViewMode("preview");
          setError("This template is no longer locked to your editing session. It has been opened read-only; reload to try again.");
          return;
        }
        setTemplateDraft((previous) => previous ? {
          ...previous,
          lock_owner_id: current.lock_owner_id,
          lock_expires_at: current.lock_expires_at,
          updated_at: current.updated_at,
          last_autosaved_at: current.last_autosaved_at,
        } : previous);
      } catch (cause) {
        if (cancelled) return;
        setLockLost(true);
        setViewMode("preview");
        setError(cause instanceof Error ? `Template lock check failed: ${cause.message}` : "Template lock check failed. The template is now read-only.");
      }
    };
    void verifyTemplateLock();
    const interval = window.setInterval(() => void verifyTemplateLock(), 15_000);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [lockLost, spaceId, templateDraft?.id, templateId, user?.id]);

  const updateDraft = <TKey extends keyof TemplateEditorDraft>(
    key: TKey,
    value: TemplateEditorDraft[TKey],
  ) => {
    if (lockLost) return;
    setDraft((current) => ({ ...current, [key]: value }));
    draftRef.current = { ...draftRef.current, [key]: value };
    setDirty(true);
    setMessage("");
    if (key === "pageWidth") setPageWidth(value as DocumentPageWidth);
    if (key !== "name") scheduleAutoSave(key === "content" ? String(value) : draftRef.current.content);
  };

  const parsedProperties = useMemo(() => {
    try {
      const value = draft.properties.trim() ? JSON.parse(draft.properties) : {};
      return value && typeof value === "object" && !Array.isArray(value)
        ? (value as Record<string, unknown>)
        : {};
    } catch {
      return {};
    }
  }, [draft.properties]);

  const templateExperience = draft.templateExperience ?? { schema_version: 1 as const, fields: [] };
  const storedExperienceEnvelope = draft.reservedMetadata?.[TEMPLATE_EXPERIENCE_KEY];
  const templateExperienceSupported = !storedExperienceEnvelope ||
    (typeof storedExperienceEnvelope === "object" && !Array.isArray(storedExperienceEnvelope) &&
      (storedExperienceEnvelope as Record<string, unknown>).schema_version === 1);
  const updateTemplateExperience = (next: TemplateExperienceConfig) =>
    updateDraft("templateExperience", next);
  const addTemplateField = () => {
    const id = `field_${typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID().replace(/-/g, "") : Date.now().toString(36)}`;
    updateTemplateExperience({
      ...templateExperience,
      fields: [...templateExperience.fields, { id, label: "New field", type: "text", default_value: "", required: false }],
    });
  };
  const updateTemplateField = (fieldId: string, patch: Partial<TemplateFieldDefinition>) => {
    updateTemplateExperience({
      ...templateExperience,
      fields: templateExperience.fields.map((field) => field.id === fieldId ? { ...field, ...patch } : field),
    });
  };
  const removeTemplateField = (fieldId: string) => {
    const fields = templateExperience.fields.filter((field) => field.id !== fieldId);
    updateTemplateExperience({
      schema_version: 1,
      fields,
      title: buildTemplateTitle(templateTitlePrefix(templateExperience), templateTitleFieldIds(templateExperience).filter((id) => id !== fieldId)),
    });
  };
  const setTitleFields = (fieldId: string, checked: boolean) => {
    const fieldIds = templateTitleFieldIds(templateExperience).filter((id) => id !== fieldId);
    if (checked) fieldIds.push(fieldId);
    updateTemplateExperience({ ...templateExperience, title: buildTemplateTitle(templateTitlePrefix(templateExperience), fieldIds) });
  };

  const setPropertyValue = (name: string, value: string, preserveEmpty = false) => {
    updateDraft("properties", JSON.stringify(setTemplatePropertyDefault(parsedProperties, name, value, preserveEmpty), null, 2));
  };

  const assignedTagNames = useMemo(
    () => draft.tags.split(",").map((tag) => tag.trim()).filter(Boolean),
    [draft.tags],
  );
  const unassignedTags = availableTags.filter(
    (tag) => !assignedTagNames.some((name) => name.toLowerCase() === tag.name.toLowerCase()),
  );
  const matchingTags = getMatchingTags(unassignedTags, newTagName);

  const addTag = async (tagName: string) => {
    const normalized = tagName.trim();
    if (!spaceId || !normalized || tagComposerBusy) return;
    setTagComposerBusy(true);
    setTagComposerError("");
    try {
      const existing = availableTags.find((tag) => tag.name.toLowerCase() === normalized.toLowerCase());
      if (!existing) {
        const created = await dataClient.createTag(spaceId, { name: normalized, color: "#6b7280" });
        setAvailableTags((current) => [...current, created]);
      }
      if (!assignedTagNames.some((name) => name.toLowerCase() === normalized.toLowerCase())) {
        updateDraft("tags", [...assignedTagNames, existing?.name ?? normalized].join(", "));
      }
      setNewTagName("");
      setTagInputFocused(false);
      setTagPickerOpen(false);
    } catch (cause) {
      setTagComposerError(cause instanceof Error ? cause.message : "Unable to add this tag.");
    } finally {
      setTagComposerBusy(false);
    }
  };

  const removeTag = (tagName: string) => {
    updateDraft("tags", assignedTagNames.filter((name) => name !== tagName).join(", "));
  };

  const propertyTypeUsesOptions = (type: string) => type === "select" || type === "multi_select";

  const createDefaultProperty = async () => {
    if (!spaceId || propertyComposerBusy) return;
    const name = newPropertyName.trim();
    if (!name) {
      setPropertyComposerError("Property name is required.");
      return;
    }
    if (Object.keys(parsedProperties).some((key) => key.toLowerCase() === name.toLowerCase())) {
      setPropertyComposerError("A default property with that name already exists.");
      return;
    }
    setPropertyComposerBusy(true);
    setPropertyComposerError("");
    try {
      const schema = await dataClient.createPropertySchema(spaceId, {
        name,
        property_type: newPropertyType,
        options: propertyTypeUsesOptions(newPropertyType)
          ? newPropertyOptions.split(",").map((option) => option.trim()).filter(Boolean)
          : undefined,
      });
      setPropertySchemas((current) => [...current, schema].sort((left, right) => left.sort_order - right.sort_order));
      setPropertyValue(schema.name, newPropertyType === "checkbox" ? "false" : "", true);
      setNewPropertyName("");
      setNewPropertyOptions("");
      setNewPropertyType("text");
      setPropertyPickerOpen(false);
    } catch (cause) {
      setPropertyComposerError(cause instanceof Error ? cause.message : "Unable to create this property.");
    } finally {
      setPropertyComposerBusy(false);
    }
  };

  const addProperty = (name: string) => {
    if (!name || Object.prototype.hasOwnProperty.call(parsedProperties, name)) return;
    updateDraft("properties", JSON.stringify({ ...parsedProperties, [name]: "" }, null, 2));
    setPropertyPickerOpen(false);
  };

  const uploadTemplateCover = async (file: File) => {
    return await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => typeof reader.result === "string" ? resolve(reader.result) : reject(new Error("Unable to read that cover image."));
      reader.onerror = () => reject(new Error("Unable to read that cover image."));
      reader.readAsDataURL(file);
    });
  };

  const saveTemplate = async () => {
    if (!spaceId || saving) return;
    setSaving(true);
    setMessage("");
    setError("");
    try {
      const input = buildTemplateEditorInput(draft);
      if (!input.name) throw new Error("Template name is required.");
      if (templateId) {
        await saveTemplateDraftNow(draft.content);
        if (inLocalMode()) {
          await dataClient.updateTemplate(spaceId, templateId, input);
        } else if (templateDraft) {
          await dataClient.publishTemplateDraft(spaceId, templateId, templateDraft.id);
          setTemplateDraft(null);
        } else {
          await dataClient.updateTemplate(spaceId, templateId, input);
        }
        if (templateDraftStorageKey && typeof window !== "undefined") window.localStorage.removeItem(templateDraftStorageKey);
        setDirty(false);
        setMessage("Template saved. It is available in the command menu.");
        emitTransientSystemNotification({
          title: "Template saved",
          body: "The saved template revision is now available in this space.",
        });
      } else {
        const created = await dataClient.createTemplate(spaceId, input);
        navigate(getTemplateEditorPath(spaceId, created.id), { replace: true });
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to save template.");
    } finally {
      setSaving(false);
    }
  };

  const openRevisions = async () => {
    if (!spaceId || !templateId) return;
    setRevisionsOpen(true);
    setRevisionBusy(true);
    try {
      setRevisions(await dataClient.listTemplateRevisions(spaceId, templateId));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to load template revisions.");
    } finally {
      setRevisionBusy(false);
    }
  };

  const revertRevision = async (revision: TemplateRevisionRecord) => {
    if (!spaceId || !templateId || revisionBusy) return;
    setRevisionBusy(true);
    try {
      await dataClient.revertTemplateRevision(spaceId, templateId, revision.id);
      const refreshed = await dataClient.getTemplate(spaceId, templateId);
      const hydrated = getTemplateEditorDraft(refreshed);
      setDraft(hydrated);
      draftRef.current = hydrated;
      setPageWidth(hydrated.pageWidth ?? "standard");
      setDirty(false);
      setMessage(`Reverted to revision ${revision.revision_number}.`);
      setRevisionsOpen(false);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to revert template revision.");
    } finally {
      setRevisionBusy(false);
    }
  };

  useEffect(() => {
    if (viewMode === "preview" || autosaveStatus === "idle") return;
    const now = Date.now();
    if (now - lastSaveNotificationAtRef.current < 10_000) return;
    lastSaveNotificationAtRef.current = now;
    emitTransientSystemNotification(
      autosaveStatus === "saved"
        ? { title: "Template draft autosaved", body: "Your working template is stored on this device." }
        : autosaveStatus === "error"
          ? { title: "Template draft autosave failed", body: "The editor will retry after the next save window." }
          : { title: "Template draft saving", body: "Your working copy is being stored." },
    );
  }, [autosaveStatus, viewMode]);

  const templateContext = useMemo(
    () => ({ title: draft.name, space: spaceName }),
    [draft.name, spaceName],
  );

  if (loading) {
    return (
      <div
        className="text-text-muted flex h-full items-center justify-center text-sm"
        role="status"
      >
        Loading template editor...
      </div>
    );
  }

  return (
    <div className={`bg-bg text-text flex h-full min-h-0 flex-col ${!mobileLayout ? "workspace-document-page" : ""}`}>
      {revisionsOpen && (
        <DialogShell labelledBy="template-revisions-title" maxWidth="lg" onClose={() => setRevisionsOpen(false)}>
          <div className="border-border flex items-start justify-between border-b px-5 py-4">
            <div>
              <h2 id="template-revisions-title" className="text-text font-semibold">Template revisions</h2>
              <p className="text-text-muted mt-1 text-sm">The ten most recent manually saved revisions are retained.</p>
            </div>
            <button type="button" className="text-text-muted hover:text-text" aria-label="Close revisions" onClick={() => setRevisionsOpen(false)}><X size={16} /></button>
          </div>
          <div className="grid max-h-[28rem] gap-2 overflow-y-auto p-4">
            {revisionBusy && <p className="text-text-muted text-sm">Loading revisions…</p>}
            {!revisionBusy && revisions.length === 0 && <p className="text-text-muted text-sm">No saved revisions yet.</p>}
            {revisions.map((revision) => (
              <div key={revision.id} className="border-border bg-surface-raised rounded-input flex items-center justify-between gap-3 border px-3 py-3">
                <div className="min-w-0">
                  <p className="text-text text-sm font-medium">Revision {revision.revision_number}</p>
                  <p className="text-text-muted text-xs">{new Date(revision.created_at).toLocaleString()} · {revision.created_by}</p>
                  <p className="text-text-muted mt-1 truncate text-xs">{revision.snapshot.name}</p>
                </div>
                <Button type="button" size="sm" variant="secondary" disabled={revisionBusy} onClick={() => void revertRevision(revision)}>Revert</Button>
              </div>
            ))}
          </div>
        </DialogShell>
      )}
      {tagPickerOpen && (
        <DialogShell labelledBy="template-tag-picker-title" maxWidth="md" onClose={() => setTagPickerOpen(false)}>
          <div className="border-border flex items-start justify-between border-b px-5 py-4">
            <div>
              <h2 id="template-tag-picker-title" className="text-text font-semibold">Add default tag</h2>
              <p className="text-text-muted mt-1 text-sm">Choose a space tag to apply to new pages.</p>
            </div>
            <button type="button" className="text-text-muted hover:text-text" aria-label="Close tag picker" onClick={() => setTagPickerOpen(false)}><X size={16} /></button>
          </div>
          <div className="grid max-h-72 gap-2 overflow-y-auto p-4">
            {availableTags
              .filter((tag) => !draft.tags.split(",").map((item) => item.trim().toLowerCase()).includes(tag.name.toLowerCase()))
              .map((tag) => (
                <button key={tag.id} type="button" className="border-border hover:bg-surface-raised rounded-input border px-3 py-3 text-left" onClick={() => addTag(tag.name)}>
                  <span className="text-text block text-sm font-medium">{tag.name}</span>
                  <span className="text-text-muted text-xs">Space tag</span>
                </button>
              ))}
            {availableTags.filter((tag) => !draft.tags.split(",").map((item) => item.trim().toLowerCase()).includes(tag.name.toLowerCase())).length === 0 && <p className="text-text-muted p-2 text-sm">No unused space tags are available. Type a new tag below.</p>}
          </div>
        </DialogShell>
      )}
      {propertyPickerOpen && (
        <DialogShell labelledBy="template-property-picker-title" maxWidth="lg" onClose={() => setPropertyPickerOpen(false)}>
          <div className="border-border flex items-start justify-between border-b px-5 py-4">
            <div><h2 id="template-property-picker-title" className="text-text text-base font-semibold">Add default property</h2><p className="text-text-muted mt-1 text-sm">Choose a property for pages created from this template.</p></div>
            <button type="button" className="text-text-muted hover:text-text" aria-label="Close property picker" onClick={() => setPropertyPickerOpen(false)}><X size={16} /></button>
          </div>
          <form className="grid gap-4 p-5" onSubmit={(event) => { event.preventDefault(); void createDefaultProperty(); }}>
            <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_13rem]">
              <Input value={newPropertyName} onChange={(event) => { setNewPropertyName(event.target.value); setPropertyComposerError(""); }} placeholder="Property name" aria-label="New property name" autoFocus className="h-10" />
              <PropertyTypeSelect value={newPropertyType} onChange={setNewPropertyType} className="h-10" />
            </div>
            {propertyTypeUsesOptions(newPropertyType) && <Input value={newPropertyOptions} onChange={(event) => setNewPropertyOptions(event.target.value)} placeholder="Options separated by commas" aria-label="Property options" />}
            {propertyComposerError && <p role="alert" className="text-danger text-xs">{propertyComposerError}</p>}
            <div className="border-border bg-surface-raised/40 grid gap-2 rounded-input border p-3">
              <p className="text-text text-sm font-medium">Existing properties</p>
              <div className="grid max-h-44 gap-1 overflow-y-auto">
                {propertySchemas.filter((schema) => !Object.prototype.hasOwnProperty.call(parsedProperties, schema.name)).map((schema) => (
                  <button key={schema.id} type="button" className="hover:bg-surface flex min-h-10 items-center justify-between rounded-button px-2.5 text-left text-sm" onClick={() => addProperty(schema.name)}><span className="text-text">{schema.name}</span><span className="text-text-muted text-xs">{schema.property_type}</span></button>
                ))}
                {propertySchemas.filter((schema) => !Object.prototype.hasOwnProperty.call(parsedProperties, schema.name)).length === 0 && <p className="text-text-muted px-2 py-2 text-xs">No unused property schemas are available.</p>}
              </div>
            </div>
            <div className="border-border flex items-center justify-end gap-2 border-t pt-4"><Button type="button" variant="secondary" onClick={() => setPropertyPickerOpen(false)}>Cancel</Button><Button type="submit" disabled={propertyComposerBusy || !newPropertyName.trim()}><Check size={14} /> {propertyComposerBusy ? "Creating…" : "Create property"}</Button></div>
          </form>
        </DialogShell>
      )}
      <header className="workspace-document-header border-border bg-bg shrink-0 border-b" data-document-ui="header">
        <div className="flex min-h-12 flex-wrap items-center gap-2 px-4 py-2 sm:gap-3 sm:px-5">
          <div className="min-w-0 flex-1"><Breadcrumb spaceName={spaceName} title={draft.name || "Template"} spaceHref={spaceId ? `/spaces/${spaceId}/docs` : undefined} className="mb-0" /></div>
          <div role="toolbar" aria-label="Template controls" className="flex min-w-0 shrink-0 flex-wrap items-center justify-end gap-2 sm:gap-2.5">
            {!mobileLayout && headingCount >= 2 && viewMode !== "source" && <button ref={contentsTriggerRef} type="button" className={`${CONTROL_CHROME_ACTION_CLASS} flex h-8 items-center gap-1.5 px-2 text-xs`} aria-label="Contents" aria-pressed={contentsOpen} onClick={() => setContentsOpen(open => !open)}><span>Contents</span></button>}
            <div className={CONTROL_CHROME_RAIL_CLASS}>
              {(
                [
                  ["preview", "Read", Eye],
                  ["edit", "Edit", Pencil],
                  ["source", "Source view", Code2],
                ] as const
              ).map(([mode, label, Icon]) => (
                <button
                  key={mode}
                  type="button"
                  onClick={() => { if (!lockLost) setViewMode(mode); }}
                  className={`${CONTROL_CHROME_ACTION_CLASS} flex items-center justify-center gap-1.5 ${mobileLayout ? "h-11 w-11" : "h-8 px-2"} ${viewMode === mode ? CONTROL_CHROME_ACTIVE_ACTION_CLASS : ""}`}
                  aria-label={label}
                  aria-pressed={viewMode === mode}
                  title={label}
                >
                  <Icon size={14} />
                  <span className={mobileLayout ? "sr-only" : "text-xs"}>{mode === "source" ? "Source" : label}</span>
                </button>
              ))}
            </div>
            <div className={CONTROL_CHROME_RAIL_CLASS}>
              <button type="button" className={`${CONTROL_CHROME_ACTION_CLASS} flex items-center justify-center gap-1.5 ${mobileLayout ? "h-11 w-11" : "h-8 px-2"}`} aria-label="Template details" title="Template details" aria-expanded={templateDetailsOpen} onClick={() => setTemplateDetailsOpen(true)}><SlidersHorizontal size={14} /><span className={mobileLayout ? "sr-only" : "text-xs"}>Details</span></button>
              <button type="button" className={`${CONTROL_CHROME_ACTION_CLASS} flex items-center justify-center gap-1.5 ${mobileLayout ? "h-11 w-11" : "h-8 px-2"}`} aria-label="Test template" title="Test template without creating a page" onClick={() => setTemplateTestOpen(true)}><TestTubeDiagonal size={14} /><span className={mobileLayout ? "sr-only" : "text-xs"}>Test</span></button>
              {templateId && (
                <button type="button" className={`${CONTROL_CHROME_ACTION_CLASS} flex items-center justify-center gap-1.5 ${mobileLayout ? "h-11 w-11" : "h-8 px-2"}`} aria-label="View template revisions" title="View template revisions" onClick={() => void openRevisions()}>
                  <History size={14} />
                  <span className={mobileLayout ? "sr-only" : "text-xs"}>Revisions</span>
                </button>
              )}
              <button
                type="button"
                className={`${CONTROL_CHROME_ACTION_CLASS} text-primary flex items-center justify-center gap-1.5 ${mobileLayout ? "h-11 w-11" : "h-8 px-2"}`}
                aria-label={saving ? "Saving template" : "Save template"}
                title={saving ? "Saving template" : "Save template"}
                onClick={() => void saveTemplate()}
                disabled={saving || lockLost || !draft.name.trim() || (!dirty && Boolean(templateId))}
              >
                <Save size={14} />
                <span className={mobileLayout ? "sr-only" : "text-xs"}>{saving ? "Saving" : "Save"}</span>
              </button>
            </div>
          </div>
        </div>
        <div className="text-text-muted flex flex-wrap items-center justify-between gap-3 px-5 pb-2 text-xs" aria-label="Template status">
          <span>{viewMode === "preview" ? "Read" : viewMode === "source" ? "Source" : "Edit"} · Template for {spaceName}</span>
              <span className={lockLost ? "text-danger" : autosaveStatus === "error" ? "text-danger" : autosaveStatus === "saved" ? "text-success" : "text-text-muted"} aria-live="polite">
              {lockLost ? "Read-only: editing lock lost" : saving ? "Saving template…" : message || (autosaveStatus === "saving" ? "Saving draft…" : autosaveStatus === "saved" ? "Draft saved" : autosaveStatus === "error" ? "Draft save failed" : dirty ? "Unsaved changes" : "Ready")}
            </span>
        </div>
      </header>
      {templateTestOpen && (
        <TemplateExperienceTestDialog
          name={draft.name}
          content={draft.content}
          properties={(() => {
            try { return draft.properties.trim() ? JSON.parse(draft.properties) as Record<string, unknown> : {}; }
            catch { return {}; }
          })()}
          config={templateExperience}
          spaceName={spaceName}
          schemas={propertySchemas}
          tags={assignedTagNames}
          icon={draft.icon ?? ""}
          coverImage={draft.coverImage ?? ""}
          customCss={draft.customCss ?? ""}
          onClose={() => setTemplateTestOpen(false)}
        />
      )}
      {!mobileLayout && viewMode === "edit" && !lockLost && <div ref={setDesktopToolbarHost} className="workspace-document-toolbar border-border bg-bg shrink-0 border-b" data-document-ui="toolbar" />}
      <div className="workspace-document-region relative min-h-0 flex-1" data-contents-eligible={headingCount >= 2 && viewMode !== "source" ? "true" : undefined}>
        <div className="starlane-document-viewport relative h-full min-h-0 flex-1 overflow-y-auto">
      <div
        data-document-page-width={pageWidth}
        data-document-editing={viewMode === "edit" && !lockLost ? "true" : undefined}
        className={`starlane-document-scope mx-auto w-full px-4 py-6 sm:px-6 ${getDocumentPageWidthClass(pageWidth)} ${mobileLayout ? "pb-40" : "pb-16"}`}
      >
        <section aria-labelledby="template-editor-name" className="relative mb-6">
          <CoverImage
            src={draft.coverImage || undefined}
            editable={viewMode === "edit" && !lockLost}
            showEmptyPlaceholder={viewMode === "edit" && !lockLost}
            onUpload={uploadTemplateCover}
            onUpdate={(value) => updateDraft("coverImage", value ?? "")}
          />
          <div className="document-title-row relative flex items-start gap-3">
            <div className="document-title-icon-lane shrink-0">
            <PageIcon
              icon={draft.icon || undefined}
              onUpdate={viewMode === "edit" && !lockLost ? (icon) => updateDraft("icon", icon) : undefined}
            />
            </div>
            <div className="min-w-0 flex-1">
              <label htmlFor="template-editor-name" className="sr-only">
                Template name
              </label>
              {viewMode === "preview" || lockLost ? <h1 id="template-editor-name" className="document-title text-text text-3xl font-semibold">{draft.name || "Untitled template"}</h1> : (
              <Input
                id="template-editor-name"
                value={draft.name}
                onChange={(event) => updateDraft("name", event.target.value)}
                className="document-title-input text-text placeholder:text-text-muted/50 h-auto w-full rounded-none border-transparent bg-transparent p-0 text-3xl font-semibold leading-tight tracking-tight focus:ring-0 focus:ring-offset-0 sm:text-4xl"
                placeholder="Untitled template"
                maxLength={100}
                autoFocus={!templateId}
                readOnly={lockLost}
              />
              )}
              <p className="text-text-muted mt-2 text-sm">Template for {spaceName}</p>
            </div>
          </div>
        </section>


        {error ? (
          <div
            className="border-danger/40 bg-danger/10 text-danger rounded-input mb-4 border px-3 py-2 text-sm"
            role="alert"
          >
            {error}
          </div>
        ) : null}



          <section ref={documentBodyRef} aria-label="Template page content">
            {viewMode === "edit" && !lockLost ? (
                <Editor
                  content={draft.content}
                  onChange={(content) => updateDraft("content", content)}
                  templateAuthoring
                  templateFields={templateExperience.fields.map(({ id, label }) => ({ id, label }))}
                desktopToolbarHost={!mobileLayout ? desktopToolbarHost : null}
                showBlockChrome={!mobileLayout}
                showMobileCommandButton={mobileLayout}
                templates={availableTemplates}
                templateContext={templateContext}
                assetContext={{ space_id: spaceId }}
                spaceId={spaceId}
                onDocumentNavigate={
                  spaceId
                    ? (targetDocumentId) =>
                        navigate(buildDocumentRoutePath(spaceId, targetDocumentId))
                    : undefined
                }
              />
            ) : viewMode === "source" ? (
              <div className="border-border bg-surface rounded-card border">
                <label htmlFor="template-editor-source" className="sr-only">
                  Template Markdown source
                </label>
                <Textarea
                  id="template-editor-source"
                  value={draft.content}
                  onChange={(event) => updateDraft("content", event.target.value)}
                  placeholder="Write Markdown source..."
                  autoFocus={!mobileLayout}
                  className="min-h-[28rem] rounded-card border-0 font-mono text-sm focus:ring-0"
                  spellCheck={false}
                  readOnly={lockLost}
                />
              </div>
            ) : (
              <div>
                <MarkdownPreview
                  content={draft.content}
                  assetContext={{ space_id: spaceId }}
                  onDocumentNavigate={
                    spaceId
                      ? (targetDocumentId) =>
                          navigate(buildDocumentRoutePath(spaceId, targetDocumentId))
                      : undefined
                  }
                />
              </div>
            )}
          </section>
      </div>
        </div>
        {!mobileLayout && viewMode !== "source" && <div className="workspace-document-contents-position" data-document-ui="contents"><DocumentContents containerRef={documentBodyRef} contentKey={`${templateId ?? "new"}:${viewMode}:${loading}`} open={contentsOpen} onClose={() => {setContentsOpen(false); contentsTriggerRef.current?.focus({preventScroll:true});}} onHeadingsChange={setHeadingCount} /></div>}
      </div>
      {templateDetailsOpen && !tagPickerOpen && !propertyPickerOpen && (
        <DialogShell
          labelledBy={customCssDialogOpen ? "template-custom-css-title" : "template-details-title"}
          presentation={customCssDialogOpen ? "centered" : "drawer-right"}
          maxWidth="lg"
          onClose={() => customCssDialogOpen ? setCustomCssDialogOpen(false) : setTemplateDetailsOpen(false)}
        >
          {customCssDialogOpen ? (
            <TemplateCssRecoveryContent value={draft.customCss ?? ""} textareaRef={retainedCssTextRef} onClose={() => setCustomCssDialogOpen(false)} />
          ) : (
          <>
          <DialogHeader><DialogTitle id="template-details-title">Template details</DialogTitle><DialogCloseAction label="Close template details" onClick={() => setTemplateDetailsOpen(false)} /></DialogHeader>
          <DialogBody className="min-h-0 overflow-y-auto p-0">
            <div className="border-border flex flex-wrap items-center justify-between gap-3 border-b px-5 py-4 sm:px-6">
              <p className="text-text-muted max-w-2xl text-sm">
                Set the metadata and presentation that new pages inherit from this template.
              </p>
              {hasRetainedTemplateCss(draft) && (
                <Button ref={retainedCssTriggerRef} type="button" variant="secondary" onClick={() => setCustomCssDialogOpen(true)}><Code2 size={14} /> View retained CSS</Button>
              )}
            </div>
            <div className="px-5 sm:px-6">
              <section className="border-border border-b py-5" aria-labelledby="template-description-heading">
                <h2 id="template-description-heading" className="text-text mb-1 text-sm font-semibold">About this template</h2>
                <p className="text-text-muted mb-3 text-xs">Tell people when this template is useful.</p>
                <label
                  htmlFor="template-editor-description"
                  className="text-text-muted mb-1 block text-xs font-medium"
                >
                  Description
                </label>
                <Textarea
                  id="template-editor-description"
                  value={draft.description}
                  onChange={(event) => updateDraft("description", event.target.value)}
                  placeholder="Explain when this template should be used"
                  maxLength={1000}
                  className="min-h-28 resize-y"
                  readOnly={lockLost}
                />
              </section>
              <section className="border-border border-b py-5" aria-labelledby="template-fields-heading">
                <div className="mb-3 flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <h2 id="template-fields-heading" className="text-text mb-1 text-sm font-semibold">Named fields</h2>
                    <p className="text-text-muted text-xs">Collect values once for the title, body markers, and linked properties.</p>
                  </div>
                  <Button type="button" size="sm" variant="secondary" onClick={addTemplateField} disabled={lockLost || !templateExperienceSupported}><Plus size={14} /> Add field</Button>
                </div>
                {!templateExperienceSupported && <p role="status" className="text-warning border-warning/30 bg-warning/10 rounded-input border px-3 py-2 text-xs">This template uses a newer field format. Its metadata is preserved, but field editing is unavailable in this version.</p>}
                {templateExperienceSupported && templateExperience.fields.length > 0 && <p className="text-text-muted mb-1 text-xs">In the editor, use Insert → Template fields to place a field in the body.</p>}
                <div className="grid gap-4">
                  {templateExperience.fields.map((field) => {
                    const selectedSchemaId = field.bindings?.find((binding) => binding.target === "property")?.property_id ?? "";
                    const selectedSchema = propertySchemas.find((schema) => schema.id === selectedSchemaId);
                    const fieldOptions = getTemplateFieldOptions(field, propertySchemas);
                    const compatibleSchemas = propertySchemas.filter((schema) =>
                      field.type === "text" || field.type === "multiline" ? schema.property_type === "text" :
                      field.type === "date" ? schema.property_type === "date" : schema.property_type === "select",
                    );
                    const titleBound = templateExperience.title?.some((part) => part.kind === "field" && part.field_id === field.id) ?? false;
                    return (
                      <div key={field.id} className="border-border grid min-w-0 gap-3 border-t pt-4">
                        <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-3">
                          <div className="col-span-2 flex items-center justify-between gap-3">
                            <label htmlFor={`template-field-name-${field.id}`} className="text-text-muted text-xs font-medium">Field name</label>
                            <Button type="button" size="sm" variant="secondary" className="text-danger hover:text-danger" aria-label={`Delete ${field.label} field`} disabled={lockLost} onClick={() => removeTemplateField(field.id)}><Trash2 size={14} aria-hidden="true" />Delete field</Button>
                          </div>
                          <div className="col-span-2 min-w-0">
                            <Input id={`template-field-name-${field.id}`} value={field.label} maxLength={120} aria-label={`${field.label} field label`} onChange={(event) => updateTemplateField(field.id, { label: event.target.value })} readOnly={lockLost} />
                          </div>
                          <label className="text-text-muted col-span-2 grid min-w-0 gap-1 text-xs font-medium">Field type
                            <Select aria-label={`Type for ${field.label}`} value={field.type} disabled={lockLost} onChange={(event) => updateTemplateField(field.id, { type: event.target.value as TemplateFieldDefinition["type"], options: event.target.value === "dropdown" ? field.options ?? [] : undefined })}>
                            <option value="text">Text</option><option value="multiline">Multiline text</option><option value="date">Date</option><option value="dropdown">Dropdown</option>
                            </Select>
                          </label>
                        </div>
                        <label className="text-text-muted grid gap-1 text-xs font-medium">Help text
                          <Textarea value={field.help_text ?? ""} maxLength={500} aria-label={`Help text for ${field.label}`} placeholder="Shown below this field when a page is created" className="min-h-16" onChange={(event) => updateTemplateField(field.id, { help_text: event.target.value })} readOnly={lockLost} />
                        </label>
                        {field.type === "multiline" ? (
                          <label className="text-text-muted grid gap-1 text-xs font-medium">Default value
                            <Textarea value={field.default_value ?? ""} aria-label={`Default value for ${field.label}`} placeholder="Optional" className="min-h-20" onChange={(event) => updateTemplateField(field.id, { default_value: event.target.value })} readOnly={lockLost} />
                          </label>
                        ) : (
                          <label className="text-text-muted grid gap-1 text-xs font-medium">Default value
                            <Input type={field.type === "date" ? "date" : "text"} value={field.default_value ?? ""} aria-label={`Default value for ${field.label}`} placeholder="Optional" onChange={(event) => updateTemplateField(field.id, { default_value: event.target.value })} readOnly={lockLost} />
                          </label>
                        )}
                        {field.type === "dropdown" && (selectedSchema ? <p className="text-text-muted text-xs">Options come from the linked {selectedSchema.name} property: {fieldOptions.join(", ") || "No options configured"}.</p> : <label className="text-text-muted grid gap-1 text-xs font-medium">Dropdown options
                          <Input value={(field.options ?? []).join(", ")} aria-label={`Options for ${field.label}`} placeholder="Separate options with commas" onChange={(event) => updateTemplateField(field.id, { options: event.target.value.split(",").map((option) => option.trim()).filter(Boolean) })} readOnly={lockLost} />
                        </label>)}
                        <div className="flex flex-wrap gap-x-5 gap-y-1">
                        <label className="text-text-muted flex min-h-9 items-center gap-2 text-xs">
                          <input type="checkbox" checked={Boolean(field.required)} disabled={lockLost} onChange={(event) => updateTemplateField(field.id, { required: event.target.checked })} />
                          Required when creating a page
                        </label>
                        <label className="text-text-muted flex min-h-9 items-center gap-2 text-xs">
                          <input type="checkbox" checked={titleBound} disabled={lockLost} onChange={(event) => setTitleFields(field.id, event.target.checked)} />
                          Include in generated title
                        </label>
                        </div>
                        <label className="text-text-muted grid gap-1 text-xs font-medium">Linked property
                          <Select aria-label={`Property binding for ${field.label}`} value={selectedSchemaId} disabled={lockLost} onChange={(event) => updateTemplateField(field.id, { bindings: event.target.value ? [{ target: "property", property_id: event.target.value }] : [] })}>
                          <option value="">No property binding</option>
                          {compatibleSchemas.map((schema) => <option key={schema.id} value={schema.id}>{schema.name}</option>)}
                          </Select>
                        </label>
                        <p className="text-text-muted text-[11px]">Field identifier: <span className="font-mono">{field.id}</span></p>
                      </div>
                    );
                  })}
                  {templateExperience.fields.length === 0 && <p className="text-text-muted rounded-input border border-dashed p-3 text-xs">No fields yet. Templates without fields keep a direct create path.</p>}
                </div>
                {templateExperience.fields.length > 0 && (
                  <div className="mt-3 grid gap-2">
                    <label htmlFor="template-title-prefix" className="text-text-muted text-xs font-medium">Generated title prefix</label>
                    <Input id="template-title-prefix" value={templateTitlePrefix(templateExperience)} placeholder="Optional title text before selected fields" readOnly={lockLost} onChange={(event) => {
                      updateTemplateExperience({ ...templateExperience, title: buildTemplateTitle(event.target.value, templateTitleFieldIds(templateExperience)) });
                    }} />
                    <p className="text-text-muted text-[11px]">Selected field values appear after the prefix, separated by an em dash.</p>
                  </div>
                )}
              </section>
              <section className="border-border border-b py-5" aria-labelledby="template-tags-heading">
                <h2 id="template-tags-heading" className="text-text mb-1 text-sm font-semibold">Default tags</h2>
                <p className="text-text-muted mb-3 text-xs">Organize pages created from this template.</p>
                <div className="relative">
                  <div className="border-border bg-bg focus-within:ring-primary/40 flex min-h-10 flex-wrap items-center gap-1.5 rounded-input border px-2 py-1.5 focus-within:ring-2">
                    {assignedTagNames.map((tagName) => {
                      const tag = availableTags.find((item) => item.name.toLowerCase() === tagName.toLowerCase());
                      return (
                        <span key={tagName} className="bg-surface-raised text-text inline-flex items-center gap-1 rounded-button border px-2 py-1 text-xs">
                          <span className="h-2.5 w-2.5 rounded-sm" style={{ backgroundColor: tag?.color ?? "#6b7280" }} />
                          {tagName}
                          <button type="button" className="text-text-muted hover:text-text" aria-label={`Remove ${tagName}`} onClick={() => removeTag(tagName)}><X size={12} /></button>
                        </span>
                      );
                    })}
                    <Search size={14} className="text-text-muted" aria-hidden="true" />
                    <Input
                      id="template-editor-tags"
                      value={newTagName}
                      onChange={(event) => { setNewTagName(event.target.value); setTagComposerError(""); }}
                      onFocus={() => setTagInputFocused(true)}
                      onBlur={() => window.setTimeout(() => setTagInputFocused(false), 120)}
                      onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); void addTag(newTagName); } }}
                      placeholder="Add or search tags"
                      className="h-7 min-w-24 flex-1 border-0 bg-transparent p-0 text-xs focus:ring-0"
                      autoComplete="off"
                      aria-label="Add or search template tags"
                    />
                    <button type="button" className="bg-primary text-primary-foreground rounded-button flex h-7 w-7 items-center justify-center disabled:opacity-40" aria-label="Add tag" disabled={!newTagName.trim() || tagComposerBusy} onClick={() => void addTag(newTagName)}><Plus size={14} /></button>
                  </div>
                  {(tagInputFocused || newTagName.trim()) && (
                    <div className="border-border bg-surface-raised shadow-card absolute inset-x-0 top-[calc(100%+0.35rem)] z-30 max-h-52 overflow-y-auto rounded-input border p-1">
                      {matchingTags.map((tag) => <button key={tag.id} type="button" className="hover:bg-surface flex min-h-9 w-full items-center gap-2 rounded-button px-2 text-left text-xs" onMouseDown={(event) => event.preventDefault()} onClick={() => void addTag(tag.name)}><span className="h-3 w-3 rounded-sm" style={{ backgroundColor: tag.color }} />{tag.name}<span className="text-text-muted ml-auto">Add</span></button>)}
                      {newTagName.trim() && matchingTags.length === 0 && <button type="button" className="hover:bg-surface text-text flex min-h-9 w-full items-center gap-2 rounded-button px-2 text-left text-xs" onMouseDown={(event) => event.preventDefault()} onClick={() => void addTag(newTagName)}><Plus size={13} className="text-primary" />Create “{newTagName.trim()}”</button>}
                    </div>
                  )}
                </div>
                {tagComposerError && <p role="alert" className="text-danger mt-1 text-xs">{tagComposerError}</p>}
                <p className="text-text-muted mt-1 text-xs">
                  Choose an existing space tag or type a new one, then press Enter.
                </p>
              </section>
              <section className="py-5" aria-labelledby="template-properties-heading">
                <h2 id="template-properties-heading" className="text-text mb-1 text-sm font-semibold">Default properties</h2>
                <p className="text-text-muted mb-3 text-xs">Provide starting values for structured metadata.</p>
                <div className="mb-1 flex items-center justify-between gap-2">
                  <label
                    htmlFor="template-editor-properties"
                    className="text-text-muted block text-xs font-medium"
                  >
                    Properties
                  </label>
                  <Button type="button" size="sm" variant="secondary" onClick={() => { setPropertyComposerError(""); setPropertyPickerOpen(true); }}>Add property</Button>
                </div>
                <div className="border-border bg-bg rounded-input grid gap-2 border p-2">
                  {Object.entries(parsedProperties).length > 0 ? (
                    Object.entries(parsedProperties).map(([name, value]) => (
                      <div key={name} className="flex items-center gap-2">
                        <span className="text-text-muted min-w-28 text-xs">{name}</span>
                        <Input
                          value={typeof value === "string" ? value : JSON.stringify(value)}
                          onChange={(event) => setPropertyValue(name, event.target.value, true)}
                          aria-label={`Default value for ${name}`}
                        />
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          className="px-2"
                          aria-label={`Remove ${name}`}
                          onClick={() => setPropertyValue(name, "", false)}
                        >
                          <X size={14} />
                        </Button>
                      </div>
                    ))
                  ) : (
                    <p className="text-text-muted px-1 py-2 text-xs">
                      No default properties yet. Add one from the menu.
                    </p>
                  )}
                </div>
                <p className="text-text-muted mt-1 text-xs">
                  Defaults are applied to pages created from this template.
                </p>
              </section>
            </div>
          <details className="border-border border-t px-5 py-3 sm:px-6">
            <summary className="text-text-muted hover:text-text flex min-h-8 cursor-pointer list-none items-center gap-2 text-xs">
              <span>Advanced details</span>
              <ChevronDown size={13} aria-hidden="true" />
            </summary>
            <dl className="text-text-muted grid gap-x-5 gap-y-1 pt-2 text-xs sm:grid-cols-2 lg:grid-cols-3">
              <div><dt className="sr-only">Created</dt><dd>Created {templateDraft ? formatRelativeTimestamp(templateDraft.created_at) : "just now"}</dd></div>
              <div><dt className="sr-only">Last edited</dt><dd>Last edited {templateDraft ? formatRelativeTimestamp(templateDraft.updated_at) : "just now"}</dd></div>
              <div><dt className="sr-only">Auto-save</dt><dd>Auto-save {lastAutoSavedAt ? formatRelativeTimestamp(lastAutoSavedAt) : "pending"}</dd></div>
              {templateDraft && <div className="flex items-center gap-1 sm:col-span-2 lg:col-span-1"><dt className="sr-only">Draft ID</dt><dd className="font-mono truncate">Draft ID {templateDraft.id}</dd><button type="button" className="text-text-muted hover:text-text rounded-button p-1" aria-label="Copy draft ID" title="Copy draft ID" onClick={() => void navigator.clipboard?.writeText(templateDraft.id)}><Copy size={12} /></button></div>}
              {!inLocalMode() && templateDraft?.lock_expires_at && <div><dt className="sr-only">Editing lock</dt><dd>{lockLost ? "Editing lock lost" : `Editing lock until ${new Date(templateDraft.lock_expires_at).toLocaleTimeString()}`}</dd></div>}
            </dl>
          </details>
          </DialogBody>
          </>
          )}
        </DialogShell>
      )}
    </div>
  );
}

export function hasRetainedTemplateCss(draft: TemplateEditorDraft): boolean {
  return typeof draft.customCss === "string" && draft.customCss.length > 0;
}

export function TemplateCssRecoveryDialog({ value, onClose }: { value: string; onClose: () => void }) {
  return (
    <DialogShell labelledBy="template-custom-css-title" maxWidth="lg" onClose={onClose}>
      <TemplateCssRecoveryContent value={value} onClose={onClose} />
    </DialogShell>
  );
}

export function TemplateCssRecoveryContent({ value, onClose, textareaRef }: {
  value: string;
  onClose: () => void;
  textareaRef?: RefObject<HTMLTextAreaElement | null>;
}) {
  return (
    <>
      <DialogHeader>
        <DialogTitle id="template-custom-css-title">Retained template CSS</DialogTitle>
        <DialogCloseAction label="Close retained template CSS" onClick={onClose} />
      </DialogHeader>
      <DialogBody className="grid min-h-0 gap-3 overflow-y-auto">
        <p id="template-custom-css-description" className="text-text-muted text-sm">
          The original stored CSS is retained for compatibility and recovery. Select and copy it below.
          Template CSS authoring is retired; ordinary template saves preserve this text.
        </p>
        <Textarea ref={textareaRef} value={value} readOnly spellCheck={false} aria-label="Retained template CSS" aria-describedby="template-custom-css-description" className="min-h-64 resize-y font-mono text-xs leading-5" />
        <p className="text-text-muted text-xs">Retained text is not a guarantee of styling. Existing rendering safety checks still apply.</p>
        <div className="flex justify-end"><Button type="button" variant="secondary" onClick={onClose}>Close</Button></div>
      </DialogBody>
    </>
  );
}

function TemplateExperienceTestDialog({
  name,
  content,
  properties,
  config,
  spaceName,
  schemas,
  tags,
  icon,
  coverImage,
  customCss,
  onClose,
}: {
  name: string;
  content: string;
  properties: Record<string, unknown>;
  config: TemplateExperienceConfig;
  spaceName: string;
  schemas: PropertySchemaRecord[];
  tags: string[];
  icon: string;
  coverImage: string;
  customCss: string;
  onClose: () => void;
}) {
  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(config.fields.map((field) => [field.id, field.default_value ?? ""])),
  );
  const resolved = useMemo(() => resolveTemplateExperience({
    content,
    properties,
    config,
    context: { title: name, space: spaceName },
    fieldValues: values,
    schemas,
  }), [config, content, name, properties, schemas, spaceName, values]);

  return (
    <DialogShell labelledBy="template-test-title" maxWidth="workspace" onClose={onClose}>
      <div className="flex max-h-[calc(100dvh-2rem-2px)] min-h-0 flex-col">
      <DialogHeader className="shrink-0">
        <div className="min-w-0">
          <DialogTitle id="template-test-title">Test template</DialogTitle>
          <p className="text-text-muted mt-1 text-xs">Test mode · Nothing is saved and no page will be created.</p>
        </div>
        <DialogCloseAction label="Close template test" onClick={onClose} />
      </DialogHeader>
      <DialogBody className={`grid min-h-0 flex-1 gap-4 overflow-y-auto ${config.fields.length > 0 ? "lg:grid-cols-[minmax(16rem,0.35fr)_minmax(0,1fr)]" : ""}`}>
        <section className="grid content-start gap-3" aria-label="Sample field values">
          <h3 className="text-text text-sm font-semibold">Sample values</h3>
          {config.fields.map((field) => (
            <div key={field.id} className="grid gap-1.5">
              <label htmlFor={`template-test-${field.id}`} className="text-text text-xs font-medium">
                {field.label}{field.required ? <span className="text-danger"> *</span> : null}
              </label>
              {field.type === "multiline" ? (
                <Textarea id={`template-test-${field.id}`} value={values[field.id] ?? ""} onChange={(event) => setValues((current) => ({ ...current, [field.id]: event.target.value }))} className="min-h-20" />
              ) : field.type === "dropdown" ? (
                <Select id={`template-test-${field.id}`} aria-label={field.label} value={values[field.id] ?? ""} onChange={(event) => setValues((current) => ({ ...current, [field.id]: event.target.value }))}>
                  <option value="">Choose…</option>
                  {getTemplateFieldOptions(field, schemas).map((option) => <option key={option} value={option}>{option}</option>)}
                </Select>
              ) : (
                <Input id={`template-test-${field.id}`} type={field.type === "date" ? "date" : "text"} value={values[field.id] ?? ""} onChange={(event) => setValues((current) => ({ ...current, [field.id]: event.target.value }))} />
              )}
              {field.help_text && <p className="text-text-muted text-xs">{field.help_text}</p>}
            </div>
          ))}
          {config.fields.length === 0 && <p className="text-text-muted text-sm">This template has no named fields. Legacy automatic tokens are still resolved in the preview.</p>}
          {resolved.unresolvedFields.length > 0 && (
            <div role="alert" className="border-danger/30 bg-danger/10 text-danger rounded-input border p-3 text-xs">
              <p className="font-medium">The sample needs attention</p>
              <ul className="mt-1 list-inside list-disc">{resolved.unresolvedFields.map((field, index) => <li key={`${field.id}:${index}`}>{field.label}: {field.reason}</li>)}</ul>
            </div>
          )}
        </section>
        <section className="grid min-w-0 content-start gap-3" aria-label="Resolved template preview">
          <h3 className="text-text text-sm font-semibold">Result preview</h3>
          <div className="border-border bg-surface rounded-input border p-3">
            <p className="text-text-muted text-xs font-medium">Generated title</p>
            <p className="text-text mt-1 text-base font-semibold">{resolved.title || name || "Untitled page"}</p>
          </div>
          <div className="border-border bg-surface min-h-32 rounded-input border p-3">
            <MarkdownPreview content={resolved.content} />
          </div>
          {resolved.property_values.length > 0 && (
            <div className="border-border bg-surface rounded-input border p-3">
              <p className="text-text text-xs font-semibold">Field property values</p>
              <dl className="mt-2 grid gap-2">{resolved.property_values.map((value) => <div key={value.property_id} className="flex justify-between gap-3 text-xs"><dt className="text-text-muted">{schemas.find((schema) => schema.id === value.property_id)?.name ?? "Unknown property"}</dt><dd className="text-text break-words text-right">{value.value || "(empty)"}</dd></div>)}</dl>
            </div>
          )}
          {resolved.guidance.length > 0 && (
            <div className="border-warning/30 bg-warning/10 rounded-input border p-3">
              <p className="text-text text-xs font-semibold">Editing guidance · excluded from reading output</p>
              <ul className="text-text-muted mt-2 grid gap-1 text-xs">{resolved.guidance.map((item) => <li key={item.id}>{item.text}</li>)}</ul>
            </div>
          )}
          <div className="border-border bg-surface rounded-input border p-3">
            <p className="text-text text-xs font-semibold">Template defaults</p>
            <dl className="mt-2 grid gap-2 text-xs">
              <div className="flex justify-between gap-3"><dt className="text-text-muted">Tags</dt><dd className="text-text text-right">{tags.join(", ") || "None"}</dd></div>
              {Object.entries(resolved.property_defaults).map(([key, value]) => <div key={key} className="flex justify-between gap-3"><dt className="text-text-muted">{key}</dt><dd className="text-text break-words text-right">{typeof value === "string" ? value || "(empty)" : JSON.stringify(value)}</dd></div>)}
            </dl>
          </div>
          <div className="border-border bg-surface rounded-input border p-3">
            <p className="text-text text-xs font-semibold">Presentation</p>
            <dl className="text-text-muted mt-2 grid gap-1 text-xs">
              <div className="flex justify-between gap-3"><dt>Page icon</dt><dd className="text-text">{icon || "None"}</dd></div>
              <div className="flex justify-between gap-3"><dt>Cover image</dt><dd className="text-text">{coverImage ? "Configured" : "None"}</dd></div>
              <div className="flex justify-between gap-3"><dt>Custom CSS</dt><dd className="text-text">{customCss.trim() ? "Configured" : "None"}</dd></div>
            </dl>
          </div>
        </section>
      </DialogBody>
      <div className="border-border bg-bg/35 flex shrink-0 justify-end border-t px-5 py-3"><Button type="button" onClick={onClose}>Done</Button></div>
      </div>
    </DialogShell>
  );
}

function readStoredTemplateDraft(key: string): StoredTemplateEditorDraft | null {
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<TemplateEditorDraft> & { savedAt?: string };
    if (!parsed.savedAt || typeof parsed.savedAt !== "string") return null;
    const { savedAt, ...draft } = parsed;
    return { savedAt, draft };
  } catch {
    return null;
  }
}

function formatRelativeTimestamp(value: string): string {
  const elapsed = Math.max(0, Date.now() - new Date(value).getTime());
  const minutes = Math.floor(elapsed / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}
