import { useEffect, useRef, useState } from "react";
import { ArrowLeft, Plus, Trash2 } from "lucide-react";
import { Editor, htmlToMarkdown, type TipTapEditor } from "@/components/editor/Editor";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import {
  addField,
  addGuidance,
  createBlankTemplate,
  createPageSnapshot,
  EXAMPLE_TEMPLATES,
  initialFieldValues,
  removeField,
  renameFieldBindings,
  validateFields,
  validateDefinition,
  type AuthoringField,
  type AuthoringTemplate,
  type FieldValues,
  type TemplatePageSnapshot,
  type WritingGuidance,
} from "./authoring";
import "./authoring.css";

type Workspace = "library" | "edit" | "test" | "use" | "page";

function cloneTemplate(template: AuthoringTemplate): AuthoringTemplate {
  return {
    ...template,
    fields: template.fields.map((item) => ({ ...item, options: [...item.options] })),
    guidance: template.guidance.map((item) => ({ ...item })),
  };
}

function useDesktopEditorToolbar() {
  const [desktop, setDesktop] = useState(
    () => typeof window !== "undefined" && window.matchMedia("(min-width: 1024px)").matches,
  );
  useEffect(() => {
    const query = window.matchMedia("(min-width: 1024px)");
    const update = () => setDesktop(query.matches);
    update();
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  return desktop;
}

function replaceBindingsWithText(editor: TipTapEditor, bindings: TemplatePageSnapshot["bindings"]) {
  if (!bindings.length) return;
  const replacements = new Map(bindings.map(({ marker, value }) => [marker, value]));
  const edits: Array<{ from: number; to: number; value: string }> = [];
  editor.state.doc.descendants((node, position) => {
    if (!node.isText || !node.text) return;
    const pattern = /\{\{[^}]+\}\}/g;
    for (const match of node.text.matchAll(pattern)) {
      const value = replacements.get(match[0]);
      if (value === undefined || match.index === undefined) continue;
      edits.push({
        from: position + match.index,
        to: position + match.index + match[0].length,
        value,
      });
    }
  });
  if (!edits.length) return;
  let transaction = editor.state.tr.setMeta("addToHistory", false);
  for (const edit of edits.reverse())
    transaction = transaction.insertText(edit.value, edit.from, edit.to);
  editor.view.dispatch(transaction);
}

function ContentEditor({
  content,
  onChange,
  onEditorReady,
}: {
  content: string;
  onChange: (value: string) => void;
  onEditorReady?: (editor: TipTapEditor) => void;
}) {
  const desktopToolbar = useDesktopEditorToolbar();
  const [toolbarHost, setToolbarHost] = useState<HTMLDivElement | null>(null);
  const changeRef = useRef(onChange);
  changeRef.current = onChange;
  const liveEditor = useRef<TipTapEditor | null>(null);
  const listener = useRef<(() => void) | null>(null);
  useEffect(
    () => () => {
      if (liveEditor.current && listener.current)
        liveEditor.current.off("update", listener.current);
    },
    [],
  );
  return (
    <div className="authoring-content-editor">
      {desktopToolbar ? <div ref={setToolbarHost} className="authoring-toolbar-host" /> : null}
      <Editor
        content={content}
        onChange={() => {}}
        desktopToolbarHost={desktopToolbar ? toolbarHost : null}
        showBlockChrome={desktopToolbar}
        showMobileCommandButton={!desktopToolbar}
        onEditorReady={(editor) => {
          liveEditor.current = editor;
          const capture = () => changeRef.current(htmlToMarkdown(editor.getHTML()));
          listener.current = capture;
          // The production owner suppresses callbacks during its initialization window.
          // This in-memory adapter also captures immediate fixture edits and snapshots.
          editor.on("update", capture);
          onEditorReady?.(editor);
        }}
      />
    </div>
  );
}

function FieldTestControl({
  field,
  value,
  onChange,
  label,
}: {
  field: AuthoringField;
  label?: string;
  value: string;
  onChange: (value: string) => void;
}) {
  if (field.kind === "multiline")
    return (
      <Textarea
        aria-label={label ?? field.label}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        rows={3}
      />
    );
  if (field.kind === "dropdown") {
    return (
      <Select
        aria-label={label ?? field.label}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="">Choose an option</option>
        {[...new Set(field.options.map((option) => option.trim()).filter(Boolean))].map(
          (option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ),
        )}
      </Select>
    );
  }
  return (
    <Input
      aria-label={label ?? field.label}
      type={field.kind === "date" ? "date" : "text"}
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  );
}

function PageWorkspace({
  page,
  onPageChange,
  onBack,
}: {
  page: TemplatePageSnapshot;
  onPageChange: (update: (current: TemplatePageSnapshot) => TemplatePageSnapshot) => void;
  onBack: () => void;
}) {
  const editorRef = useRef<TipTapEditor | null>(null);
  const capture = (editor: TipTapEditor) =>
    onPageChange((current) => ({
      ...current,
      content: htmlToMarkdown(editor.getHTML()),
      contentJson: editor.getJSON(),
      bindingsResolved: true,
    }));
  return (
    <div className="template-authoring" data-prototype="template-authoring-page">
      <div className="authoring-workspace-head">
        <Button variant="ghost" onClick={onBack}>
          <ArrowLeft size={16} /> Back to template
        </Button>
        <span className="authoring-prototype-note">Independent page copy · prototype</span>
      </div>
      <div className="authoring-created-page">
        <label>
          Page title
          <Input
            value={page.title}
            onChange={(event) =>
              onPageChange((current) => ({ ...current, title: event.target.value }))
            }
          />
        </label>
        {Object.keys(page.properties).length ? (
          <div className="authoring-page-properties">
            <h2>Properties</h2>
            <div>
              {Object.entries(page.properties).map(([label, value]) => (
                <span key={label}>
                  <b>{label}</b>
                  {value || "—"}
                </span>
              ))}
            </div>
          </div>
        ) : null}
        <div className="authoring-content-heading">
          <h2>Page content</h2>
          <p>Edit any block or add new content. Your template stays separate.</p>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => {
              const editor = editorRef.current;
              if (!editor) return;
              editor
                .chain()
                .focus("end")
                .insertContent([
                  {
                    type: "heading",
                    attrs: { level: 2 },
                    content: [{ type: "text", text: "New section" }],
                  },
                  { type: "paragraph" },
                ])
                .run();
              capture(editor);
            }}
          >
            <Plus size={15} /> Add section
          </Button>
        </div>
        <ContentEditor
          content={page.content}
          onChange={() => {
            if (editorRef.current) capture(editorRef.current);
          }}
          onEditorReady={(editor) => {
            editorRef.current = editor;
            if (page.contentJson)
              editor.commands.setContent(page.contentJson, { emitUpdate: false });
            if (!page.bindingsResolved) replaceBindingsWithText(editor, page.bindings);
            capture(editor);
          }}
        />
        <p className="authoring-page-note">
          Changes stay in this prototype until reload. Changing the template does not change this
          page.
        </p>
      </div>
    </div>
  );
}

