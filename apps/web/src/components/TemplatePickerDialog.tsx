import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, FileText } from "lucide-react";
import type { PropertySchemaRecord, TemplateRecord } from "@/lib/dataClient";
import {
  getTemplateExperience,
  getTemplateFieldOptions,
  resolveTemplateExperience,
  type TemplateFieldValues,
  type TemplateRenderContext,
} from "@/lib/templateExperience";
import { MarkdownPreview } from "@/components/editor/MarkdownPreview";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SearchInput } from "@/components/ui/search-input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import {
  DialogBody,
  DialogCloseAction,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";

interface TemplatePickerDialogProps {
  operation?: "create" | "insert";
  templates: TemplateRecord[];
  schemas?: PropertySchemaRecord[];
  destination?: string;
  initialTitle?: string;
  initialTemplateId?: string;
  renderContext?: TemplateRenderContext;
  loading?: boolean;
  loadError?: string;
  onRetry?: () => void;
  onClose: () => void;
  onSelect: (
    template?: TemplateRecord,
    fieldValues?: TemplateFieldValues,
    sourceId?: string,
  ) => void | boolean | Promise<void | boolean>;
  allowBlank?: boolean;
}

function createOperationId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `template-create-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

/** Shared by desktop and mobile so discovery, preview, and field entry behave the same. */
export function TemplatePickerDialog({
  operation = "create",
  templates,
  schemas = [],
  destination = "Space root",
  initialTitle,
  initialTemplateId,
  renderContext = {},
  loading = false,
  loadError,
  onRetry,
  onClose,
  onSelect,
  allowBlank = true,
}: TemplatePickerDialogProps) {
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [valuesByTemplateId, setValuesByTemplateId] = useState<Record<string, TemplateFieldValues>>({});
  const [submitError, setSubmitError] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [sourceId, setSourceId] = useState("");
  const selected = templates.find((template) => template.id === selectedId);
  const fieldValues = selectedId ? valuesByTemplateId[selectedId] ?? {} : {};
  const selectedConfig = selected ? getTemplateExperience(selected) : undefined;
  const resolved = selected && selectedConfig
    ? resolveTemplateExperience({
        template: selected,
        config: selectedConfig,
        context: { ...renderContext, title: initialTitle || renderContext.title },
        fieldValues,
        schemas,
      })
    : undefined;
  const filteredTemplates = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    if (!normalizedQuery) return templates;
    return templates.filter((template) =>
      `${template.name}\n${template.description ?? ""}\n${template.content}`
        .toLocaleLowerCase()
        .includes(normalizedQuery),
    );
  }, [query, templates]);

  const selectForPreview = (template: TemplateRecord) => {
    if (selectedId !== template.id && !sourceId) setSubmitError("");
    if (!valuesByTemplateId[template.id]) {
      const config = getTemplateExperience(template);
      setValuesByTemplateId((current) => ({ ...current, [template.id]: Object.fromEntries(
        config.fields.map((field) => [field.id, field.default_value ?? ""]),
      ) }));
    }
    setSelectedId(template.id);
  };

  useEffect(() => {
    const template = templates.find((candidate) => candidate.id === initialTemplateId);
    if (!template || selectedId) return;
    const config = getTemplateExperience(template);
    setValuesByTemplateId((current) => current[template.id] ? current : ({
      ...current,
      [template.id]: Object.fromEntries(config.fields.map((field) => [field.id, field.default_value ?? ""])),
    }));
    setSelectedId(template.id);
  }, [initialTemplateId, selectedId, templates]);

  const updateFieldValue = (fieldId: string, value: string) => {
    if (!selectedId || sourceId) return;
    setValuesByTemplateId((current) => ({
      ...current,
      [selectedId]: { ...(current[selectedId] ?? {}), [fieldId]: value },
    }));
  };

  const submit = async (template?: TemplateRecord) => {
    if (submitting) return;
    if (operation === "insert") {
      setSubmitting(true);
      setSubmitError("");
      try {
        const accepted = await onSelect(template);
        if (accepted === false) setSubmitError("The section could not be inserted. The current page is unchanged.");
      } catch {
        setSubmitError("The section could not be inserted. The current page is unchanged.");
      } finally {
        setSubmitting(false);
      }
      return;
    }
    const templateConfig = template ? getTemplateExperience(template) : undefined;
    const result = template && templateConfig
      ? resolveTemplateExperience({
          template,
          config: templateConfig,
          context: { ...renderContext, title: initialTitle || renderContext.title },
          fieldValues,
          schemas,
        })
      : undefined;
    if (result?.unresolvedFields.length) {
      setSubmitError(result.unresolvedFields.map((field) => `${field.label}: ${field.reason}`).join(" "));
      return;
    }

    setSubmitting(true);
    setSubmitError("");
    const operationId = sourceId || createOperationId();
    setSourceId(operationId);
    try {
      const created = await onSelect(template, fieldValues, operationId);
      if (created !== false) return;
      setSubmitError("The page could not be created. Your answers are still here; try again.");
    } catch {
      setSubmitError("The page could not be created. Your answers are still here; try again.");
    } finally {
      setSubmitting(false);
    }
  };

  const fieldBindingLabel = (fieldId: string) => {
    const field = selectedConfig?.fields.find((candidate) => candidate.id === fieldId);
    const boundNames = (field?.bindings ?? []).map((binding) =>
      schemas.find((schema) => schema.id === binding.property_id)?.name,
    ).filter((name): name is string => Boolean(name));
    return boundNames.length ? `Also fills ${boundNames.join(", ")}` : undefined;
  };

  const suggestedTitle = resolved?.title || selected?.name || initialTitle || "Untitled";
  const finalTitle = initialTitle?.trim() || suggestedTitle;

  return (
    <DialogShell
      labelledBy="template-picker-title"
      describedBy="template-picker-description"
      maxWidth="workspace"
      onClose={onClose}
    >
      <div data-dialog-frame="" className="flex max-h-[calc(100dvh-2rem-2px)] min-h-[min(24rem,calc(100dvh-2rem-2px))] flex-col">
        <DialogHeader className="px-4 py-3">
          <div className="min-w-0">
            <DialogTitle id="template-picker-title" className="text-sm">
              {selected
                ? operation === "insert" ? "Preview reusable section" : "Preview template"
                : operation === "insert" ? "Insert reusable section" : "New page"}
            </DialogTitle>
            <DialogDescription id="template-picker-description" className="mt-0 text-xs">
              {selected
                ? operation === "insert"
                  ? "Review the content before inserting it into the current page."
                  : "Review the starting content and any required details."
                : operation === "insert"
                  ? "Find reusable content for the current page."
                  : "Find a starting point for your page."}
            </DialogDescription>
          </div>
          <DialogCloseAction label="Close template picker" onClick={onClose} />
        </DialogHeader>

        <div className="border-border flex flex-wrap gap-x-5 gap-y-1 border-b px-4 py-2 text-xs">
          <span className="text-text-muted">{operation === "insert" ? "Current page" : "Destination"} <span className="text-text">{destination}</span></span>
          {operation === "create" && <span className="text-text-muted">Page title <span className="text-text">{finalTitle}</span></span>}
        </div>

        {!selected ? (
          <>
            <DialogBody className="min-h-0 flex-1 space-y-3 overflow-y-auto p-3">
              <SearchInput
                aria-label="Search templates"
                placeholder="Search templates"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
              {operation === "create" && allowBlank && (
                <button
                  type="button"
                  onClick={() => void submit()}
                  disabled={submitting}
                  className="flex min-h-11 w-full items-center gap-2 rounded-button border border-border px-3 py-2 text-left text-sm text-text transition-colors duration-100 hover:bg-surface-raised focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
                >
                  <FileText size={16} className="shrink-0 text-text-muted" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">Blank page</span>
                    <span className="block truncate text-xs text-text-muted">Start with an empty page</span>
                  </span>
                  <span className="text-xs text-text-muted">Create</span>
                </button>
              )}
              {loading && <p role="status" className="px-2 py-4 text-sm text-text-muted">Loading templates…</p>}
              {loadError && (
                <div role="alert" className="space-y-2 rounded-input border border-danger/30 bg-danger/10 p-3 text-sm text-danger">
                  <p>{loadError}</p>
                  {onRetry && <Button type="button" variant="secondary" size="sm" onClick={onRetry}>Retry</Button>}
                </div>
              )}
              {!loading && !loadError && filteredTemplates.length === 0 && (
                <p className="px-2 py-5 text-sm text-text-muted">
                  {query.trim() ? "No templates match that search." : allowBlank ? "No saved templates yet. You can start with a blank page." : "No saved templates are available in this space."}
                </p>
              )}
              <div className="space-y-1">
                {filteredTemplates.map((template) => (
                  <button
                    key={template.id}
                    type="button"
                    onClick={() => selectForPreview(template)}
                    className="flex min-h-11 w-full items-center gap-2 rounded-button px-3 py-2 text-left text-sm text-text transition-colors duration-100 hover:bg-surface-raised focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
                  >
                    <FileText size={16} className="shrink-0 text-text-muted" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium">{template.name}</span>
                      <span className="block truncate text-xs text-text-muted">{template.description || "Template"}</span>
                    </span>
                    <span className="text-xs text-text-muted">Preview</span>
                  </button>
                ))}
              </div>
              {submitError && <p role="alert" className="text-sm text-danger">{submitError}</p>}
            </DialogBody>
            <DialogFooter className="px-4 py-3">
              <Button type="button" variant="secondary" onClick={onClose}>Cancel</Button>
            </DialogFooter>
          </>
        ) : (
          <>
            <DialogBody className="min-h-0 flex-1 space-y-4 overflow-y-auto p-4">
              <div>
                <button
                  type="button"
                  onClick={() => { setSelectedId(null); setSubmitError(""); }}
                  className="mb-2 inline-flex min-h-9 items-center gap-1 rounded-button px-2 text-sm text-text-muted hover:bg-surface-raised hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
                >
                  <ArrowLeft size={14} aria-hidden="true" />
                  All templates
                </button>
                <h3 className="text-text text-base font-semibold">{selected.name}</h3>
                {selected.description && <p className="mt-1 text-sm text-text-muted">{selected.description}</p>}
              </div>
              {operation === "create" && selectedConfig?.fields.length ? (
                <fieldset className="space-y-3">
                  <legend className="text-sm font-medium text-text">Template details</legend>
                  {selectedConfig.fields.map((field) => {
                    const id = `template-field-${field.id}`;
                    const helpId = `${id}-help`;
                    const bindingLabel = fieldBindingLabel(field.id);
                    return (
                      <div key={field.id} className="space-y-1.5">
                        <label htmlFor={id} className="text-sm font-medium text-text">
                          {field.label}{field.required ? <span className="text-danger"> *</span> : null}
                        </label>
                        {field.type === "multiline" ? (
                          <Textarea
                            id={id}
                            aria-describedby={field.help_text || bindingLabel ? helpId : undefined}
                            value={fieldValues[field.id] ?? ""}
                            onChange={(event) => updateFieldValue(field.id, event.target.value)}
                            disabled={Boolean(sourceId)}
                            placeholder={field.help_text || "Enter a value"}
                          />
                        ) : field.type === "dropdown" ? (
                          <Select
                            id={id}
                            aria-describedby={field.help_text || bindingLabel ? helpId : undefined}
                            value={fieldValues[field.id] ?? ""}
                            onChange={(event) => updateFieldValue(field.id, event.target.value)}
                            disabled={Boolean(sourceId)}
                          >
                            <option value="">Choose an option</option>
                            {getTemplateFieldOptions(field, schemas).map((option) => <option key={option} value={option}>{option}</option>)}
                          </Select>
                        ) : (
                          <Input
                            id={id}
                            type={field.type === "date" ? "date" : "text"}
                            aria-describedby={field.help_text || bindingLabel ? helpId : undefined}
                            value={fieldValues[field.id] ?? ""}
                            onChange={(event) => updateFieldValue(field.id, event.target.value)}
                            disabled={Boolean(sourceId)}
                            placeholder={field.type === "text" ? field.help_text || "Enter a value" : undefined}
                          />
                        )}
                        {(field.help_text || bindingLabel) && (
                          <p id={helpId} className="text-xs text-text-muted">
                            {[field.help_text, bindingLabel].filter(Boolean).join(" · ")}
                          </p>
                        )}
                      </div>
                    );
                  })}
                </fieldset>
              ) : null}
              <section aria-labelledby="template-content-preview-title" className="min-w-0">
                <h4 id="template-content-preview-title" className="mb-2 text-sm font-medium text-text">Content preview</h4>
                <div className="border-border rounded-input border bg-surface p-3">
                  <MarkdownPreview content={resolved?.content ?? selected.content} />
                </div>
              </section>
              {operation === "create" && resolved?.unresolvedFields.length ? (
                <div role="alert" className="rounded-input border border-warning/30 bg-warning/10 px-3 py-2 text-sm text-text">
                  {resolved.unresolvedFields.map((field) => `${field.label}: ${field.reason}`).join(" ")}
                </div>
              ) : null}
              {submitError && <p role="alert" className="text-sm text-danger">{submitError}</p>}
            </DialogBody>
            <DialogFooter className="flex-wrap px-4 py-3">
              <Button type="button" variant="secondary" onClick={() => { setSelectedId(null); setSubmitError(""); }} disabled={submitting || Boolean(sourceId)}>Back</Button>
              <Button
                type="button"
                onClick={() => void submit(selected)}
                disabled={submitting || (operation === "create" && Boolean(resolved?.unresolvedFields.length))}
              >
                {submitting
                  ? operation === "insert" ? "Inserting…" : "Creating…"
                  : operation === "insert" ? "Insert section" : "Use template"}
              </Button>
            </DialogFooter>
          </>
        )}
      </div>
    </DialogShell>
  );
}
