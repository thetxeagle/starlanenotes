import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  ArrowRight,
  AlertTriangle,
  BookmarkPlus,
  CheckCircle2,
  Clock3,
  Copy,
  FileText,
  Filter,
  Layers3,
  RefreshCw,
  Trash2,
  UserCheck,
  Users,
  Workflow,
  X,
} from "lucide-react";
import { useAuth } from "@/hooks/useAuth";
import {
  dataClient,
  type DocumentRecord,
  type DocumentWorkflowEventRecord,
  type WorkflowTemplateApproverRecord,
  type WorkflowTemplateRecord,
} from "@/lib/dataClient";
import { Button } from "@/components/ui/button";
import { DialogShell } from "@/components/ui/dialog-shell";
import { Input } from "@/components/ui/input";
import { SearchInput } from "@/components/ui/search-input";
import { Select } from "@/components/ui/select";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";

type QueueStatus = "active" | "completed";
export type QueueScope = "all" | "mine" | "role" | "unassigned";
export type QueueSort =
  | "updated_desc"
  | "updated_asc"
  | "title_asc"
  | "workflow_asc"
  | "deadline_asc";
export type QueueAge = "all" | "today" | "week" | "stale";
export type WorkflowHandoffAction = "delegate" | "escalate";
export type QueueFocus = "all" | "delegated" | "escalated" | "due_soon" | "overdue";
export type WorkflowReassignmentTargetSource = "approver" | "handoff" | "current_user" | "admin";
export type WorkflowNamingTarget = "queue" | "rule";

export interface WorkflowQueuePreset {
  id: string;
  name: string;
  status: QueueStatus;
  templateId: string;
  stageId: string;
  scope: QueueScope;
  query: string;
  sort: QueueSort;
  age: QueueAge;
  focus: QueueFocus;
}

const PRESET_STORAGE_KEY = "starlane.workflowQueue.presets.v1";
const REASSIGNMENT_RULE_STORAGE_KEY = "starlane.workflowQueue.reassignmentRules.v1";

export const WORKFLOW_QUEUE_ERROR_CLASS =
  "mb-3 rounded-input border border-danger/30 bg-danger/10 px-3 py-2 text-sm text-danger";

export function getWorkflowNamingDialogTitle(target: WorkflowNamingTarget): string {
  return target === "queue" ? "Save queue view" : "Save reassignment rule";
}

export function getWorkflowNamingDialogLabel(target: WorkflowNamingTarget): string {
  return target === "queue" ? "Queue view name" : "Reassignment rule name";
}

export function getWorkflowNamingDialogDescription(target: WorkflowNamingTarget): string {
  return target === "queue"
    ? "Save the current workflow filters so you can reuse this queue view."
    : "Save the current delegation or escalation settings for selected workflow rows.";
}

export function canSaveWorkflowNamingValue(value: string): boolean {
  return value.trim().length > 0;
}

export function getWorkflowQueueDocumentPath(spaceId: string, documentId: string): string {
  return buildDocumentRoutePath(spaceId, documentId);
}

export function getWorkflowAttentionDueClass(dueStatus: WorkflowAttention["dueStatus"]): string {
  if (dueStatus === "overdue") {
    return "border-danger/30 bg-danger/10 text-danger";
  }
  if (dueStatus === "due_soon") {
    return "border-warning/30 bg-warning/10 text-warning";
  }
  return "border-border bg-surface text-text-muted";
}

export interface WorkflowReassignmentTarget {
  id: string;
  label: string;
  value: string;
  source: WorkflowReassignmentTargetSource;
}

export interface WorkflowReassignmentRule {
  id: string;
  name: string;
  action: WorkflowHandoffAction;
  target: string;
  note: string;
  dueDate: string;
  createdAt: string;
}