function BoundPreview({ template, values }: { template: AuthoringTemplate; values: FieldValues }) {
  const [content, setContent] = useState(template.content);
  return (
    <div className="authoring-rich-preview">
      <Editor
        content={content}
        readOnly
        autoFocus={false}
        showBlockChrome={false}
        onChange={() => {}}
        onEditorReady={(editor) => {
          if (template.contentJson)
            editor.commands.setContent(template.contentJson, { emitUpdate: false });
          replaceBindingsWithText(
            editor,
            createPageSnapshot(template, values, "", "preview").bindings,
          );
          setContent(htmlToMarkdown(editor.getHTML()));
        }}
      />
    </div>
  );
}

/** In-memory authoring prototype; it does not write to the Notes template or document stores. */
export function TemplateAuthoringPrototype() {
  const [workspace, setWorkspace] = useState<Workspace>("library");
  const [template, setTemplate] = useState<AuthoringTemplate>(createBlankTemplate("initial-draft"));
  const [savedTemplates, setSavedTemplates] = useState<AuthoringTemplate[]>([]);
  const [testValues, setTestValues] = useState<FieldValues>({});
  const [testTitle, setTestTitle] = useState("");
  const [page, setPage] = useState<TemplatePageSnapshot | null>(null);
  const [notice, setNotice] = useState("");
  const [fieldErrors, setFieldErrors] = useState<string[]>([]);
  const [bindingField, setBindingField] = useState("");
  const idSequence = useRef(0);
  const [addedFieldId, setAddedFieldId] = useState<string | null>(null);
  const templateEditorRef = useRef<TipTapEditor | null>(null);

  const nextId = (kind: string) => {
    const next = ++idSequence.current;
    return `${kind}-${next}`;
  };

  const openTemplate = (source: AuthoringTemplate) => {
    const draft = cloneTemplate(source);
    setTemplate(draft);
    setTestValues(initialFieldValues(draft));
    setTestTitle(draft.name === "Untitled template" ? "" : draft.name);
    setFieldErrors([]);
    setNotice("");
    setWorkspace("edit");
  };

  const updateField = (id: string, changes: Partial<AuthoringField>) => {
    setTemplate((current) =>
      changes.label !== undefined
        ? renameFieldBindings(current, id, changes.label)
        : {
            ...current,
            fields: current.fields.map((item) => {
              if (item.id !== id) return item;
              const next = { ...item, ...changes };
              if (changes.kind !== undefined && changes.kind !== item.kind) next.defaultValue = "";
              if (
                changes.options &&
                !changes.options.map((option) => option.trim()).includes(next.defaultValue)
              )
                next.defaultValue = "";
              return next;
            }),
          },
    );
  };

  const updateGuidance = (id: string, changes: Partial<WritingGuidance>) => {
    setTemplate((current) => ({
      ...current,
      guidance: current.guidance.map((item) => (item.id === id ? { ...item, ...changes } : item)),
    }));
  };

  const beginTest = () => {
    const errors = validateDefinition(template);
    if (errors.length) {
      setNotice(errors.join(". "));
      return;
    }
    setFieldErrors([]);
    setNotice("");
    setTestValues(initialFieldValues(template));
    setTestTitle(template.name === "Untitled template" ? "Untitled page" : template.name);
    setWorkspace("test");
  };

  const useTemplate = () => {
    const errors = validateFields(template, testValues);
    setFieldErrors(errors);
    if (errors.length) {
      setNotice(`Check these fields: ${errors.join(", ")}.`);
      return;
    }
    const nextPage = createPageSnapshot(template, testValues, testTitle, nextId("page"));
    setPage(nextPage);
    setNotice("");
    setWorkspace("page");
  };

  const saveTemplate = () => {
    if (!template.name.trim() || template.name === "Untitled template") {
      setNotice("Give this template a name before saving it to the prototype library.");
      return;
    }
    const errors = validateDefinition(template);
    if (errors.length) {
      setNotice(errors.join(". "));
      return;
    }
    setSavedTemplates((current) => [
      ...current.filter((item) => item.id !== template.id),
      cloneTemplate(template),
    ]);
    setNotice("Saved in this prototype library. This change is held in memory only.");
  };

  const appendBinding = () => {
    const item = template.fields.find(({ id }) => id === bindingField);
    if (!item) return;
    const editor = templateEditorRef.current;
    if (!editor) return;
    editor
      .chain()
      .focus()
      .insertContent({ type: "text", text: `{{${item.label.trim()}}}` })
      .run();
    setTemplate((current) => ({
      ...current,
      content: htmlToMarkdown(editor.getHTML()),
      contentJson: editor.getJSON(),
    }));
    setBindingField("");
  };

  if (workspace === "library") {
    const exampleIds = new Set(EXAMPLE_TEMPLATES.map(({ id }) => id));
    const library = [
      ...EXAMPLE_TEMPLATES.map((item) => savedTemplates.find(({ id }) => id === item.id) ?? item),
      ...savedTemplates.filter(({ id }) => !exampleIds.has(id)),
    ];
    return (
      <div className="template-authoring" data-prototype="template-authoring">
        <div className="authoring-library-head">
          <div>
            <h1>Build a template</h1>
            <p>Start with an empty page or adapt a working example.</p>
          </div>
          <Button onClick={() => openTemplate(createBlankTemplate(nextId("template")))}>
            <Plus size={16} /> Create blank template
          </Button>
        </div>
        <p className="authoring-prototype-note" role="note">
          Prototype workspace · templates and pages stay in memory while this view is open.
        </p>
        <h2>Your template library</h2>
        <div className="authoring-library-list">
          {library.map((item) => (
            <article className="authoring-library-item" key={item.id}>
              <div className="authoring-library-icon" aria-hidden="true">
                {item.name.slice(0, 1)}
              </div>
              <div className="authoring-library-copy">
                <h3>{item.name}</h3>
                <p>
                  {item.description || "A reusable starting point you can shape for your own work."}
                </p>
                <span>
                  {item.fields.length} fields · {item.guidance.length} writing prompts
                </span>
              </div>
              <div className="authoring-library-actions">
                <Button
                  variant="secondary"
                  aria-label={`Edit ${item.name} template`}
                  onClick={() => openTemplate(item)}
                >
                  Edit template
                </Button>
                <Button
                  aria-label={`Use ${item.name} template`}
                  onClick={() => {
                    openTemplate(item);
                    setWorkspace("use");
                  }}
                >
                  Use template
                </Button>
              </div>
            </article>
          ))}
        </div>
      </div>
    );
  }

  if (workspace === "edit") {
    return (
      <div className="template-authoring" data-prototype="template-authoring">
        <div className="authoring-workspace-head">
          <Button
            variant="ghost"
            onClick={() => {
              setNotice("");
              setWorkspace("library");
            }}
          >
            <ArrowLeft size={16} /> Library
          </Button>
          <div className="authoring-workspace-actions">
            {page ? (
              <Button variant="secondary" onClick={() => setWorkspace("page")}>
                Open existing page
              </Button>
            ) : null}
            <Button variant="secondary" onClick={saveTemplate}>
              Save template
            </Button>
            <Button onClick={beginTest}>Test template</Button>
          </div>
        </div>
        <div className="authoring-editor-intro">
          <h1>Edit template</h1>
          <p>Build the page in the editor. Add prompts and reusable details in the inspector.</p>
        </div>
        {notice ? (
          <p className="authoring-notice" role="status">
            {notice}
          </p>
        ) : null}
        <div className="authoring-template-meta">
          <label>
            Template name
            <Input
              value={template.name}
              onChange={(event) =>
                setTemplate((current) => ({ ...current, name: event.target.value }))
              }
            />
          </label>
          <label>
            Description
            <Textarea
              rows={2}
              value={template.description}
              onChange={(event) =>
                setTemplate((current) => ({ ...current, description: event.target.value }))
              }
            />
          </label>
        </div>
        <div className="authoring-editor-layout">
          <section className="authoring-editor-column">
            <div className="authoring-content-heading">
              <h2>Page content</h2>
              <p>Use the editor to write, format, and rearrange blocks.</p>
              <div className="authoring-content-actions">
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => {
                    const editor = templateEditorRef.current;
                    if (!editor) return;
                    editor
                      .chain()
                      .focus("end")
                      .insertContent([
                        {
                          type: "heading",
                          attrs: { level: 2 },
                          content: [{ type: "text", text: "New section" }],
                        },
                        { type: "paragraph" },
                      ])
                      .run();
                    setTemplate((current) => ({
                      ...current,
                      content: htmlToMarkdown(editor.getHTML()),
                      contentJson: editor.getJSON(),
                    }));
                  }}
                >
                  <Plus size={15} /> Add section
                </Button>
                {template.fields.length ? (
                  <>
                    <Select
                      aria-label="Field to insert"
                      value={bindingField}
                      onChange={(event) => setBindingField(event.target.value)}
                    >
                      <option value="">Insert a field answer…</option>
                      {template.fields.map((item) => (
                        <option key={item.id} value={item.id}>
                          {item.label || "Untitled field"}
                        </option>
                      ))}
                    </Select>
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={!bindingField}
                      onClick={appendBinding}
                    >
                      Insert field
                    </Button>
                  </>
                ) : null}
              </div>
            </div>
            <ContentEditor
              content={template.content}
              onChange={(content) =>
                setTemplate((current) => ({
                  ...current,
                  content,
                  contentJson: templateEditorRef.current?.getJSON(),
                }))
              }
              onEditorReady={(editor) => {
                templateEditorRef.current = editor;
                if (template.contentJson)
                  editor.commands.setContent(template.contentJson, { emitUpdate: false });
              }}
            />
          </section>
          <aside className="authoring-inspector">
            <section className="authoring-inspector-section">
              <div className="authoring-section-title">
                <div>
                  <h2>Fields</h2>
                  <p>Details asked before creating a page.</p>
                </div>
                <Button
                  variant="secondary"
                  size="sm"
                  aria-label="Add field"
                  onClick={() => {
                    const id = nextId("field");
                    setAddedFieldId(id);
                    setTemplate((current) => addField(current, id));
                  }}
                >
                  <Plus size={15} /> Add
                </Button>
              </div>
              {template.fields.length === 0 ? (
                <p className="authoring-inspector-empty">
                  No fields. This template can start with a blank page.
                </p>
              ) : null}
              {template.fields.map((item) => (
                <article className="authoring-inspector-item" key={item.id}>
                  <div className="authoring-config-head">
                    <strong>{item.label || "Untitled field"}</strong>
                    <Button
                      variant="ghost"
                      size="sm"
                      aria-label={`Delete ${item.label || "field"}`}
                      onClick={() => setTemplate((current) => removeField(current, item.id))}
                    >
                      <Trash2 size={15} />
                    </Button>
                  </div>
                  <label>
                    Label
                    <Input
                      value={item.label}
                      autoFocus={item.id === addedFieldId}
                      onChange={(event) => updateField(item.id, { label: event.target.value })}
                    />
                  </label>
                  <label>
                    Response type
                    <Select
                      aria-label={`Response type for ${item.label}`}
                      value={item.kind}
                      onChange={(event) =>
                        updateField(item.id, { kind: event.target.value as AuthoringField["kind"] })
                      }
                    >
                      <option value="text">Short text</option>
                      <option value="multiline">Long text</option>
                      <option value="date">Date</option>
                      <option value="dropdown">Dropdown</option>
                    </Select>
                  </label>
                  {item.kind === "dropdown" ? (
                    <label>
                      Options
                      <Textarea
                        rows={3}
                        value={item.options.join("\n")}
                        onChange={(event) =>
                          updateField(item.id, { options: event.target.value.split("\n") })
                        }
                        placeholder="One option per line"
                      />
                    </label>
                  ) : null}
                  <label>
                    Suggested answer
                    <FieldTestControl
                      field={item}
                      label={`${item.label} suggested answer`}
                      value={item.defaultValue}
                      onChange={(value) => updateField(item.id, { defaultValue: value })}
                    />
                  </label>
                  <label className="authoring-check">
                    <input
                      type="checkbox"
                      checked={item.required}
                      onChange={(event) => updateField(item.id, { required: event.target.checked })}
                    />{" "}
                    Required
                  </label>
                </article>
              ))}
            </section>
            <section className="authoring-inspector-section">
              <div className="authoring-section-title">
                <div>
                  <h2>Writing guidance</h2>
                  <p>Optional prompts shown with this template.</p>
                </div>
                <Button
                  variant="secondary"
                  size="sm"
                  aria-label="Add writing guidance"
                  onClick={() => setTemplate((current) => addGuidance(current, nextId("guidance")))}
                >
                  <Plus size={15} /> Add
                </Button>
              </div>
              {template.guidance.map((item) => (
                <article className="authoring-inspector-item" key={item.id}>
                  <div className="authoring-config-head">
                    <strong>{item.section || "Prompt"}</strong>
                    <Button
                      variant="ghost"
                      size="sm"
                      aria-label={`Delete ${item.section || "guidance"}`}
                      onClick={() =>
                        setTemplate((current) => ({
                          ...current,
                          guidance: current.guidance.filter(({ id }) => id !== item.id),
                        }))
                      }
                    >
                      <Trash2 size={15} />
                    </Button>
                  </div>
                  <label>
                    Section name
                    <Input
                      value={item.section}
                      onChange={(event) => updateGuidance(item.id, { section: event.target.value })}
                    />
                  </label>
                  <label>
                    Guidance
                    <Textarea
                      rows={2}
                      value={item.prompt}
                      onChange={(event) => updateGuidance(item.id, { prompt: event.target.value })}
                      placeholder="What should someone write?"
                    />
                  </label>
                </article>
              ))}
            </section>
          </aside>
        </div>
      </div>
    );
  }

  if (workspace === "test" || workspace === "use") {
    const invalid = validateFields(template, testValues);
    return (
      <div className="template-authoring" data-prototype="template-authoring">
        <div className="authoring-workspace-head">
          <Button
            variant="ghost"
            onClick={() => {
              setFieldErrors([]);
              setNotice("");
              setWorkspace("edit");
            }}
          >
            <ArrowLeft size={16} /> Edit template
          </Button>
          <Button variant="secondary" onClick={saveTemplate}>
            Save template
          </Button>
        </div>
        <div className="authoring-editor-intro">
          <h1>
            {workspace === "use" ? "Use" : "Test"} {template.name || "template"}
          </h1>
          <p>
            {workspace === "use"
              ? "Fill the fields, review the result, then create your own editable page."
              : "Try the fields and preview the result. Testing creates no page."}
          </p>
        </div>
        {notice ? (
          <p className="authoring-notice" role="alert">
            {notice}
          </p>
        ) : null}
        <div className="authoring-test-layout">
          <section className="authoring-test-form">
            <h2>Try the fields</h2>
            <label>
              Page title
              <Input value={testTitle} onChange={(event) => setTestTitle(event.target.value)} />
            </label>
            {template.fields.map((item) => (
              <label key={item.id}>
                {item.label || "Untitled field"}
                {item.required ? <span className="authoring-required"> · required</span> : null}
                <FieldTestControl
                  field={item}
                  value={testValues[item.id] ?? ""}
                  onChange={(value) =>
                    setTestValues((current) => ({ ...current, [item.id]: value }))
                  }
                />
                {fieldErrors.includes(item.label) ? (
                  <span className="authoring-error">Enter a valid answer for this field.</span>
                ) : null}
              </label>
            ))}
            <Button onClick={workspace === "test" ? () => setWorkspace("use") : useTemplate}>
              {workspace === "test" ? "Use this template" : "Create editable page"}
            </Button>
          </section>
          <section className="authoring-test-preview">
            <h2>Page preview</h2>
            <article className="authoring-preview-content">
              <h2>{testTitle || template.name || "Untitled page"}</h2>
              <BoundPreview
                key={JSON.stringify([template.content, testValues])}
                template={template}
                values={testValues}
              />
            </article>
            {template.guidance.length ? (
              <div className="authoring-preview-guidance">
                <h3>Writing guidance</h3>
                {template.guidance.map((item) => (
                  <p key={item.id}>
                    <b>{item.section}</b>
                    {item.prompt ? ` — ${item.prompt}` : ""}
                  </p>
                ))}
              </div>
            ) : null}
            {invalid.length ? (
              <p className="authoring-muted">
                {invalid.length} {invalid.length === 1 ? "field needs" : "fields need"} attention.
              </p>
            ) : null}
          </section>
        </div>
      </div>
    );
  }

  return page ? (
    <PageWorkspace
      key={page.id}
      page={page}
      onPageChange={(update) => setPage((current) => (current ? update(current) : current))}
      onBack={() => setWorkspace("edit")}
    />
  ) : null;
}
