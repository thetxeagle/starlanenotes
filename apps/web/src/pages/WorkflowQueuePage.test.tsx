import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import {
  compareQueueDocuments,
  canSaveWorkflowNamingValue,
  createWorkflowReassignmentRule,
  createWorkflowQueuePreset,
  extractWorkflowDueDate,
  extractWorkflowHandoffTarget,
  formatBulkTransitionMessage,
  formatDeadlineEscalationMessage,
  formatWorkflowHandoffMessage,
  formatWorkflowHandoffNote,
  formatApprovers,
  getBulkTransitionStages,
  getApplicableApprovers,
  getApprovalMatch,
  getDeadlineEscalationCandidates,
  getSharedWorkflowTemplateId,
  getWorkflowAttention,
  getWorkflowAttentionDueClass,
  getWorkflowNamingDialogDescription,
  getWorkflowNamingDialogLabel,
  getWorkflowNamingDialogTitle,
  getWorkflowReassignmentTargets,
  getWorkflowQueueDocumentPath,
  matchesWorkflowFocus,
  matchesQueueAge,
  matchesQueueScope,
  readWorkflowReassignmentRules,
  serializeQueueLinks,
  writeWorkflowReassignmentRules,
  WORKFLOW_QUEUE_ERROR_CLASS,
  WorkflowQueuePage,
} from "./WorkflowQueuePage";
import type {
  DocumentRecord,
  DocumentWorkflowEventRecord,
  WorkflowTemplateRecord,
} from "@/lib/dataClient";

const template: WorkflowTemplateRecord = {
  id: "template-1",
  space_id: "space-1",
  name: "Publishing",
  stages: [
    { id: "draft", name: "Draft", sort_order: 0 },
    { id: "review", name: "Review", sort_order: 1 },
  ],
  approvers: [
    { id: "approver-owner", approver_type: "role", approver_id: "owner", stage_id: "review" },
    { id: "approver-admin", approver_type: "role", approver_id: "admin" },
    { id: "approver-user", approver_type: "user", approver_id: "editor@example.com" },
  ],
  created_at: "2026-01-01T00:00:00.000Z",
  updated_at: "2026-01-01T00:00:00.000Z",
};

const reviewDocument: DocumentRecord = {
  id: "doc-1",
  title: "Review me",
  workflow_template_id: "template-1",
  workflow_template_name: "Publishing",
  workflow_stage_id: "review",
  workflow_stage_name: "Review",
  workflow_status: "active",
  updated_at: "2026-01-02T00:00:00.000Z",
};

vi.mock("@/hooks/useAuth", () => ({
  useAuth: () => ({
    user: {
      id: "user-1",
      email: "owner@example.com",
      display_name: "Owner",
      role: "owner",
    },
  }),
}));