export function WorkflowQueuePage() {
  const { spaceId } = useParams<{ spaceId: string }>();
  const navigate = useNavigate();
  const { user } = useAuth();
  const [status, setStatus] = useState<QueueStatus>("active");
  const [templateId, setTemplateId] = useState("");
  const [stageId, setStageId] = useState("");
  const [scope, setScope] = useState<QueueScope>("all");
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<QueueSort>("updated_desc");
  const [age, setAge] = useState<QueueAge>("all");
  const [focus, setFocus] = useState<QueueFocus>("all");
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [bulkStageId, setBulkStageId] = useState("");
  const [bulkNote, setBulkNote] = useState("");
  const [bulkBusy, setBulkBusy] = useState(false);
  const [handoffAction, setHandoffAction] = useState<WorkflowHandoffAction>("delegate");
  const [handoffTarget, setHandoffTarget] = useState("");
  const [handoffNote, setHandoffNote] = useState("");
  const [handoffDueDate, setHandoffDueDate] = useState("");
  const [handoffBusy, setHandoffBusy] = useState(false);
  const [presets, setPresets] = useState<WorkflowQueuePreset[]>([]);
  const [reassignmentRules, setReassignmentRules] = useState<WorkflowReassignmentRule[]>([]);
  const [activeReassignmentRuleId, setActiveReassignmentRuleId] = useState("");
  const [message, setMessage] = useState("");
  const [templates, setTemplates] = useState<WorkflowTemplateRecord[]>([]);
  const [documents, setDocuments] = useState<DocumentRecord[]>([]);
  const [workflowEventsByDocId, setWorkflowEventsByDocId] = useState<
    Map<string, DocumentWorkflowEventRecord[]>
  >(new Map());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [namingTarget, setNamingTarget] = useState<WorkflowNamingTarget | null>(null);
  const [namingValue, setNamingValue] = useState("");

  const selectedTemplate = useMemo(
    () => templates.find((template) => template.id === templateId),
    [templateId, templates],
  );

  const stages = selectedTemplate?.stages ?? [];
  const templateById = useMemo(
    () => new Map(templates.map((template) => [template.id, template])),
    [templates],
  );

  const stageSummaries = useMemo(() => {
    const counts = new Map<string, { id: string; name: string; count: number }>();
    documents.forEach((doc) => {
      const id = doc.workflow_stage_id ?? "unknown";
      const name = doc.workflow_stage_name ?? "Unknown stage";
      const current = counts.get(id) ?? { id, name, count: 0 };
      current.count += 1;
      counts.set(id, current);
    });
    return [...counts.values()].sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
  }, [documents]);

  const visibleDocuments = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    return documents
      .filter((doc) =>
        matchesQueueScope(doc, templateById.get(doc.workflow_template_id ?? ""), scope, user),
      )
      .filter((doc) => matchesQueueAge(doc, age))
      .filter((doc) => matchesWorkflowFocus(workflowEventsByDocId.get(doc.id) ?? [], focus))
      .filter((doc) => {
        if (!normalizedQuery) return true;
        const workflowAttention = getWorkflowAttention(workflowEventsByDocId.get(doc.id) ?? []);
        const haystack = [
          doc.title,
          doc.workflow_template_name,
          doc.workflow_stage_name,
          workflowAttention.target,
          workflowAttention.dueDate,
          workflowAttention.latestNote,
          formatApprovers(
            getApplicableApprovers(
              templateById.get(doc.workflow_template_id ?? ""),
              doc.workflow_stage_id,
            ),
          ),
        ]
          .filter(Boolean)
          .join(" ")
          .toLowerCase();
        return haystack.includes(normalizedQuery);
      })
      .sort((a, b) =>
        compareQueueDocuments(
          a,
          b,
          sort,
          workflowEventsByDocId.get(a.id) ?? [],
          workflowEventsByDocId.get(b.id) ?? [],
        ),
      );
  }, [age, documents, focus, query, scope, sort, templateById, user, workflowEventsByDocId]);

  const selectedDocuments = useMemo(
    () => visibleDocuments.filter((doc) => selectedIds.includes(doc.id)),
    [selectedIds, visibleDocuments],
  );
  const bulkTemplateId = useMemo(
    () => getSharedWorkflowTemplateId(selectedDocuments),
    [selectedDocuments],
  );
  const bulkTemplate = bulkTemplateId ? templateById.get(bulkTemplateId) : undefined;
  const bulkStages = useMemo(
    () => getBulkTransitionStages(bulkTemplate, selectedDocuments),
    [bulkTemplate, selectedDocuments],
  );

  const ageCounts = useMemo(
    () => ({
      today: documents.filter((doc) => matchesQueueAge(doc, "today")).length,
      week: documents.filter((doc) => matchesQueueAge(doc, "week")).length,
      stale: documents.filter((doc) => matchesQueueAge(doc, "stale")).length,
    }),
    [documents],
  );

  const approvalCounts = useMemo(
    () => ({
      mine: documents.filter((doc) =>
        matchesQueueScope(doc, templateById.get(doc.workflow_template_id ?? ""), "mine", user),
      ).length,
      role: documents.filter((doc) =>
        matchesQueueScope(doc, templateById.get(doc.workflow_template_id ?? ""), "role", user),
      ).length,
      unassigned: documents.filter((doc) =>
        matchesQueueScope(doc, templateById.get(doc.workflow_template_id ?? ""), "unassigned", user),
      ).length,
    }),
    [documents, templateById, user],
  );

  const focusCounts = useMemo(
    () => ({
      delegated: documents.filter((doc) =>
        matchesWorkflowFocus(workflowEventsByDocId.get(doc.id) ?? [], "delegated"),
      ).length,
      escalated: documents.filter((doc) =>
        matchesWorkflowFocus(workflowEventsByDocId.get(doc.id) ?? [], "escalated"),
      ).length,
      dueSoon: documents.filter((doc) =>
        matchesWorkflowFocus(workflowEventsByDocId.get(doc.id) ?? [], "due_soon"),
      ).length,
      overdue: documents.filter((doc) =>
        matchesWorkflowFocus(workflowEventsByDocId.get(doc.id) ?? [], "overdue"),
      ).length,
    }),
    [documents, workflowEventsByDocId],
  );

  const deadlineEscalationCandidates = useMemo(
    () => getDeadlineEscalationCandidates(visibleDocuments, workflowEventsByDocId),
    [visibleDocuments, workflowEventsByDocId],
  );
  const reassignmentTargets = useMemo(
    () =>
      getWorkflowReassignmentTargets(
        selectedDocuments,
        templateById,
        workflowEventsByDocId,
        user,
      ),
    [selectedDocuments, templateById, workflowEventsByDocId, user],
  );

  const loadQueue = useCallback(async () => {
    if (!spaceId) return;
    setLoading(true);
    setError("");
    try {
      const [nextTemplates, nextDocuments] = await Promise.all([
        dataClient.listWorkflowTemplates(spaceId),
        dataClient.listDocuments(spaceId, {
          include_nested: true,
          workflow_status: status,
          workflow_template_id: templateId || undefined,
          workflow_stage_id: stageId || undefined,
        }),
      ]);
      const workflowDocuments = nextDocuments.filter((doc) => doc.workflow_template_id);
      const eventEntries = await Promise.all(
        workflowDocuments.map(async (doc) => {
          const events = await dataClient
            .listDocumentWorkflowEvents(spaceId, doc.id)
            .catch(() => []);
          return [doc.id, events] as const;
        }),
      );
      setTemplates(nextTemplates);
      setDocuments(workflowDocuments);
      setWorkflowEventsByDocId(new Map(eventEntries));
    } catch {
      setError("Unable to load workflow queue.");
    } finally {
      setLoading(false);
    }
  }, [spaceId, stageId, status, templateId]);

  useEffect(() => {
    void loadQueue();
  }, [loadQueue]);

  useEffect(() => {
    setStageId("");
  }, [templateId]);

  useEffect(() => {
    if (!spaceId) return;
    setPresets(readWorkflowQueuePresets(spaceId));
    setReassignmentRules(readWorkflowReassignmentRules(spaceId));
    setActiveReassignmentRuleId("");
  }, [spaceId]);

  useEffect(() => {
    const visibleIds = new Set(visibleDocuments.map((doc) => doc.id));
    setSelectedIds((current) => current.filter((id) => visibleIds.has(id)));
  }, [visibleDocuments]);

  useEffect(() => {
    setBulkStageId((current) =>
      bulkStages.some((stage) => stage.id === current) ? current : (bulkStages[0]?.id ?? ""),
    );
  }, [bulkStages]);

  if (!spaceId) return null;
  const activeSpaceId = spaceId;

  const allVisibleSelected =
    visibleDocuments.length > 0 && visibleDocuments.every((doc) => selectedIds.includes(doc.id));

  const activeFilters = [
    status !== "active" ? { label: "Completed", clear: () => setStatus("active") } : null,
    templateId
      ? { label: selectedTemplate?.name ?? "Template", clear: () => setTemplateId("") }
      : null,
    stageId
      ? { label: stages.find((stage) => stage.id === stageId)?.name ?? "Stage", clear: () => setStageId("") }
      : null,
    scope !== "all" ? { label: scopeLabel(scope), clear: () => setScope("all") } : null,
    age !== "all" ? { label: ageLabel(age), clear: () => setAge("all") } : null,
    focus !== "all" ? { label: focusLabel(focus), clear: () => setFocus("all") } : null,
    query.trim() ? { label: `Search: ${query.trim()}`, clear: () => setQuery("") } : null,
  ].filter((filter): filter is { label: string; clear: () => void } => Boolean(filter));

  function toggleSelectAllVisible() {
    setSelectedIds(allVisibleSelected ? [] : visibleDocuments.map((doc) => doc.id));
  }

  function toggleSelected(id: string) {
    setSelectedIds((current) =>
      current.includes(id) ? current.filter((item) => item !== id) : [...current, id],
    );
  }

  async function copySelectedLinks() {
    if (selectedDocuments.length === 0) return;
    const text = serializeQueueLinks(selectedDocuments, activeSpaceId);
    try {
      await navigator.clipboard.writeText(text);
      setMessage(
        `Copied ${selectedDocuments.length} workflow ${
          selectedDocuments.length === 1 ? "link" : "links"
        }.`,
      );
    } catch {
      setMessage("Unable to copy links from this browser.");
    }
  }

  function saveCurrentPreset() {
    setNamingTarget("queue");
    setNamingValue("");
  }

  function closeNamingDialog() {
    setNamingTarget(null);
    setNamingValue("");
  }

  function saveNamedQueuePreset(name: string) {
    const nextPreset = createWorkflowQueuePreset(name, {
      status,
      templateId,
      stageId,
      scope,
      query,
      sort,
      age,
      focus,
    });
    const nextPresets = [
      nextPreset,
      ...presets.filter((preset) => preset.name !== nextPreset.name),
    ].slice(0, 8);
    setPresets(nextPresets);
    writeWorkflowQueuePresets(activeSpaceId, nextPresets);
    setMessage(`Saved queue view "${nextPreset.name}".`);
  }

  function saveNamedReassignmentRule(name: string) {
    const nextRule = createWorkflowReassignmentRule(name, {
      action: handoffAction,
      target: handoffTarget,
      note: handoffNote,
      dueDate: handoffDueDate,
    });
    const nextRules = [
      nextRule,
      ...reassignmentRules.filter((rule) => rule.name !== nextRule.name),
    ].slice(0, 12);
    setReassignmentRules(nextRules);
    setActiveReassignmentRuleId(nextRule.id);
    writeWorkflowReassignmentRules(activeSpaceId, nextRules);
    setMessage(`Saved reassignment rule "${nextRule.name}".`);
  }

  function saveNamedWorkflowItem() {
    const name = namingValue.trim();
    if (!namingTarget || !name) return;
    if (namingTarget === "queue") {
      saveNamedQueuePreset(name);
      closeNamingDialog();
      return;
    }
    if (!formatWorkflowHandoffNote(handoffAction, handoffTarget, handoffNote, handoffDueDate)) {
      setMessage("Add a delegation or escalation target/note before saving a rule.");
      closeNamingDialog();
      return;
    }
    saveNamedReassignmentRule(name);
    closeNamingDialog();
  }

  function applyPreset(preset: WorkflowQueuePreset) {
    setStatus(preset.status);
    setTemplateId(preset.templateId);
    setStageId(preset.stageId);
    setScope(preset.scope);
    setQuery(preset.query);
    setSort(preset.sort);
    setAge(preset.age);
    setFocus(preset.focus ?? "all");
    setMessage(`Applied queue view "${preset.name}".`);
  }

  function deletePreset(id: string) {
    const nextPresets = presets.filter((preset) => preset.id !== id);
    setPresets(nextPresets);
    writeWorkflowQueuePresets(activeSpaceId, nextPresets);
  }

  function saveCurrentReassignmentRule() {
    if (!formatWorkflowHandoffNote(handoffAction, handoffTarget, handoffNote, handoffDueDate)) {
      setMessage("Add a delegation or escalation target/note before saving a rule.");
      return;
    }
    setNamingTarget("rule");
    setNamingValue("");
  }

  function applyReassignmentRule(id: string) {
    setActiveReassignmentRuleId(id);
    const rule = reassignmentRules.find((item) => item.id === id);
    if (!rule) return;
    setHandoffAction(rule.action);
    setHandoffTarget(rule.target);
    setHandoffNote(rule.note);
    setHandoffDueDate(rule.dueDate);
    setMessage(`Applied reassignment rule "${rule.name}".`);
  }

  function deleteActiveReassignmentRule() {
    if (!activeReassignmentRuleId) return;
    const deleted = reassignmentRules.find((rule) => rule.id === activeReassignmentRuleId);
    const nextRules = reassignmentRules.filter((rule) => rule.id !== activeReassignmentRuleId);
    setReassignmentRules(nextRules);
    setActiveReassignmentRuleId("");
    writeWorkflowReassignmentRules(activeSpaceId, nextRules);
    if (deleted) setMessage(`Deleted reassignment rule "${deleted.name}".`);
  }

  async function transitionSelectedDocuments() {
    if (!bulkStageId || selectedDocuments.length === 0 || !bulkTemplate) return;
    setBulkBusy(true);
    setMessage("");
    let moved = 0;
    let failed = 0;
    for (const doc of selectedDocuments) {
      if (doc.workflow_stage_id === bulkStageId) continue;
      try {
        await dataClient.transitionDocumentWorkflow(activeSpaceId, doc.id, {
          stage_id: bulkStageId,
          note: bulkNote.trim() || undefined,
        });
        moved += 1;
      } catch {
        failed += 1;
      }
    }
    setBulkBusy(false);
    setBulkNote("");
    setSelectedIds([]);
    await loadQueue();
    setMessage(formatBulkTransitionMessage(moved, failed, selectedDocuments.length));
  }

  async function recordSelectedWorkflowEvents() {
    if (selectedDocuments.length === 0) return;
    const note = formatWorkflowHandoffNote(
      handoffAction,
      handoffTarget,
      handoffNote,
      handoffDueDate,
    );
    if (!note) {
      setMessage("Add a delegation or escalation target/note before recording the handoff.");
      return;
    }
    setHandoffBusy(true);
    let recorded = 0;
    let failed = 0;
    for (const doc of selectedDocuments) {
      try {
        await dataClient.recordDocumentWorkflowEvent(activeSpaceId, doc.id, {
          action: handoffAction,
          note,
        });
        recorded += 1;
      } catch {
        failed += 1;
      }
    }
    setHandoffBusy(false);
    setHandoffTarget("");
    setHandoffNote("");
    setHandoffDueDate("");
    setActiveReassignmentRuleId("");
    setSelectedIds([]);
    await loadQueue();
    setMessage(formatWorkflowHandoffMessage(recorded, failed, selectedDocuments.length, handoffAction));
  }

  async function escalateOverdueDelegations() {
    if (deadlineEscalationCandidates.length === 0) return;
    setHandoffBusy(true);
    let recorded = 0;
    let failed = 0;
    for (const candidate of deadlineEscalationCandidates) {
      const attention = getWorkflowAttention(workflowEventsByDocId.get(candidate.id) ?? []);
      const target = attention.target || "admin";
      const dueDetail = attention.dueDate ? `overdue since ${attention.dueDate}` : "overdue";
      try {
        await dataClient.recordDocumentWorkflowEvent(activeSpaceId, candidate.id, {
          action: "escalate",
          note: formatWorkflowHandoffNote("escalate", target, dueDetail),
        });
        recorded += 1;
      } catch {
        failed += 1;
      }
    }
    setHandoffBusy(false);
    await loadQueue();
    setMessage(formatDeadlineEscalationMessage(recorded, failed, deadlineEscalationCandidates.length));
  }

  return (
    <div className="flex h-full flex-col">
      {namingTarget ? (
        <DialogShell
          labelledBy="workflow-naming-dialog-title"
          maxWidth="sm"
          onClose={closeNamingDialog}
        >
          <form
            className="p-3"
            onSubmit={(event) => {
              event.preventDefault();
              saveNamedWorkflowItem();
            }}
          >
            <div className="mb-3">
              <h2 id="workflow-naming-dialog-title" className="text-sm font-medium text-text">
                {getWorkflowNamingDialogTitle(namingTarget)}
              </h2>
              <p className="mt-1 text-xs text-text-muted">
                {getWorkflowNamingDialogDescription(namingTarget)}
              </p>
              <label
                htmlFor="workflow-naming-dialog-input"
                className="mt-3 block text-xs font-medium text-text-muted"
              >
                {getWorkflowNamingDialogLabel(namingTarget)}
              </label>
              <Input
                id="workflow-naming-dialog-input"
                value={namingValue}
                onChange={(event) => setNamingValue(event.target.value)}
                placeholder={namingTarget === "queue" ? "My review queue" : "Escalate to admins"}
                className="mt-1 h-8 bg-surface text-sm"
                autoComplete="off"
                autoFocus
              />
            </div>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" size="sm" onClick={closeNamingDialog}>
                Cancel
              </Button>
              <Button type="submit" size="sm" disabled={!canSaveWorkflowNamingValue(namingValue)}>
                Save
              </Button>
            </div>
          </form>
        </DialogShell>
      ) : null}
      <header className="border-b border-border px-4 py-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-sm font-semibold text-text">
              <Workflow size={16} />
              Workflow queue
            </div>
            <p className="mt-1 text-xs text-text-muted">
              {visibleDocuments.length} of {documents.length}{" "}
              {status === "active" ? "active" : "completed"} documents
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button type="button" variant="secondary" size="sm" onClick={saveCurrentPreset}>
              <BookmarkPlus size={14} />
              Save view
            </Button>
            <Button type="button" variant="secondary" size="sm" onClick={() => void loadQueue()}>
              <RefreshCw size={14} />
              Refresh
            </Button>
          </div>
        </div>
        <div className="mt-3 grid gap-2 md:grid-cols-[160px_1fr_1fr]">
          <Select
            value={status}
            onChange={(event) => setStatus(event.target.value as QueueStatus)}
            aria-label="Workflow status"
          >
            <option value="active">Active</option>
            <option value="completed">Completed</option>
          </Select>
          <Select
            value={templateId}
            onChange={(event) => setTemplateId(event.target.value)}
            aria-label="Workflow template"
          >
            <option value="">All templates</option>
            {templates.map((template) => (
              <option key={template.id} value={template.id}>
                {template.name}
              </option>
            ))}
          </Select>
          <Select
            value={stageId}
            onChange={(event) => setStageId(event.target.value)}
            disabled={!selectedTemplate}
            aria-label="Workflow stage"
          >
            <option value="">All stages</option>
            {stages.map((stage) => (
              <option key={stage.id} value={stage.id}>
                {stage.name}
              </option>
            ))}
          </Select>
        </div>
        <div className="mt-2 grid gap-2 md:grid-cols-[1fr_170px_200px_170px_190px]">
          <SearchInput
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search queue"
            aria-label="Search workflow queue"
          />
          <Select
            value={sort}
            onChange={(event) => setSort(event.target.value as QueueSort)}
            aria-label="Sort workflow queue"
          >
            <option value="updated_desc">Newest updated</option>
            <option value="updated_asc">Oldest updated</option>
            <option value="title_asc">Title A-Z</option>
            <option value="workflow_asc">Workflow A-Z</option>
            <option value="deadline_asc">Deadline first</option>
          </Select>
          <Select
            value={scope}
            onChange={(event) => setScope(event.target.value as QueueScope)}
            aria-label="Approval scope"
          >
            <option value="all">All approval scopes</option>
            <option value="mine">My approvals ({approvalCounts.mine})</option>
            <option value="role">Role approvals ({approvalCounts.role})</option>
            <option value="unassigned">No assigned approver ({approvalCounts.unassigned})</option>
          </Select>
          <Select
            value={age}
            onChange={(event) => setAge(event.target.value as QueueAge)}
            aria-label="Updated age"
          >
            <option value="all">Any age</option>
            <option value="today">Updated today ({ageCounts.today})</option>
            <option value="week">Updated this week ({ageCounts.week})</option>
            <option value="stale">Stale 7+ days ({ageCounts.stale})</option>
          </Select>
          <Select
            value={focus}
            onChange={(event) => setFocus(event.target.value as QueueFocus)}
            aria-label="Workflow attention"
          >
            <option value="all">Any handoff</option>
            <option value="delegated">Delegated ({focusCounts.delegated})</option>
            <option value="escalated">Escalated ({focusCounts.escalated})</option>
            <option value="due_soon">Due soon ({focusCounts.dueSoon})</option>
            <option value="overdue">Overdue ({focusCounts.overdue})</option>
          </Select>
        </div>
        {(activeFilters.length > 0 || presets.length > 0) && (
          <div className="mt-3 flex flex-wrap items-center gap-2">
            {activeFilters.map((filter) => (
              <button
                key={filter.label}
                type="button"
                onClick={filter.clear}
                className="inline-flex items-center gap-1 rounded-input border border-border bg-surface px-2 py-1 text-xs text-text hover:bg-surface-raised"
              >
                {filter.label}
                <X size={12} />
              </button>
            ))}
            {presets.map((preset) => (
              <span
                key={preset.id}
                className="inline-flex items-center gap-1 rounded-input border border-border bg-surface px-1 py-1 text-xs text-text"
              >
                <button
                  type="button"
                  onClick={() => applyPreset(preset)}
                  className="px-1 hover:text-primary"
                >
                  {preset.name}
                </button>
                <button
                  type="button"
                  onClick={() => deletePreset(preset.id)}
                  className="rounded-input p-0.5 text-text-muted hover:bg-surface-raised hover:text-text"
                  aria-label={`Delete saved queue view ${preset.name}`}
                >
                  <Trash2 size={12} />
                </button>
              </span>
            ))}
          </div>
        )}
        {stageSummaries.length > 0 && (
          <div className="mt-3 flex gap-2 overflow-x-auto pb-1">
            {stageSummaries.slice(0, 6).map((stage) => (
              <button
                key={stage.id}
                type="button"
                onClick={() => setStageId(stage.id === "unknown" ? "" : stage.id)}
                className="shrink-0 rounded-input border border-border bg-surface px-2.5 py-1.5 text-left text-xs text-text hover:bg-surface-raised"
              >
                <span className="block max-w-40 truncate font-medium">{stage.name}</span>
                <span className="text-text-muted">
                  {stage.count} {stage.count === 1 ? "document" : "documents"}
                </span>
              </button>
            ))}
          </div>
        )}
      </header>

      <main className="flex-1 overflow-y-auto p-4">
        {message && (
          <div className="mb-3 flex items-center justify-between gap-3 rounded-input border border-border bg-surface px-3 py-2 text-sm text-text">
            <span>{message}</span>
            <button
              type="button"
              onClick={() => setMessage("")}
              className="rounded-input p-1 text-text-muted hover:bg-surface-raised hover:text-text"
              aria-label="Dismiss workflow queue message"
            >
              <X size={14} />
            </button>
          </div>
        )}
        {error && (
          <div className={WORKFLOW_QUEUE_ERROR_CLASS}>
            {error}
          </div>
        )}
        {loading ? (
          <div className="space-y-2">
            {[0, 1, 2].map((item) => (
              <div
                key={item}
                className="h-12 animate-pulse rounded-input border border-border bg-surface"
              />
            ))}
          </div>
        ) : visibleDocuments.length === 0 ? (
          <div className="flex h-full min-h-64 items-center justify-center">
            <div className="text-center">
              {scope === "mine" || scope === "role" ? (
                <UserCheck size={28} className="mx-auto text-text-muted" />
              ) : query ? (
                <Filter size={28} className="mx-auto text-text-muted" />
              ) : (
                <CheckCircle2 size={28} className="mx-auto text-text-muted" />
              )}
              <p className="mt-2 text-sm text-text">
                {query || scope !== "all"
                  ? "No workflow documents match these filters"
                  : "No matching workflow documents"}
              </p>
            </div>
          </div>
        ) : (
          <div className="overflow-hidden rounded-input border border-border">
            <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border bg-surface px-3 py-2">
              <div className="flex items-center gap-2 text-xs text-text-muted">
                <Layers3 size={14} />
                {selectedDocuments.length > 0
                  ? `${selectedDocuments.length} selected`
                  : `${visibleDocuments.length} visible`}
              </div>
              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  variant="secondary"
                  size="sm"
                  disabled={selectedDocuments.length === 0}
                  onClick={() => void copySelectedLinks()}
                >
                  <Copy size={14} />
                  Copy links
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  size="sm"
                  disabled={selectedDocuments.length === 0}
                  onClick={() => {
                    const first = selectedDocuments[0];
                    if (spaceId && first) navigate(getWorkflowQueueDocumentPath(spaceId, first.id));
                  }}
                >
                  <FileText size={14} />
                  Open first
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  size="sm"
                  disabled={handoffBusy || deadlineEscalationCandidates.length === 0}
                  onClick={() => void escalateOverdueDelegations()}
                >
                  <AlertTriangle size={14} />
                  Escalate overdue
                </Button>
              </div>
            </div>
            {selectedDocuments.length > 0 && (
              <div className="grid gap-2 border-b border-border bg-surface px-3 py-2">
                <div className="grid gap-2 md:grid-cols-[1fr_1fr_auto]">
                {bulkTemplate ? (
                  <>
                    <Select
                      value={bulkStageId}
                      onChange={(event) => setBulkStageId(event.target.value)}
                      aria-label="Bulk workflow target stage"
                    >
                      {bulkStages.map((stage) => (
                        <option key={stage.id} value={stage.id}>
                          Move to {stage.name}
                        </option>
                      ))}
                    </Select>
                    <Input
                      value={bulkNote}
                      onChange={(event) => setBulkNote(event.target.value)}
                      placeholder="Optional transition note"
                      aria-label="Bulk workflow transition note"
                    />
                    <Button
                      type="button"
                      size="sm"
                      disabled={!bulkStageId || bulkBusy || bulkStages.length === 0}
                      onClick={() => void transitionSelectedDocuments()}
                    >
                      <ArrowRight size={14} />
                      {bulkBusy ? "Moving..." : "Move selected"}
                    </Button>
                  </>
                ) : (
                  <div className="text-xs text-text-muted md:col-span-3">
                    Select documents from one workflow template to move them together.
                  </div>
                )}
                </div>
                <div className="flex flex-wrap items-center gap-2 border-t border-border pt-2">
                  <Select
                    value={activeReassignmentRuleId}
                    onChange={(event) => applyReassignmentRule(event.target.value)}
                    disabled={reassignmentRules.length === 0}
                    aria-label="Saved workflow reassignment rule"
                  >
                    <option value="">Saved reassignment rule</option>
                    {reassignmentRules.map((rule) => (
                      <option key={rule.id} value={rule.id}>
                        {rule.name}
                      </option>
                    ))}
                  </Select>
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    onClick={saveCurrentReassignmentRule}
                  >
                    <BookmarkPlus size={14} />
                    Save rule
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    disabled={!activeReassignmentRuleId}
                    onClick={deleteActiveReassignmentRule}
                  >
                    <Trash2 size={14} />
                    Delete rule
                  </Button>
                </div>
                <div className="grid gap-2 border-t border-border pt-2 md:grid-cols-[150px_220px_1fr_1.25fr_170px_auto]">
                  <Select
                    value={handoffAction}
                    onChange={(event) => setHandoffAction(event.target.value as WorkflowHandoffAction)}
                    aria-label="Workflow handoff action"
                  >
                    <option value="delegate">Delegate</option>
                    <option value="escalate">Escalate</option>
                  </Select>
                  <Select
                    value=""
                    onChange={(event) => {
                      const target = reassignmentTargets.find(
                        (item) => item.id === event.target.value,
                      );
                      if (target) setHandoffTarget(target.value);
                    }}
                    disabled={reassignmentTargets.length === 0}
                    aria-label="Workflow reassignment rule"
                  >
                    <option value="">Target rule</option>
                    {reassignmentTargets.map((target) => (
                      <option key={target.id} value={target.id}>
                        {target.label}
                      </option>
                    ))}
                  </Select>
                  <Input
                    value={handoffTarget}
                    onChange={(event) => setHandoffTarget(event.target.value)}
                    placeholder="Target user or role"
                    aria-label="Workflow handoff target"
                  />
                  <Input
                    value={handoffNote}
                    onChange={(event) => setHandoffNote(event.target.value)}
                    placeholder="Reason or next action"
                    aria-label="Workflow handoff note"
                  />
                  <Input
                    type="date"
                    value={handoffDueDate}
                    onChange={(event) => setHandoffDueDate(event.target.value)}
                    aria-label="Workflow handoff due date"
                  />
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    disabled={handoffBusy || selectedDocuments.length === 0}
                    onClick={() => void recordSelectedWorkflowEvents()}
                  >
                    <AlertTriangle size={14} />
                    {handoffBusy ? "Recording..." : "Record handoff"}
                  </Button>
                </div>
              </div>
            )}
            <table className="w-full text-left text-sm">
              <thead className="border-b border-border bg-surface">
                <tr className="text-xs uppercase text-text-muted">
                  <th className="w-10 px-3 py-2 font-medium">
                    <input
                      type="checkbox"
                      checked={allVisibleSelected}
                      onChange={toggleSelectAllVisible}
                      aria-label="Select all visible workflow documents"
                    />
                  </th>
                  <th className="px-3 py-2 font-medium">Document</th>
                  <th className="px-3 py-2 font-medium">Workflow</th>
                  <th className="px-3 py-2 font-medium">Stage</th>
                  <th className="px-3 py-2 font-medium">Approver</th>
                  <th className="px-3 py-2 font-medium">Handoff</th>
                  <th className="px-3 py-2 font-medium">Age</th>
                  <th className="px-3 py-2 font-medium">Updated</th>
                </tr>
              </thead>
              <tbody>
                {visibleDocuments.map((doc) => {
                  const template = templateById.get(doc.workflow_template_id ?? "");
                  const approvers = getApplicableApprovers(template, doc.workflow_stage_id);
                  const approvalMatch = getApprovalMatch(approvers, user);
                  const attention = getWorkflowAttention(workflowEventsByDocId.get(doc.id) ?? []);
                  return (
                    <tr
                      key={doc.id}
                      tabIndex={0}
                      className="cursor-pointer border-b border-border last:border-b-0 hover:bg-surface-raised focus:bg-surface-raised focus:outline-none"
                      onClick={() => {
                        if (spaceId) navigate(getWorkflowQueueDocumentPath(spaceId, doc.id));
                      }}
                      onKeyDown={(event) => {
                        if (event.key === "Enter" || event.key === " ") {
                          event.preventDefault();
                          if (spaceId) navigate(getWorkflowQueueDocumentPath(spaceId, doc.id));
                        }
                      }}
                    >
                      <td className="px-3 py-2" onClick={(event) => event.stopPropagation()}>
                        <input
                          type="checkbox"
                          checked={selectedIds.includes(doc.id)}
                          onChange={() => toggleSelected(doc.id)}
                          aria-label={`Select ${doc.title}`}
                        />
                      </td>
                      <td className="px-3 py-2">
                        <div className="flex min-w-0 items-center gap-2">
                          <FileText size={14} className="shrink-0 text-text-muted" />
                          <span className="truncate text-text">{doc.title}</span>
                        </div>
                      </td>
                      <td className="px-3 py-2 text-text-muted">
                        {doc.workflow_template_name ?? "Workflow"}
                      </td>
                      <td className="px-3 py-2">
                        <span className="rounded-input bg-surface px-2 py-1 text-xs text-text">
                          {doc.workflow_stage_name ?? doc.workflow_status}
                        </span>
                      </td>
                      <td className="px-3 py-2">
                        <div className="flex items-center gap-1.5 text-xs text-text-muted">
                          {approvalMatch === "role" ? (
                            <Users size={13} />
                          ) : approvalMatch === "user" ? (
                            <UserCheck size={13} />
                          ) : null}
                          <span className="max-w-40 truncate">{formatApprovers(approvers)}</span>
                        </div>
                      </td>
                      <td className="px-3 py-2">
                        <WorkflowAttentionBadge attention={attention} />
                      </td>
                      <td className="px-3 py-2">
                        <span className="inline-flex items-center gap-1 rounded-input bg-surface px-2 py-1 text-xs text-text-muted">
                          <Clock3 size={12} />
                          {formatAge(doc.updated_at)}
                        </span>
                      </td>
                      <td className="px-3 py-2 text-xs text-text-muted">
                        {doc.updated_at ? formatDateTime(doc.updated_at) : "Unknown"}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </main>
    </div>
  );
}

function formatDateTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Unknown";
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

interface WorkflowAttention {
  action: WorkflowHandoffAction | null;
  target: string;
  dueDate: string;
  dueStatus: "none" | "due_soon" | "overdue";
  latestNote: string;
}

function WorkflowAttentionBadge({ attention }: { attention: WorkflowAttention }) {
  if (!attention.action && !attention.dueDate) {
    return <span className="text-xs text-text-muted">None</span>;
  }

  const actionLabel =
    attention.action === "delegate"
      ? "Delegated"
      : attention.action === "escalate"
        ? "Escalated"
        : "Due";
  const dueClass = getWorkflowAttentionDueClass(attention.dueStatus);

  return (
    <div className="flex max-w-56 flex-wrap gap-1 text-xs">
      <span className="rounded-input border border-border bg-surface px-2 py-1 text-text-muted">
        {actionLabel}
        {attention.target ? `: ${attention.target}` : ""}
      </span>
      {attention.dueDate && (
        <span className={`rounded-input border px-2 py-1 ${dueClass}`}>
          Due {attention.dueDate}
        </span>
      )}
    </div>
  );
}

export function compareQueueDocuments(
  a: DocumentRecord,
  b: DocumentRecord,
  sort: QueueSort,
  aEvents: DocumentWorkflowEventRecord[] = [],
  bEvents: DocumentWorkflowEventRecord[] = [],
): number {
  switch (sort) {
    case "updated_asc":
      return getTimestamp(a.updated_at) - getTimestamp(b.updated_at);
    case "title_asc":
      return a.title.localeCompare(b.title);
    case "workflow_asc":
      return `${a.workflow_template_name ?? ""} ${a.workflow_stage_name ?? ""}`.localeCompare(
        `${b.workflow_template_name ?? ""} ${b.workflow_stage_name ?? ""}`,
      );
    case "deadline_asc": {
      const aDeadline = getDueTimestamp(getWorkflowAttention(aEvents).dueDate);
      const bDeadline = getDueTimestamp(getWorkflowAttention(bEvents).dueDate);
      if (aDeadline !== bDeadline) return aDeadline - bDeadline;
      return getTimestamp(b.updated_at) - getTimestamp(a.updated_at);
    }
    case "updated_desc":
    default:
      return getTimestamp(b.updated_at) - getTimestamp(a.updated_at);
  }
}

function getTimestamp(value?: string): number {
  if (!value) return 0;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? 0 : date.getTime();
}

export function matchesQueueAge(doc: DocumentRecord, age: QueueAge): boolean {
  if (age === "all") return true;
  const timestamp = getTimestamp(doc.updated_at);
  if (!timestamp) return false;
  const ageMs = Date.now() - timestamp;
  const dayMs = 24 * 60 * 60 * 1000;
  if (age === "today") return ageMs >= 0 && ageMs < dayMs;
  if (age === "week") return ageMs >= 0 && ageMs < 7 * dayMs;
  return ageMs >= 7 * dayMs;
}

export function getWorkflowAttention(
  events: DocumentWorkflowEventRecord[],
  now = Date.now(),
): WorkflowAttention {
  const latestHandoff = [...events]
    .filter((event) => event.action === "delegate" || event.action === "escalate")
    .sort((a, b) => getTimestamp(b.created_at) - getTimestamp(a.created_at))[0];
  const latestNote = latestHandoff?.note?.trim() ?? "";
  const dueDate = extractWorkflowDueDate(latestNote);
  return {
    action: latestHandoff?.action === "delegate" || latestHandoff?.action === "escalate"
      ? latestHandoff.action
      : null,
    target: extractWorkflowHandoffTarget(latestNote),
    dueDate,
    dueStatus: getDueStatus(dueDate, now),
    latestNote,
  };
}

export function matchesWorkflowFocus(
  events: DocumentWorkflowEventRecord[],
  focus: QueueFocus,
): boolean {
  if (focus === "all") return true;
  const attention = getWorkflowAttention(events);
  if (focus === "delegated") return attention.action === "delegate";
  if (focus === "escalated") return attention.action === "escalate";
  return attention.dueStatus === focus;
}

export function getDeadlineEscalationCandidates(
  documents: DocumentRecord[],
  eventsByDocId: Map<string, DocumentWorkflowEventRecord[]>,
  now = Date.now(),
): DocumentRecord[] {
  return documents.filter((doc) => {
    const attention = getWorkflowAttention(eventsByDocId.get(doc.id) ?? [], now);
    return attention.action === "delegate" && attention.dueStatus === "overdue";
  });
}

export function extractWorkflowHandoffTarget(note: string): string {
  const match = note.match(/^(?:Delegated|Escalated)\s+to\s+([^:]+?)(?:\s*:|$)/i);
  return match?.[1]?.trim() ?? "";
}

export function extractWorkflowDueDate(note: string): string {
  const match = note.match(/\b(?:due|deadline)\s*:?\s*(\d{4}-\d{2}-\d{2})\b/i);
  return match?.[1] ?? "";
}

function getDueStatus(
  dueDate: string,
  now = Date.now(),
): WorkflowAttention["dueStatus"] {
  const dueTimestamp = getDueTimestamp(dueDate);
  if (!Number.isFinite(dueTimestamp)) return "none";
  const dayMs = 24 * 60 * 60 * 1000;
  const todayStart = startOfDay(now);
  if (dueTimestamp < todayStart) return "overdue";
  if (dueTimestamp - todayStart <= 2 * dayMs) return "due_soon";
  return "none";
}

function getDueTimestamp(dueDate: string): number {
  if (!dueDate) return Number.POSITIVE_INFINITY;
  const timestamp = new Date(`${dueDate}T00:00:00.000Z`).getTime();
  return Number.isNaN(timestamp) ? Number.POSITIVE_INFINITY : timestamp;
}

function startOfDay(timestamp: number): number {
  const date = new Date(timestamp);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

export function getApplicableApprovers(
  template: WorkflowTemplateRecord | undefined,
  stageId: string | undefined,
): WorkflowTemplateApproverRecord[] {
  if (!template) return [];
  const scopedApprovers = template.approvers.filter((approver) => approver.stage_id === stageId);
  if (scopedApprovers.length > 0) return scopedApprovers;
  return template.approvers.filter((approver) => !approver.stage_id);
}

export function matchesQueueScope(
  doc: DocumentRecord,
  template: WorkflowTemplateRecord | undefined,
  scope: QueueScope,
  user: { id: string; email: string; role: string } | null,
): boolean {
  const approvers = getApplicableApprovers(template, doc.workflow_stage_id);
  if (scope === "all") return true;
  if (scope === "unassigned") return approvers.length === 0;
  if (!user) return false;
  if (scope === "mine") {
    return approvers.some(
      (approver) =>
        approver.approver_type === "user" &&
        (approver.approver_id === user.id || approver.approver_id === user.email),
    );
  }
  return approvers.some(
    (approver) =>
      approver.approver_type === "role" &&
      (approver.approver_id === user.role || user.role === "admin"),
  );
}

export function getApprovalMatch(
  approvers: WorkflowTemplateApproverRecord[],
  user: { id: string; email: string; role: string } | null,
): "user" | "role" | "none" {
  if (!user) return "none";
  if (
    approvers.some(
      (approver) =>
        approver.approver_type === "user" &&
        (approver.approver_id === user.id || approver.approver_id === user.email),
    )
  ) {
    return "user";
  }
  if (
    approvers.some(
      (approver) =>
        approver.approver_type === "role" &&
        (approver.approver_id === user.role || user.role === "admin"),
    )
  ) {
    return "role";
  }
  return "none";
}

export function formatApprovers(approvers: WorkflowTemplateApproverRecord[]): string {
  if (approvers.length === 0) return "No assigned approver";
  return approvers
    .map((approver) =>
      approver.approver_type === "role" ? `Role: ${approver.approver_id}` : approver.approver_id,
    )
    .join(", ");
}

export function getWorkflowReassignmentTargets(
  documents: DocumentRecord[],
  templateById: Map<string, WorkflowTemplateRecord>,
  eventsByDocId: Map<string, DocumentWorkflowEventRecord[]>,
  user: { id: string; email: string; role: string } | null,
): WorkflowReassignmentTarget[] {
  const targets = new Map<string, WorkflowReassignmentTarget>();
  const addTarget = (
    value: string,
    source: WorkflowReassignmentTargetSource,
    label: string,
  ) => {
    const normalizedValue = value.trim();
    if (!normalizedValue) return;
    const key = `${source}:${normalizedValue.toLowerCase()}`;
    if (!targets.has(key)) {
      targets.set(key, {
        id: key.replace(/[^a-z0-9:._@-]+/gi, "-"),
        label,
        value: normalizedValue,
        source,
      });
    }
  };

  documents.forEach((doc) => {
    const template = templateById.get(doc.workflow_template_id ?? "");
    getApplicableApprovers(template, doc.workflow_stage_id).forEach((approver) => {
      addTarget(
        approver.approver_id,
        "approver",
        approver.approver_type === "role"
          ? `Stage approver role: ${approver.approver_id}`
          : `Stage approver user: ${approver.approver_id}`,
      );
    });
    const attention = getWorkflowAttention(eventsByDocId.get(doc.id) ?? []);
    if (attention.target) {
      addTarget(attention.target, "handoff", `Previous handoff: ${attention.target}`);
    }
  });

  if (user?.email) addTarget(user.email, "current_user", `Current user: ${user.email}`);
  if (user?.id && user.id !== user.email) {
    addTarget(user.id, "current_user", `Current user ID: ${user.id}`);
  }
  addTarget("admin", "admin", "Admin escalation");

  const sourceOrder: Record<WorkflowReassignmentTargetSource, number> = {
    approver: 0,
    handoff: 1,
    current_user: 2,
    admin: 3,
  };

  return [...targets.values()].sort(
    (a, b) => sourceOrder[a.source] - sourceOrder[b.source] || a.label.localeCompare(b.label),
  );
}

export function serializeQueueLinks(documents: DocumentRecord[], spaceId: string): string {
  return documents
    .map((doc) => `${doc.title} - ${getWorkflowQueueDocumentPath(spaceId, doc.id)}`)
    .join("\n");
}

export function createWorkflowQueuePreset(
  name: string,
  filters: Omit<WorkflowQueuePreset, "id" | "name">,
): WorkflowQueuePreset {
  return {
    id: `preset-${Date.now().toString(36)}-${name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`,
    name,
    ...filters,
  };
}

export function createWorkflowReassignmentRule(
  name: string,
  rule: Pick<WorkflowReassignmentRule, "action" | "target" | "note" | "dueDate">,
): WorkflowReassignmentRule {
  return {
    id: `rule-${Date.now().toString(36)}-${name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`,
    name,
    action: rule.action,
    target: rule.target.trim(),
    note: rule.note.trim(),
    dueDate: normalizeWorkflowDueDate(rule.dueDate),
    createdAt: new Date().toISOString(),
  };
}

export function getSharedWorkflowTemplateId(documents: DocumentRecord[]): string | null {
  if (documents.length === 0) return null;
  const [firstTemplateId] = documents.map((doc) => doc.workflow_template_id ?? "");
  if (!firstTemplateId) return null;
  return documents.every((doc) => doc.workflow_template_id === firstTemplateId)
    ? firstTemplateId
    : null;
}

export function getBulkTransitionStages(
  template: WorkflowTemplateRecord | undefined,
  documents: DocumentRecord[],
): WorkflowTemplateRecord["stages"] {
  if (!template || documents.length === 0) return [];
  return template.stages
    .filter((stage) => documents.some((doc) => doc.workflow_stage_id !== stage.id))
    .sort((a, b) => a.sort_order - b.sort_order);
}

export function formatBulkTransitionMessage(
  moved: number,
  failed: number,
  selected: number,
): string {
  if (moved === 0 && failed === 0) return "Selected documents were already at that stage.";
  if (failed === 0) {
    return `Moved ${moved} of ${selected} selected workflow ${
      selected === 1 ? "document" : "documents"
    }.`;
  }
  return `Moved ${moved} of ${selected}; ${failed} ${
    failed === 1 ? "document needs" : "documents need"
  } manual review.`;
}

export function formatWorkflowHandoffNote(
  action: WorkflowHandoffAction,
  target: string,
  note: string,
  dueDate = "",
): string {
  const normalizedTarget = target.trim();
  const normalizedNote = note.trim();
  const normalizedDueDate = normalizeWorkflowDueDate(dueDate);
  if (!normalizedTarget && !normalizedNote && !normalizedDueDate) return "";
  const prefix = action === "delegate" ? "Delegated" : "Escalated";
  const details = [normalizedNote, normalizedDueDate ? `due ${normalizedDueDate}` : ""]
    .filter(Boolean)
    .join("; ");
  if (normalizedTarget && details) return `${prefix} to ${normalizedTarget}: ${details}`;
  if (normalizedTarget) return `${prefix} to ${normalizedTarget}`;
  return `${prefix}: ${details}`;
}

export function formatWorkflowHandoffMessage(
  recorded: number,
  failed: number,
  selected: number,
  action: WorkflowHandoffAction,
): string {
  const label = action === "delegate" ? "delegation" : "escalation";
  if (failed === 0) return `Recorded ${label} on ${recorded} of ${selected} selected documents.`;
  return `Recorded ${label} on ${recorded} of ${selected}; ${failed} ${
    failed === 1 ? "document needs" : "documents need"
  } manual review.`;
}

export function formatDeadlineEscalationMessage(
  recorded: number,
  failed: number,
  selected: number,
): string {
  if (failed === 0) {
    return `Escalated ${recorded} of ${selected} overdue delegated workflow ${
      selected === 1 ? "document" : "documents"
    }.`;
  }
  return `Escalated ${recorded} of ${selected}; ${failed} ${
    failed === 1 ? "document needs" : "documents need"
  } manual review.`;
}

function normalizeWorkflowDueDate(value: string): string {
  const trimmed = value.trim();
  return /^\d{4}-\d{2}-\d{2}$/.test(trimmed) ? trimmed : "";
}

function scopeLabel(scope: QueueScope): string {
  switch (scope) {
    case "mine":
      return "My approvals";
    case "role":
      return "Role approvals";
    case "unassigned":
      return "No assigned approver";
    case "all":
    default:
      return "All approval scopes";
  }
}

function ageLabel(age: QueueAge): string {
  switch (age) {
    case "today":
      return "Updated today";
    case "week":
      return "Updated this week";
    case "stale":
      return "Stale 7+ days";
    case "all":
    default:
      return "Any age";
  }
}

function focusLabel(focus: QueueFocus): string {
  switch (focus) {
    case "delegated":
      return "Delegated";
    case "escalated":
      return "Escalated";
    case "due_soon":
      return "Due soon";
    case "overdue":
      return "Overdue";
    case "all":
    default:
      return "Any handoff";
  }
}

function formatAge(value?: string): string {
  const timestamp = getTimestamp(value);
  if (!timestamp) return "Unknown";
  const ageMs = Math.max(0, Date.now() - timestamp);
  const dayMs = 24 * 60 * 60 * 1000;
  if (ageMs < dayMs) return "Today";
  const days = Math.floor(ageMs / dayMs);
  if (days < 7) return `${days}d`;
  const weeks = Math.floor(days / 7);
  return `${weeks}w`;
}

function readWorkflowQueuePresets(spaceId: string): WorkflowQueuePreset[] {
  const storage = getBrowserLocalStorage();
  if (!storage) return [];
  try {
    const parsed = JSON.parse(storage.getItem(PRESET_STORAGE_KEY) ?? "{}") as Record<
      string,
      WorkflowQueuePreset[]
    >;
    return Array.isArray(parsed[spaceId]) ? parsed[spaceId] : [];
  } catch {
    return [];
  }
}

function writeWorkflowQueuePresets(spaceId: string, presets: WorkflowQueuePreset[]) {
  const storage = getBrowserLocalStorage();
  if (!storage) return;
  try {
    const parsed = JSON.parse(storage.getItem(PRESET_STORAGE_KEY) ?? "{}") as Record<
      string,
      WorkflowQueuePreset[]
    >;
    parsed[spaceId] = presets;
    storage.setItem(PRESET_STORAGE_KEY, JSON.stringify(parsed));
  } catch {
    // Browser storage can be unavailable in locked-down contexts; saved views are optional.
  }
}

export function readWorkflowReassignmentRules(spaceId: string): WorkflowReassignmentRule[] {
  const storage = getBrowserLocalStorage();
  if (!storage) return [];
  try {
    const parsed = JSON.parse(storage.getItem(REASSIGNMENT_RULE_STORAGE_KEY) ?? "{}") as Record<
      string,
      unknown[]
    >;
    if (!Array.isArray(parsed[spaceId])) return [];
    return parsed[spaceId]
      .map(normalizeWorkflowReassignmentRule)
      .filter((rule): rule is WorkflowReassignmentRule => rule !== null);
  } catch {
    return [];
  }
}

export function writeWorkflowReassignmentRules(
  spaceId: string,
  rules: WorkflowReassignmentRule[],
) {
  const storage = getBrowserLocalStorage();
  if (!storage) return;
  try {
    const parsed = JSON.parse(storage.getItem(REASSIGNMENT_RULE_STORAGE_KEY) ?? "{}") as Record<
      string,
      WorkflowReassignmentRule[]
    >;
    parsed[spaceId] = rules;
    storage.setItem(REASSIGNMENT_RULE_STORAGE_KEY, JSON.stringify(parsed));
  } catch {
    // Browser storage can be unavailable in locked-down contexts; saved rules are optional.
  }
}

function normalizeWorkflowReassignmentRule(value: unknown): WorkflowReassignmentRule | null {
  if (!value || typeof value !== "object") return null;
  const rule = value as Partial<WorkflowReassignmentRule>;
  if (
    typeof rule.id !== "string" ||
    typeof rule.name !== "string" ||
    (rule.action !== "delegate" && rule.action !== "escalate")
  ) {
    return null;
  }
  return {
    id: rule.id,
    name: rule.name,
    action: rule.action,
    target: typeof rule.target === "string" ? rule.target : "",
    note: typeof rule.note === "string" ? rule.note : "",
    dueDate: typeof rule.dueDate === "string" ? normalizeWorkflowDueDate(rule.dueDate) : "",
    createdAt: typeof rule.createdAt === "string" ? rule.createdAt : "",
  };
}

function getBrowserLocalStorage(): Storage | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage;
}
