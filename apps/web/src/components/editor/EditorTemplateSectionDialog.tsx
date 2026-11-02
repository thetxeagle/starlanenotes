import { useState } from "react";
import type { PropertySchemaRecord, TemplateRecord } from "@/lib/dataClient";
import {
  getTemplateFieldOptions,
  type TemplateFieldValues,
  type TemplateRenderContext,
} from "@/lib/templateExperience";
import { resolveTemplateSection } from "@/lib/templateSection";
import { MarkdownPreview } from "@/components/editor/MarkdownPreview";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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

export function EditorTemplateSectionDialog({
  template,
  renderContext = {},
  currentPageLabel,
  schemas = [],
  onInsert,
  onClose,
}: {
  template: TemplateRecord;
  renderContext?: TemplateRenderContext;
  currentPageLabel: string;
  schemas?: PropertySchemaRecord[];
  onInsert: (markdown: string) => boolean | void;
  onClose: () => void;
}) {
  const [fieldValues, setFieldValues] = useState<TemplateFieldValues>(() =>
    Object.fromEntries(
      resolveTemplateSection({ template, schemas }).fields.map((field) => [field.id, field.default_value ?? ""]),
    ),
  );
  const [error, setError] = useState("");
  const [inserting, setInserting] = useState(false);
  const resolved = resolveTemplateSection({ template, context: renderContext, fieldValues, schemas });

  const insert = () => {
    if (inserting || resolved.unresolvedFields.length) return;
    setInserting(true);
    setError("");
    try {
      const accepted = onInsert(resolved.content);
      if (accepted === false) {
        setError("The section could not be inserted. Your answers are still here; try again.");
        setInserting(false);
        return;
      }
      onClose();
    } catch {
      setError("The section could not be inserted. Your answers are still here; try again.");
      setInserting(false);
    }
  };

  return (
    <DialogShell
      labelledBy="template-section-dialog-title"
      describedBy="template-section-dialog-description"
      maxWidth="xl"
      onClose={onClose}
    >
      <div className="flex max-h-[calc(100dvh-2rem)] min-h-0 flex-col">
        <DialogHeader>
          <div className="min-w-0">
            <DialogTitle id="template-section-dialog-title">Insert section</DialogTitle>
            <DialogDescription id="template-section-dialog-description">
              {template.name} will be added to {currentPageLabel}.
            </DialogDescription>
          </div>
          <DialogCloseAction label="Close reusable section" disabled={inserting} onClick={onClose} />
        </DialogHeader>
        <DialogBody className="min-h-0 flex-1 space-y-4 overflow-y-auto">
          {resolved.fields.length > 0 && (
            <fieldset className="space-y-3">
              <legend className="text-sm font-medium text-text">Section details</legend>
              {resolved.fields.map((field) => {
                const id = `section-template-field-${field.id}`;
                return (
                  <div key={field.id} className="space-y-1.5">
                    <label htmlFor={id} className="text-sm font-medium text-text">
                      {field.label}{field.required ? <span className="text-danger"> *</span> : null}
                    </label>
                    {field.type === "multiline" ? (
                      <Textarea
                        id={id}
                        value={fieldValues[field.id] ?? ""}
                        onChange={(event) => setFieldValues((current) => ({ ...current, [field.id]: event.target.value }))}
                        disabled={inserting}
                        placeholder={field.help_text || "Enter a value"}
                        aria-describedby={field.help_text ? `${id}-help` : undefined}
                      />
                    ) : field.type === "dropdown" ? (
                      <Select
                        id={id}
                        value={fieldValues[field.id] ?? ""}
                        onChange={(event) => setFieldValues((current) => ({ ...current, [field.id]: event.target.value }))}
                        disabled={inserting}
                        aria-describedby={field.help_text ? `${id}-help` : undefined}
                      >
                        <option value="">Choose an option</option>
                        {getTemplateFieldOptions(field, []).map((option) => <option key={option} value={option}>{option}</option>)}
                      </Select>
                    ) : (
                      <Input
                        id={id}
                        type={field.type === "date" ? "date" : "text"}
                        value={fieldValues[field.id] ?? ""}
                        onChange={(event) => setFieldValues((current) => ({ ...current, [field.id]: event.target.value }))}
                        disabled={inserting}
                        placeholder={field.help_text || undefined}
                        aria-describedby={field.help_text ? `${id}-help` : undefined}
                      />
                    )}
                    {field.help_text && <p id={`${id}-help`} className="text-xs text-text-muted">{field.help_text}</p>}
                  </div>
                );
              })}
            </fieldset>
          )}
          <section aria-labelledby="template-section-preview-title">
            <h3 id="template-section-preview-title" className="mb-2 text-sm font-medium text-text">Insertion preview</h3>
            <div className="border-border rounded-input border bg-surface p-3">
              <MarkdownPreview content={resolved.content} />
            </div>
          </section>
          {resolved.unresolvedFields.length > 0 && (
            <p role="alert" className="rounded-input border border-warning/30 bg-warning/10 px-3 py-2 text-sm text-text">
              {resolved.unresolvedFields.map((field) => `${field.label}: ${field.reason}`).join(" ")}
            </p>
          )}
          {error && <p role="alert" className="text-sm text-danger">{error}</p>}
        </DialogBody>
        <DialogFooter>
          <Button type="button" variant="secondary" onClick={onClose} disabled={inserting}>Cancel</Button>
          <Button type="button" onClick={insert} disabled={inserting || resolved.unresolvedFields.length > 0}>
            {inserting ? "Inserting…" : "Insert section"}
          </Button>
        </DialogFooter>
      </div>
    </DialogShell>
  );
}