describe("WorkflowQueuePage queue helpers", () => {
  beforeEach(() => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      clear: () => store.clear(),
      getItem: (key: string) => store.get(key) ?? null,
      removeItem: (key: string) => store.delete(key),
      setItem: (key: string, value: string) => store.set(key, value),
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    localStorage.clear();
    vi.unstubAllGlobals();
  });

  it("builds encoded document routes for workflow queue rows", () => {
    expect(getWorkflowQueueDocumentPath("space-1", "doc-1")).toBe(
      "/spaces/space-1/docs/doc-1",
    );
    expect(getWorkflowQueueDocumentPath("space/one", "doc/one")).toBe(
      "/spaces/space%2Fone/docs/doc%2Fone",
    );
  });

  it("uses stage-scoped approvers before template-wide approvers", () => {
    const approvers = getApplicableApprovers(template, "review");

    expect(approvers).toHaveLength(1);
    expect(approvers[0].approver_id).toBe("owner");
  });

  it("renders workflow queue header filters with shared form controls", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter initialEntries={["/spaces/space-1/workflows"]}>
        <Routes>
          <Route path="/spaces/:spaceId/workflows" element={<WorkflowQueuePage />} />
        </Routes>
      </MemoryRouter>,
    );

    expect(html).toContain('aria-label="Search workflow queue"');
    expect(html).toContain('aria-label="Workflow status"');
    expect(html).toContain('aria-label="Sort workflow queue"');
    expect(html).toContain("focus:ring-2");
    expect(html).not.toContain("Search size");
  });

  it("falls back to global approvers when a stage has no scoped approvers", () => {
    const approvers = getApplicableApprovers(template, "draft");

    expect(approvers.map((approver) => approver.approver_id)).toEqual([
      "admin",
      "editor@example.com",
    ]);
  });

  it("matches role, admin, user, and unassigned approval scopes", () => {
    expect(
      matchesQueueScope(reviewDocument, template, "role", {
        id: "user-1",
        email: "owner@example.com",
        role: "owner",
      }),
    ).toBe(true);
    expect(
      matchesQueueScope(reviewDocument, template, "role", {
        id: "user-2",
        email: "admin@example.com",
        role: "admin",
      }),
    ).toBe(true);
    expect(
      matchesQueueScope(
        { ...reviewDocument, workflow_stage_id: "draft" },
        template,
        "mine",
        {
          id: "user-3",
          email: "editor@example.com",
          role: "member",
        },
      ),
    ).toBe(true);
    expect(matchesQueueScope(reviewDocument, { ...template, approvers: [] }, "unassigned", null))
      .toBe(true);
  });

  it("labels the current user's approval match", () => {
    expect(
      getApprovalMatch(getApplicableApprovers(template, "review"), {
        id: "user-1",
        email: "owner@example.com",
        role: "owner",
      }),
    ).toBe("role");
    expect(
      getApprovalMatch(getApplicableApprovers(template, "draft"), {
        id: "user-2",
        email: "editor@example.com",
        role: "member",
      }),
    ).toBe("user");
  });

  it("formats approvers and sorts queue rows", () => {
    expect(formatApprovers(getApplicableApprovers(template, "review"))).toBe("Role: owner");
    expect(formatApprovers([])).toBe("No assigned approver");

    const older: DocumentRecord = {
      ...reviewDocument,
      id: "doc-older",
      title: "A older",
      updated_at: "2026-01-01T00:00:00.000Z",
    };
    expect(compareQueueDocuments(reviewDocument, older, "updated_desc")).toBeLessThan(0);
    expect(compareQueueDocuments(reviewDocument, older, "title_asc")).toBeGreaterThan(0);
  });

  it("filters queue rows by updated age bucket", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-10T12:00:00.000Z"));

    const today = { ...reviewDocument, updated_at: "2026-01-10T08:00:00.000Z" };
    const thisWeek = { ...reviewDocument, updated_at: "2026-01-06T08:00:00.000Z" };
    const stale = { ...reviewDocument, updated_at: "2026-01-01T08:00:00.000Z" };

    expect(matchesQueueAge(today, "today")).toBe(true);
    expect(matchesQueueAge(thisWeek, "week")).toBe(true);
    expect(matchesQueueAge(stale, "stale")).toBe(true);
    expect(matchesQueueAge(stale, "today")).toBe(false);
  });

  it("serializes selected queue links", () => {
    expect(serializeQueueLinks([reviewDocument], "space-1")).toBe(
      "Review me - /spaces/space-1/docs/doc-1",
    );
    expect(serializeQueueLinks([{ ...reviewDocument, id: "doc/one" }], "space/one")).toBe(
      "Review me - /spaces/space%2Fone/docs/doc%2Fone",
    );
  });

  it("creates saved queue presets from current filters", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-10T12:00:00.000Z"));

    const preset = createWorkflowQueuePreset("My Reviews", {
      status: "active",
      templateId: "template-1",
      stageId: "review",
      scope: "mine",
      query: "policy",
      sort: "updated_desc",
      age: "stale",
      focus: "delegated",
    });

    expect(preset.name).toBe("My Reviews");
    expect(preset.scope).toBe("mine");
    expect(preset.focus).toBe("delegated");
    expect(preset.id).toContain("my-reviews");
  });

  it("labels workflow naming dialogs without browser-native prompts", () => {
    expect(getWorkflowNamingDialogTitle("queue")).toBe("Save queue view");
    expect(getWorkflowNamingDialogLabel("queue")).toBe("Queue view name");
    expect(getWorkflowNamingDialogDescription("queue")).toContain("current workflow filters");

    expect(getWorkflowNamingDialogTitle("rule")).toBe("Save reassignment rule");
    expect(getWorkflowNamingDialogLabel("rule")).toBe("Reassignment rule name");
    expect(getWorkflowNamingDialogDescription("rule")).toContain("delegation or escalation");

    expect(canSaveWorkflowNamingValue("   ")).toBe(false);
    expect(canSaveWorkflowNamingValue("Escalate to admins")).toBe(true);
  });

  it("detects whether selected rows share a workflow template", () => {
    expect(getSharedWorkflowTemplateId([reviewDocument, { ...reviewDocument, id: "doc-2" }])).toBe(
      "template-1",
    );
    expect(
      getSharedWorkflowTemplateId([
        reviewDocument,
        { ...reviewDocument, id: "doc-3", workflow_template_id: "template-2" },
      ]),
    ).toBeNull();
    expect(getSharedWorkflowTemplateId([])).toBeNull();
  });

  it("offers bulk transition stages that would move at least one selected row", () => {
    const stages = getBulkTransitionStages(template, [
      reviewDocument,
      { ...reviewDocument, id: "doc-2", workflow_stage_id: "draft" },
    ]);

    expect(stages.map((stage) => stage.id)).toEqual(["draft", "review"]);
    expect(getBulkTransitionStages(template, [reviewDocument]).map((stage) => stage.id)).toEqual([
      "draft",
    ]);
    expect(getBulkTransitionStages(undefined, [reviewDocument])).toEqual([]);
  });

  it("formats bulk transition results", () => {
    expect(formatBulkTransitionMessage(0, 0, 2)).toBe(
      "Selected documents were already at that stage.",
    );
    expect(formatBulkTransitionMessage(2, 0, 2)).toBe("Moved 2 of 2 selected workflow documents.");
    expect(formatBulkTransitionMessage(1, 1, 2)).toBe(
      "Moved 1 of 2; 1 document needs manual review.",
    );
  });

  it("formats workflow delegation and escalation records", () => {
    expect(formatWorkflowHandoffNote("delegate", "owner", "Please review")).toBe(
      "Delegated to owner: Please review",
    );
    expect(formatWorkflowHandoffNote("delegate", "owner", "Please review", "2026-01-12")).toBe(
      "Delegated to owner: Please review; due 2026-01-12",
    );
    expect(formatWorkflowHandoffNote("delegate", "owner", "", "2026-01-12")).toBe(
      "Delegated to owner: due 2026-01-12",
    );
    expect(formatWorkflowHandoffNote("escalate", "admin", "")).toBe("Escalated to admin");
    expect(formatWorkflowHandoffNote("delegate", "", "")).toBe("");
    expect(formatWorkflowHandoffMessage(2, 0, 2, "delegate")).toBe(
      "Recorded delegation on 2 of 2 selected documents.",
    );
    expect(formatWorkflowHandoffMessage(1, 1, 2, "escalate")).toBe(
      "Recorded escalation on 1 of 2; 1 document needs manual review.",
    );
    expect(formatDeadlineEscalationMessage(2, 0, 2)).toBe(
      "Escalated 2 of 2 overdue delegated workflow documents.",
    );
    expect(formatDeadlineEscalationMessage(1, 1, 2)).toBe(
      "Escalated 1 of 2; 1 document needs manual review.",
    );
  });

  it("builds structured reassignment target rules from selected workflow rows", () => {
    const priorHandoff: DocumentWorkflowEventRecord = {
      id: "event-handoff",
      document_workflow_id: "workflow-1",
      document_id: "doc-1",
      from_stage_id: undefined,
      to_stage_id: "review",
      action: "delegate",
      note: "Delegated to editor@example.com: due 2026-01-12",
      created_by: "owner",
      created_at: "2026-01-10T10:00:00.000Z",
    };

    const targets = getWorkflowReassignmentTargets(
      [reviewDocument],
      new Map([["template-1", template]]),
      new Map([["doc-1", [priorHandoff]]]),
      { id: "user-1", email: "owner@example.com", role: "owner" },
    );

    expect(targets.map((target) => target.label)).toEqual([
      "Stage approver role: owner",
      "Previous handoff: editor@example.com",
      "Current user ID: user-1",
      "Current user: owner@example.com",
      "Admin escalation",
    ]);
    expect(targets.map((target) => target.value)).toContain("owner");
    expect(targets.map((target) => target.value)).toContain("editor@example.com");
  });

  it("persists reusable workflow reassignment rules by space", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-10T12:00:00.000Z"));

    const rule = createWorkflowReassignmentRule("Escalate to admins", {
      action: "escalate",
      target: "admin",
      note: "Deadline missed",
      dueDate: "2026-01-12",
    });

    writeWorkflowReassignmentRules("space-1", [rule]);

    expect(readWorkflowReassignmentRules("space-1")).toEqual([rule]);
    expect(readWorkflowReassignmentRules("space-2")).toEqual([]);
  });

  it("normalizes stored workflow reassignment rules", () => {
    localStorage.setItem(
      "starlane.workflowQueue.reassignmentRules.v1",
      JSON.stringify({
        "space-1": [
          {
            id: "rule-1",
            name: "Delegate to editor",
            action: "delegate",
            target: "editor@example.com",
            note: "Please review",
            dueDate: "invalid",
          },
          { id: "broken", name: "Broken", action: "unknown" },
        ],
      }),
    );

    expect(readWorkflowReassignmentRules("space-1")).toEqual([
      {
        id: "rule-1",
        name: "Delegate to editor",
        action: "delegate",
        target: "editor@example.com",
        note: "Please review",
        dueDate: "",
        createdAt: "",
      },
    ]);
  });

  it("extracts workflow handoff targets and due dates", () => {
    expect(
      extractWorkflowHandoffTarget("Delegated to editor@example.com: due 2026-01-12"),
    ).toBe("editor@example.com");
    expect(extractWorkflowHandoffTarget("Escalated to owner")).toBe("owner");
    expect(extractWorkflowDueDate("Escalated to owner: deadline 2026-01-12")).toBe(
      "2026-01-12",
    );
  });

  it("filters and sorts queue rows by handoff urgency", () => {
    const delegatedEvent: DocumentWorkflowEventRecord = {
      id: "event-1",
      document_workflow_id: "workflow-1",
      document_id: "doc-1",
      from_stage_id: undefined,
      to_stage_id: "review",
      action: "delegate",
      note: "Delegated to editor@example.com: due 2026-01-12",
      created_by: "owner",
      created_at: "2026-01-10T10:00:00.000Z",
    };
    const escalatedEvent: DocumentWorkflowEventRecord = {
      ...delegatedEvent,
      id: "event-2",
      document_id: "doc-2",
      action: "escalate",
      note: "Escalated to admin: deadline 2026-01-09",
      created_at: "2026-01-11T10:00:00.000Z",
    };

    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-10T12:00:00.000Z"));

    expect(getWorkflowAttention([delegatedEvent]).target).toBe("editor@example.com");
    expect(getWorkflowAttention([delegatedEvent]).dueStatus).toBe("due_soon");
    expect(getWorkflowAttention([escalatedEvent]).dueStatus).toBe("overdue");
    expect(matchesWorkflowFocus([delegatedEvent], "delegated")).toBe(true);
    expect(matchesWorkflowFocus([escalatedEvent], "overdue")).toBe(true);
    expect(
      compareQueueDocuments(
        reviewDocument,
        { ...reviewDocument, id: "doc-2", updated_at: "2026-01-03T00:00:00.000Z" },
        "deadline_asc",
        [delegatedEvent],
        [escalatedEvent],
      ),
    ).toBeGreaterThan(0);
    expect(
      getDeadlineEscalationCandidates(
        [reviewDocument, { ...reviewDocument, id: "doc-2" }],
        new Map([
          ["doc-1", [delegatedEvent]],
          ["doc-2", [escalatedEvent]],
        ]),
      ).map((doc) => doc.id),
    ).toEqual([]);

    expect(getWorkflowAttentionDueClass("overdue")).toContain("text-danger");
    expect(getWorkflowAttentionDueClass("overdue")).not.toMatch(/red-|amber-/);
    expect(getWorkflowAttentionDueClass("due_soon")).toContain("text-warning");
    expect(getWorkflowAttentionDueClass("due_soon")).not.toMatch(/red-|amber-/);

    const overdueDelegation = {
      ...delegatedEvent,
      id: "event-3",
      note: "Delegated to owner: due 2026-01-09",
    };
    expect(
      getDeadlineEscalationCandidates(
        [reviewDocument],
        new Map([["doc-1", [overdueDelegation]]]),
      ).map((doc) => doc.id),
    ).toEqual(["doc-1"]);
  });

  it("uses semantic danger tokens for queue errors", () => {
    expect(WORKFLOW_QUEUE_ERROR_CLASS).toContain("border-danger");
    expect(WORKFLOW_QUEUE_ERROR_CLASS).toContain("bg-danger");
    expect(WORKFLOW_QUEUE_ERROR_CLASS).toContain("text-danger");
    expect(WORKFLOW_QUEUE_ERROR_CLASS).not.toMatch(/red-/);
  });
});
