import { useEffect, useMemo, useState } from "react";
import {
  ArrowLeft,
  Check,
  ChevronRight,
  ClipboardCheck,
  FilePlus2,
  FileText,
  FlaskConical,
  Info,
  Library,
  Moon,
  Search,
  Sun,
  Undo2,
  Redo2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import {
  DialogShell,
  DialogBody,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { MarkdownPreview } from "@/components/editor/MarkdownPreview";
import {
  buildIncidentOutput,
  EMPTY_RESPONSES,
  formatIncidentTitle,
  guidanceForRevision,
  INCIDENT_FIXTURES,
  INCIDENT_SECTIONS,
  incidentProperties,
  type IncidentFieldValues,
  type IncidentResponses,
  type IncidentSection,
  type TemplateRevision,
} from "./fixture";

import { TemplateAuthoringPrototype } from "./TemplateAuthoringPrototype";

type Flow = "author" | "create" | "save" | "test";
type CreatedFixturePage = {
  title: string;
  destination: string;
  fields: IncidentFieldValues;
  responses: IncidentResponses;
  revision: string;
};
const INITIAL_FIELDS: IncidentFieldValues = {
  service: "Payments API",
  severity: "SEV-2",
  date: "2026-10-07",
  owner: "Maya Chen",
};

export function FixtureOutput({
  fields,
  responses,
  title,
}: {
  fields: IncidentFieldValues;
  responses: IncidentResponses;
  title?: string;
}) {
  const output = buildIncidentOutput(fields, responses);
  return (
    <article className="template-plain-output" aria-label="Generated fixture document output">
      <h3>{title ?? output.title}</h3>
      <div className="template-plain-properties">
        {Object.entries(output.properties).map(([key, value]) => (
          <span key={key}>
            {key}
            <b>{value || "—"}</b>
          </span>
        ))}
      </div>
      {output.sections.map((section) => (
        <section key={section.id}>
          <h4>{section.title}</h4>
          {section.id === "summary" ? (
            <p>
              Service: {output.summaryFields.service || "—"}
              <br />
              Severity: {output.summaryFields.severity || "—"}
              <br />
              Incident date: {output.summaryFields.date || "—"}
              <br />
              Owner: {output.summaryFields.owner || "—"}
            </p>
          ) : null}
          {section.text ? (
            <p className="template-plain-response">{section.text}</p>
          ) : (
            <p className="template-plain-empty">No response yet.</p>
          )}
        </section>
      ))}
    </article>
  );
}

function FieldInput({
  id,
  label,
  value,
  onChange,
  error,
  type = "text",
}: {
  id: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
  error?: string;
  type?: string;
}) {
  return (
    <div className="template-field">
      <label htmlFor={id}>
        {label} <span aria-hidden="true">*</span>
      </label>
      <Input
        id={id}
        value={value}
        type={type}
        aria-invalid={Boolean(error)}
        onChange={(event) => onChange(event.target.value)}
      />
      {error ? <span className="template-error">{error}</span> : null}
    </div>
  );
}

export function TemplateExperiencePrototype() {
  const [flow, setFlow] = useState<Flow>("author");
  const [theme, setTheme] = useState<"light" | "dark">("light");
  const [query, setQuery] = useState("");
  const [blankContent, setBlankContent] = useState("");
  const [failNextCreation, setFailNextCreation] = useState(false);
  useEffect(() => {
    document.documentElement.classList.remove("dark");
    document.documentElement.dataset.theme = "light";
  }, []);
  const [pickerState, setPickerState] = useState<"ready" | "loading" | "failure">("ready");
  const [selectedId, setSelectedId] = useState<string>(INCIDENT_FIXTURES[0].id);
  const [destination, setDestination] = useState("Engineering / Incidents");
  const [title, setTitle] = useState(formatIncidentTitle(INITIAL_FIELDS));
  const [titleEdited, setTitleEdited] = useState(false);
  const [fields, setFields] = useState<IncidentFieldValues>(INITIAL_FIELDS);
  const [fieldErrors, setFieldErrors] = useState<Partial<IncidentFieldValues>>({});
  const [responses, setResponses] = useState<IncidentResponses>(EMPTY_RESPONSES);
  const [responseUndo, setResponseUndo] = useState<IncidentResponses[]>([]);
  const [responseRedo, setResponseRedo] = useState<IncidentResponses[]>([]);
  const [createdPage, setCreatedPage] = useState<CreatedFixturePage | null>(null);
  const [showCreatedPage, setShowCreatedPage] = useState(false);
  const [pageSaved, setPageSaved] = useState(false);
  const [saveDialog, setSaveDialog] = useState(false);
  const [saveName, setSaveName] = useState("Incident Report — service team");
  const [saveDescription, setSaveDescription] = useState(
    "A reusable incident record for service teams.",
  );
  const [includeTags, setIncludeTags] = useState(true);
  const [includeProperties, setIncludeProperties] = useState(true);
  const [includePresentation, setIncludePresentation] = useState(true);
  const [fixtureSaved, setFixtureSaved] = useState(false);
  const [sample, setSample] = useState<IncidentFieldValues>({
    ...INITIAL_FIELDS,
    service: "Identity",
  });
  const [sampleResponses, setSampleResponses] = useState<IncidentResponses>({
    ...EMPTY_RESPONSES,
    impact: "A small number of sign-ins took longer than usual.",
  });
  const sampleErrors = (Object.keys(sample) as (keyof IncidentFieldValues)[]).filter(
    (key) => !sample[key].trim(),
  );
  const [revision, setRevision] = useState<TemplateRevision>("published");
  const [notice, setNotice] = useState("");

  const selected = INCIDENT_FIXTURES.find((item) => item.id === selectedId);
  const filtered = useMemo(
    () =>
      INCIDENT_FIXTURES.filter((item) =>
        `${item.name} ${item.description}`.toLowerCase().includes(query.toLowerCase()),
      ),
    [query],
  );
  const setField = (key: keyof IncidentFieldValues, value: string) => {
    setFields((current) => {
      const next = { ...current, [key]: value };
      if (!titleEdited && (key === "service" || key === "date"))
        setTitle(formatIncidentTitle(next));
      return next;
    });
    setFieldErrors((current) => ({ ...current, [key]: undefined }));
  };
  const setResponse = (key: IncidentSection, value: string) => {
    setResponseUndo((history) => [...history, responses]);
    setResponseRedo([]);
    const next = { ...responses, [key]: value };
    setResponses(next);
    setPageSaved(false);
    if (createdPage) setCreatedPage((page) => (page ? { ...page, responses: next } : null));
  };
  const undoResponse = () => {
    const previous = responseUndo[responseUndo.length - 1];
    if (!previous) return;
    setResponseRedo((history) => [...history, responses]);
    setResponses(previous);
    setPageSaved(false);
    setResponseUndo((history) => history.slice(0, -1));
    if (createdPage) setCreatedPage((page) => (page ? { ...page, responses: previous } : null));
  };
  const redoResponse = () => {
    const next = responseRedo[responseRedo.length - 1];
    if (!next) return;
    setResponseUndo((history) => [...history, responses]);
    setResponses(next);
    setPageSaved(false);
    setResponseRedo((history) => history.slice(0, -1));
    if (createdPage) setCreatedPage((page) => (page ? { ...page, responses: next } : null));
  };
  const setMode = (next: "light" | "dark") => {
    setTheme(next);
    document.documentElement.dataset.theme = next;
    document.documentElement.classList.toggle("dark", next === "dark");
  };
  const useTemplate = () => {
    const errors: Partial<IncidentFieldValues> = {};
    (Object.keys(fields) as (keyof IncidentFieldValues)[]).forEach((key) => {
      if (!fields[key].trim()) errors[key] = "This field is required.";
    });
    setFieldErrors(errors);
    if (Object.keys(errors).length) {
      setNotice("Complete the required fields before creating the fixture page.");
      return;
    }
    if (failNextCreation) {
      setFailNextCreation(false);
      setNotice(
        "Simulated creation failure. No fixture page was created. Your title, destination and fields are retained; retry Use template.",
      );
      return;
    }
    const nextTitle = titleEdited ? title : formatIncidentTitle(fields);
    setTitle(nextTitle);
    setResponses(EMPTY_RESPONSES);
    setResponseUndo([]);
    setResponseRedo([]);
    setCreatedPage({
      title: nextTitle,
      destination,
      fields: { ...fields },
      responses: EMPTY_RESPONSES,
      revision: selected?.revision ?? "Published revision 4",
    });
    setShowCreatedPage(true);
    setPageSaved(false);
    setNotice("Fixture page created in the selected destination. No real page was written.");
  };
  const createBlank = () => {
    setSelectedId("blank");
    setCreatedPage({
      title: "Untitled",
      destination,
      fields: { service: "", severity: "", date: "", owner: "" },
      responses: EMPTY_RESPONSES,
      revision: "Blank page",
    });
    setShowCreatedPage(true);
    setTitle("Untitled");
    setTitleEdited(false);
    setBlankContent("");
    setPageSaved(false);
    setNotice("Blank fixture page opened. No real page was written.");
  };
  const simulatePickerLoad = () => {
    setPickerState("loading");
    window.setTimeout(
      () => setPickerState((state) => (state === "loading" ? "ready" : state)),
      700,
    );
  };
  const saveFixtureTemplate = () => {
    setFixtureSaved(true);
    setSaveDialog(false);
    setNotice("Fixture template saved from Published revision 7; this is a local simulation.");
  };
  const setSampleResponse = (key: IncidentSection, value: string) =>
    setSampleResponses((current) => ({ ...current, [key]: value }));
  const sampleDefaults =
    revision === "published" ? "incident · operations" : "incident · operations · postmortem";
  const currentPage = createdPage && showCreatedPage ? { ...createdPage, title } : null;
  const properties =
    currentPage && currentPage.revision !== "Blank page"
      ? incidentProperties(currentPage.fields)
      : {};

  return (
    <main className="template-prototype">
      <div className="template-review-banner">
        <div>
          <strong>Prototype · fixture only</strong>
          <span>
            No real pages or templates are written. Save, revision, and publish states are
            simulated.
          </span>
        </div>
        <div className="template-theme-toggle" aria-label="Prototype theme">
          <Button
            variant={theme === "light" ? "secondary" : "ghost"}
            aria-label="Light theme"
            onClick={() => setMode("light")}
          >
            <Sun size={16} />
          </Button>
          <Button
            variant={theme === "dark" ? "secondary" : "ghost"}
            aria-label="Dark theme"
            onClick={() => setMode("dark")}
          >
            <Moon size={16} />
          </Button>
        </div>
      </div>
      <header className="template-topbar">
        <div className="template-brand">
          <span className="template-brand-mark">S</span>
          <span>Starlane Notes</span>
          <ChevronRight size={15} />
          <span>Template experience</span>
        </div>
        <div className="template-space-label">
          {flow === "author" ? "Example space" : "Engineering space"} <span>Fixture workspace</span>
        </div>
      </header>
      <nav className="template-flow-tabs" aria-label="Template prototype flows">
        <Button
          variant={flow === "author" ? "secondary" : "ghost"}
          aria-pressed={flow === "author"}
          onClick={() => setFlow("author")}
        >
          <Library size={16} /> Build templates
        </Button>
        <Button
          variant={flow === "create" ? "secondary" : "ghost"}
          onClick={() => setFlow("create")}
        >
          <FilePlus2 size={16} /> Incident example
        </Button>
        <Button variant={flow === "save" ? "secondary" : "ghost"} onClick={() => setFlow("save")}>
          <ClipboardCheck size={16} /> Save incident as template
        </Button>
        <Button variant={flow === "test" ? "secondary" : "ghost"} onClick={() => setFlow("test")}>
          <FlaskConical size={16} /> Test incident
        </Button>
      </nav>
      <div hidden={flow !== "author"}>
        <TemplateAuthoringPrototype />
      </div>
      {flow === "create" ? (
        <div className="template-workspace">
          <aside className="template-picker-panel">
            <div className="template-panel-heading">
              <div>
                <p className="template-context-label">Create a document</p>
                <h1>Choose a starting point</h1>
              </div>
            </div>
            <label className="template-search-label" htmlFor="template-search">
              Find a template
            </label>
            <div className="template-search">
              <Search size={17} />
              <Input
                id="template-search"
                value={query}
                placeholder="Search templates"
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
            <Button className="template-blank-button" variant="secondary" onClick={createBlank}>
              <FileText size={17} />
              <span>
                <strong>Blank page</strong>
                <small>Start with an empty document</small>
              </span>
              <ChevronRight size={16} />
            </Button>
            <div className="template-list-label">
              SPACE TEMPLATES <span>{filtered.length}</span>
            </div>
            <div className="template-demo-states">
              <span>Picker states</span>
              <Button variant="ghost" size="sm" onClick={simulatePickerLoad}>
                Loading
              </Button>
              <Button variant="ghost" size="sm" onClick={() => setPickerState("failure")}>
                Failure
              </Button>
              {pickerState === "failure" ? (
                <Button variant="ghost" size="sm" onClick={() => setPickerState("ready")}>
                  Retry
                </Button>
              ) : null}
            </div>
            {pickerState === "loading" ? (
              <div className="template-empty" role="status">
                Loading space templates…
              </div>
            ) : pickerState === "failure" ? (
              <div className="template-empty" role="alert">
                <strong>Templates could not be loaded</strong>
                <span>Fixture network failure state.</span>
                <Button variant="secondary" onClick={() => setPickerState("ready")}>
                  Retry
                </Button>
              </div>
            ) : filtered.length ? (
              filtered.map((item) => (
                <button
                  className={`template-result ${selectedId === item.id ? "is-selected" : ""}`}
                  key={item.id}
                  type="button"
                  onClick={() => {
                    setSelectedId(item.id);
                    setShowCreatedPage(false);
                  }}
                >
                  <FileText size={17} />
                  <span>
                    <strong>{item.name}</strong>
                    <small>{item.description}</small>
                  </span>
                  <ChevronRight size={16} />
                </button>
              ))
            ) : (
              <div className="template-empty">
                <Search size={18} />
                <strong>No templates found</strong>
                <span>Try another name or create a blank page.</span>
              </div>
            )}
            <div className="template-destination">
              <label htmlFor="template-destination">Destination</label>
              <Select
                id="template-destination"
                value={destination}
                onChange={(event) => setDestination(event.target.value)}
              >
                <option value="Engineering / Incidents">Engineering / Incidents</option>
                <option value="Engineering / Operations">Engineering / Operations</option>
                <option value="Engineering / Root">Engineering / Space root</option>
                <option value="Engineering / Incidents / Parent page">
                  Engineering / Incidents / Parent page
                </option>
              </Select>
              <span>Engineering space · folder path</span>
            </div>
          </aside>
          <section className="template-detail-panel" aria-label="Template preview and fields">
            {currentPage ? (
              <div className="template-page-view">
                <div className="template-page-toolbar">
                  <Button variant="ghost" onClick={() => setShowCreatedPage(false)}>
                    <ArrowLeft size={16} /> Back to template
                  </Button>
                  <span className="template-fixture-chip">
                    Fixture page · {currentPage.revision}
                  </span>
                </div>
                <div className="template-page-meta">
                  <span>{currentPage.destination}</span>
                  <span>·</span>
                  <span>{selected?.name ?? "Blank page"}</span>
                </div>
                <Input
                  className="template-page-title"
                  aria-label="Fixture page title"
                  value={title}
                  onChange={(event) => {
                    setTitle(event.target.value);
                    setTitleEdited(true);
                    setCreatedPage((page) =>
                      page ? { ...page, title: event.target.value } : page,
                    );
                  }}
                />
                {currentPage.revision === "Blank page" ? (
                  <div className="template-field">
                    <label htmlFor="blank-content">Blank fixture content</label>
                    <Textarea
                      id="blank-content"
                      value={blankContent}
                      onChange={(event) => {
                        setBlankContent(event.target.value);
                        setPageSaved(false);
                      }}
                      rows={8}
                      placeholder="Start writing…"
                    />
                    <p className="template-context-label">
                      Plain-text fixture editor. Production uses the existing page editor.
                    </p>
                  </div>
                ) : (
                  <>
                    {Object.keys(properties).length ? (
                      <div className="template-properties">
                        <span>Properties · fixture snapshot</span>
                        {Object.entries(properties).map(([key, value]) => (
                          <b key={key}>
                            {key}
                            <i>{String(value)}</i>
                          </b>
                        ))}
                      </div>
                    ) : null}
                    <div className="template-section-heading">
                      <strong>Incident sections</strong>
                      <span>
                        Guidance is separate fixture metadata. Only authored responses enter
                        document content.
                      </span>
                      <div>
                        <Button
                          variant="ghost"
                          size="sm"
                          aria-label="Undo response edit"
                          disabled={!responseUndo.length}
                          onClick={undoResponse}
                        >
                          <Undo2 size={16} />
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          aria-label="Redo response edit"
                          disabled={!responseRedo.length}
                          onClick={redoResponse}
                        >
                          <Redo2 size={16} />
                        </Button>
                      </div>
                    </div>
                    <div className="template-guidance-sections">
                      {INCIDENT_SECTIONS.map((section) => (
                        <section
                          className="template-guidance-section"
                          key={section.id}
                          data-guidance-id={section.id}
                        >
                          <h2>{section.title}</h2>
                          {!currentPage.responses[section.id] ? (
                            <Button
                              variant="ghost"
                              className="template-guidance-card"
                              aria-label={`Replace ${section.title} guidance`}
                              onClick={() =>
                                document.getElementById(`response-${section.id}`)?.focus()
                              }
                            >
                              <span>Instructional guidance · proposed structured fixture</span>
                              <span>{section.guidance}</span>
                            </Button>
                          ) : null}
                          <label htmlFor={`response-${section.id}`}>Your response</label>
                          <Textarea
                            id={`response-${section.id}`}
                            value={currentPage.responses[section.id]}
                            placeholder="Replace the guidance by writing your response here…"
                            rows={section.id === "timeline" ? 3 : 2}
                            onChange={(event) => setResponse(section.id, event.target.value)}
                          />
                        </section>
                      ))}
                    </div>
                    <div className="template-output-block">
                      <strong>Document output · plain-text fixture binding</strong>
                      <span>
                        Field values and authored responses render as React text nodes. Structured
                        guidance remains separate and unresolved guidance is omitted.
                      </span>
                      <FixtureOutput
                        fields={currentPage.fields}
                        responses={currentPage.responses}
                        title={currentPage.title}
                      />
                    </div>
                  </>
                )}
                <div className="template-save-row">
                  <span>
                    {pageSaved ? (
                      <>
                        <Check size={15} /> Saved in fixture
                      </>
                    ) : (
                      "Unsaved fixture page"
                    )}
                  </span>
                  <Button
                    onClick={() => {
                      setPageSaved(true);
                      setNotice("Fixture page saved locally for this prototype only.");
                    }}
                  >
                    Save fixture page
                  </Button>
                </div>
              </div>
            ) : selectedId === "blank" ? (
              <div className="template-preview-empty">
                <FileText size={25} />
                <h2>Blank page selected</h2>
                <p>Choose a destination, name the page, and start with an empty document.</p>
                <Button onClick={createBlank}>Open blank fixture page</Button>
              </div>
            ) : !selected ? null : (
              <>
                <div className="template-preview-header">
                  <div>
                    <p className="template-context-label">Template preview</p>
                    <h2>{selected.name}</h2>
                    <p>{selected.description}</p>
                  </div>
                  <span className="template-revision">{selected.revision}</span>
                </div>
                <div className="template-preview-paper">
                  <p className="template-document-location">{selected.location}</p>
                  <h3>{formatIncidentTitle(fields)}</h3>
                  <div className="template-preview-property-row">
                    <span>
                      Severity <b>{fields.severity}</b>
                    </span>
                    <span>
                      Owner <b>{fields.owner}</b>
                    </span>
                  </div>
                  <div className="template-preview-body">
                    {INCIDENT_SECTIONS.map((section) => (
                      <div key={section.id}>
                        <MarkdownPreview content={`### ${section.title}`} />
                        <p className="template-guidance-preview">Guidance · {section.guidance}</p>
                        {section.id === "summary" ? (
                          <p>
                            Service: {fields.service}
                            <br />
                            Severity: {fields.severity}
                            <br />
                            Incident date: {fields.date}
                            <br />
                            Owner: {fields.owner}
                          </p>
                        ) : null}
                      </div>
                    ))}
                  </div>
                </div>
                <div className="template-fields-panel">
                  <div>
                    <div>
                      <p className="template-context-label">Required before creation</p>
                      <h3>Incident details</h3>
                    </div>
                    <span>4 fields</span>
                  </div>
                  <div className="template-fields-grid">
                    <FieldInput
                      id="field-service"
                      label="Service"
                      value={fields.service}
                      error={fieldErrors.service}
                      onChange={(value) => setField("service", value)}
                    />
                    <div className="template-field">
                      <label htmlFor="field-severity">
                        Severity <span aria-hidden="true">*</span>
                      </label>
                      <Select
                        id="field-severity"
                        value={fields.severity}
                        onChange={(event) => setField("severity", event.target.value)}
                      >
                        <option value="">Choose severity</option>
                        <option value="SEV-1">SEV-1</option>
                        <option value="SEV-2">SEV-2</option>
                        <option value="SEV-3">SEV-3</option>
                        <option value="SEV-4">SEV-4</option>
                      </Select>
                      {fieldErrors.severity ? (
                        <span className="template-error">{fieldErrors.severity}</span>
                      ) : null}
                    </div>
                    <FieldInput
                      id="field-date"
                      label="Incident date"
                      type="date"
                      value={fields.date}
                      error={fieldErrors.date}
                      onChange={(value) => setField("date", value)}
                    />
                    <FieldInput
                      id="field-owner"
                      label="Owner"
                      value={fields.owner}
                      error={fieldErrors.owner}
                      onChange={(value) => setField("owner", value)}
                    />
                  </div>
                  <div className="template-binding-note">
                    <Info size={15} />
                    <span>
                      Stable fixture field IDs populate the generated title, summary, and matching
                      page properties. Values remain here while you preview.
                    </span>
                  </div>
                </div>
                <div className="template-create-footer">
                  <div className="template-title-summary">
                    <span>Page title</span>
                    <Input
                      aria-label="Page title"
                      value={title}
                      onChange={(event) => {
                        setTitle(event.target.value);
                        setTitleEdited(true);
                      }}
                    />
                    <small>Generated from Service and Incident date. You can edit it.</small>
                  </div>
                  <div className="flex flex-col gap-2">
                    <Button onClick={useTemplate}>Use template</Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      aria-pressed={failNextCreation}
                      onClick={() => setFailNextCreation(!failNextCreation)}
                    >
                      Simulate creation failure
                    </Button>
                  </div>
                </div>
              </>
            )}
          </section>
        </div>
      ) : null}

      {flow === "save" ? (
        <section className="template-flow-page">
          <div className="template-flow-heading">
            <p className="template-context-label">Reuse an existing page</p>
            <h1>Save as template</h1>
            <p>Review what will carry forward. The original page remains independent.</p>
          </div>
          <div className="template-save-layout">
            <div className="template-source-card">
              <span className="template-fixture-chip">Source fixture page</span>
              <h2>Payments API incident — retry storm</h2>
              <p>Engineering / Incidents · Updated Oct 7, 2026</p>
              <div className="template-property-list">
                <span>
                  Severity <b>SEV-2</b>
                </span>
                <span>
                  Service <b>Payments API</b>
                </span>
                <span>
                  Owner <b>Maya Chen</b>
                </span>
              </div>
              <div className="template-source-content">
                <h3>Summary</h3>
                <p>
                  Intermittent retries increased after the 09:12 UTC deploy. Most requests recovered
                  without manual intervention.
                </p>
                <h3>Impact</h3>
                <p>Some checkout attempts took longer than usual.</p>
                <h3>Timeline</h3>
                <p>09:12 deploy · 09:18 alert · 09:30 mitigation</p>
              </div>
              <Button
                variant="secondary"
                onClick={() => setNotice("The source fixture page is still unchanged.")}
              >
                Check source unchanged
              </Button>
            </div>
            <div className="template-save-form">
              <div className="template-revision-callout">
                <Info size={17} />
                <span>
                  <strong>Source revision: Published revision 7</strong>
                  <small>
                    The current published version is copied. A private draft is not substituted.
                  </small>
                </span>
              </div>
              <div className="template-field">
                <label htmlFor="save-template-name">Template name</label>
                <Input
                  id="save-template-name"
                  value={saveName}
                  onChange={(event) => setSaveName(event.target.value)}
                />
              </div>
              <div className="template-field">
                <label htmlFor="save-template-description">Description</label>
                <Textarea
                  id="save-template-description"
                  value={saveDescription}
                  onChange={(event) => setSaveDescription(event.target.value)}
                  rows={3}
                />
              </div>
              <div className="template-field">
                <label htmlFor="save-template-space">Destination space</label>
                <Select id="save-template-space" defaultValue="engineering">
                  <option value="engineering">Engineering</option>
                  <option value="personal" disabled>
                    Other spaces · asset policy pending
                  </option>
                </Select>
              </div>
              <div className="template-include-options">
                <strong>Include from source</strong>
                {[
                  [includeTags, setIncludeTags, "Tags", "incident, operations"],
                  [
                    includeProperties,
                    setIncludeProperties,
                    "Properties",
                    "Severity, Service, Owner",
                  ],
                  [
                    includePresentation,
                    setIncludePresentation,
                    "Presentation",
                    "Wide page width, icon, cover",
                  ],
                ].map(([checked, setter, label, detail]) => (
                  <label key={String(label)}>
                    <input
                      type="checkbox"
                      checked={checked as boolean}
                      onChange={(event) =>
                        (setter as (value: boolean) => void)(event.target.checked)
                      }
                    />
                    <span>
                      <b>{String(label)}</b>
                      <small>{String(detail)}</small>
                    </span>
                  </label>
                ))}
              </div>
              <p className="template-privacy-note">
                Permissions, comments, page history, and private collaboration data are never
                copied. Embedded assets are disabled because cross-space access is not verified.
              </p>
              <Button
                className="template-wide-button"
                disabled={!saveName.trim()}
                onClick={() => setSaveDialog(true)}
              >
                Review fixture template
              </Button>
              {fixtureSaved ? (
                <p className="template-success-line">
                  <Check size={16} /> Fixture template ready for visual review
                </p>
              ) : null}
            </div>
          </div>
        </section>
      ) : null}

      {flow === "test" ? (
        <section className="template-flow-page template-test-page">
          <div className="template-flow-heading">
            <p className="template-context-label">Authoring workspace</p>
            <h1>Test template</h1>
            <p>
              Try sample values against a revision and inspect the resulting document without
              creating a page.
            </p>
          </div>
          <div className="template-test-callout">
            <FlaskConical size={17} />
            <span>
              <strong>Test mode · fixture only</strong>
              <small>No page, revision, or server record is created here.</small>
            </span>
            <label htmlFor="test-revision">Revision</label>
            <Select
              id="test-revision"
              value={revision}
              onChange={(event) => setRevision(event.target.value as TemplateRevision)}
            >
              <option value="published">Published revision 4</option>
              <option value="draft">Working draft simulation</option>
            </Select>
          </div>
          <div className="template-test-grid">
            <div className="template-test-inputs">
              <h2>Sample fields</h2>
              <p>Change a value to see every title, body, and property binding update.</p>
              <FieldInput
                id="sample-service"
                label="Service"
                value={sample.service}
                error={!sample.service.trim() ? "Required to render this sample." : undefined}
                onChange={(value) => setSample((current) => ({ ...current, service: value }))}
              />
              <div className="template-field">
                <label htmlFor="sample-severity">
                  Severity <span aria-hidden="true">*</span>
                </label>
                <Select
                  id="sample-severity"
                  value={sample.severity}
                  onChange={(event) =>
                    setSample((current) => ({ ...current, severity: event.target.value }))
                  }
                >
                  <option value="">Choose severity</option>
                  <option value="SEV-1">SEV-1</option>
                  <option value="SEV-2">SEV-2</option>
                  <option value="SEV-3">SEV-3</option>
                  <option value="SEV-4">SEV-4</option>
                </Select>
                {!sample.severity.trim() ? (
                  <span className="template-error">Required to render this sample.</span>
                ) : null}
              </div>
              <FieldInput
                id="sample-date"
                label="Incident date"
                type="date"
                value={sample.date}
                error={!sample.date.trim() ? "Required to render this sample." : undefined}
                onChange={(value) => setSample((current) => ({ ...current, date: value }))}
              />
              <FieldInput
                id="sample-owner"
                label="Owner"
                value={sample.owner}
                error={!sample.owner.trim() ? "Required to render this sample." : undefined}
                onChange={(value) => setSample((current) => ({ ...current, owner: value }))}
              />
              <div className="template-field">
                <label htmlFor="sample-impact">Impact sample response</label>
                <Textarea
                  id="sample-impact"
                  value={sampleResponses.impact}
                  placeholder="Write a sample impact response"
                  rows={2}
                  onChange={(event) => setSampleResponse("impact", event.target.value)}
                />
              </div>
              {sampleErrors.length ? (
                <p className="template-error" role="alert">
                  Complete required sample fields to render: {sampleErrors.join(", ")}.
                </p>
              ) : null}
              <Button
                variant="secondary"
                onClick={() => setSample({ ...INITIAL_FIELDS, service: "Identity" })}
              >
                Reset sample
              </Button>
            </div>
            <div className="template-test-output">
              <div className="template-output-label">
                <span>Generated result · plain-text fixture binding</span>
                <span className="template-fixture-chip">
                  {revision === "published" ? "Published r4" : "Draft simulation"}
                </span>
              </div>
              <FixtureOutput fields={sample} responses={sampleResponses} />
              <div className="template-guidance-list">
                <h3>Unresolved guidance · excluded from document output</h3>
                {INCIDENT_SECTIONS.filter((section) => !sampleResponses[section.id].trim()).map(
                  (section) => (
                    <p key={section.id}>
                      <b>{section.title}</b> · {guidanceForRevision(section.id, revision)}
                    </p>
                  ),
                )}
              </div>
              <div className="template-defaults">
                <h3>Simulated defaults</h3>
                <span>
                  Tags <b>{sampleDefaults}</b>
                </span>
                <span>
                  Page width <b>Wide</b>
                </span>
                <span>
                  Presentation <b>Icon · cover</b>
                </span>
              </div>
            </div>
          </div>
          <div className="template-independence">
            <Check size={17} />
            <span>
              <strong>Existing pages stay independent</strong>
              <small>
                {createdPage
                  ? `${createdPage.title} was created from ${createdPage.revision}; its saved field and response snapshot remains unchanged as you simulate another revision.`
                  : "A fixture page created in the Create page flow remains frozen to its recorded fields and revision."}
              </small>
            </span>
            <Button
              variant="secondary"
              onClick={() => {
                setRevision((current) => (current === "published" ? "draft" : "published"));
                setNotice(
                  "The future-use fixture changed revision. Any existing fixture page snapshot remains unchanged.",
                );
              }}
            >
              Simulate template revision
            </Button>
          </div>
        </section>
      ) : null}
      <div className="template-prototype-status" role="status" aria-live="polite">
        {notice || "All content is fixture data for prototype review."}
      </div>
      {saveDialog ? (
        <DialogShell
          labelledBy="template-review-title"
          describedBy="template-review-description"
          onClose={() => setSaveDialog(false)}
        >
          <DialogHeader>
            <div>
              <DialogTitle id="template-review-title">Review template details</DialogTitle>
              <DialogDescription id="template-review-description">
                Check the published revision and selected defaults before saving this fixture.
              </DialogDescription>
            </div>
          </DialogHeader>
          <DialogBody>
            <div className="template-dialog-summary">
              <span>
                Template <b>{saveName}</b>
              </span>
              <span>
                Description <b>{saveDescription}</b>
              </span>
              <span>
                Copied from <b>Published revision 7</b>
              </span>
              <span>
                Destination <b>Engineering</b>
              </span>
              <span>
                Included{" "}
                <b>
                  {[
                    includeTags && "tags",
                    includeProperties && "properties",
                    includePresentation && "presentation",
                  ]
                    .filter(Boolean)
                    .join(", ") || "content only"}
                </b>
              </span>
            </div>
            <p className="template-privacy-note">
              This action only updates local prototype state. The original fixture page remains
              unchanged.
            </p>
          </DialogBody>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setSaveDialog(false)}>
              Back to review
            </Button>
            <Button onClick={saveFixtureTemplate}>Save fixture template</Button>
          </DialogFooter>
        </DialogShell>
      ) : null}
    </main>
  );
}
