export type IncidentFieldValues = {
  service: string;
  severity: string;
  date: string;
  owner: string;
};
export type IncidentSection = "summary" | "impact" | "timeline" | "recovery" | "followup";
export type IncidentResponses = Record<IncidentSection, string>;
export type TemplateRevision = "published" | "draft";

export const INCIDENT_SECTIONS: readonly {
  id: IncidentSection;
  title: string;
  guidance: string;
}[] = [
  { id: "summary", title: "Summary", guidance: "Describe the user impact and current status." },
  {
    id: "impact",
    title: "Impact",
    guidance: "Include affected services and approximate duration.",
  },
  {
    id: "timeline",
    title: "Timeline",
    guidance: "Add timestamped events as the incident develops.",
  },
  {
    id: "recovery",
    title: "Recovery",
    guidance: "Record the mitigation and how recovery was confirmed.",
  },
  {
    id: "followup",
    title: "Follow-up actions",
    guidance: "List owners and due dates for follow-up actions.",
  },
];

export const INCIDENT_FIXTURES = [
  {
    id: "incident-report",
    name: "Incident Report",
    description: "A consistent record for service incidents, recovery, and follow-up.",
    location: "Engineering · Operations",
    revision: "Published revision 4",
    tags: ["incident", "operations"],
    properties: { severity: "SEV-2", service: "Payments API", owner: "Maya Chen" },
  },
] as const;

export const EMPTY_RESPONSES: IncidentResponses = {
  summary: "",
  impact: "",
  timeline: "",
  recovery: "",
  followup: "",
};

export function formatIncidentTitle(fields: IncidentFieldValues) {
  return `Incident — ${fields.service || "[Service]"} — ${fields.date || "[Incident date]"}`;
}

export function incidentProperties(fields: IncidentFieldValues) {
  return {
    Service: fields.service,
    Severity: fields.severity,
    "Incident date": fields.date,
    Owner: fields.owner,
  };
}

export function guidanceForRevision(id: IncidentSection, revision: TemplateRevision) {
  if (revision === "draft" && id === "impact") {
    return "Draft revision guidance: Separate customer impact from internal recovery metrics.";
  }
  if (revision === "draft" && id === "followup") {
    return "Draft revision guidance: Add a named owner and a target date for each action.";
  }
  return INCIDENT_SECTIONS.find((section) => section.id === id)?.guidance ?? "";
}

/** Build a structured fixture result. Guidance stays separate from authored values. */
export function buildIncidentOutput(fields: IncidentFieldValues, responses: IncidentResponses) {
  return {
    title: formatIncidentTitle(fields),
    properties: incidentProperties(fields),
    summaryFields: fields,
    sections: INCIDENT_SECTIONS.map(({ id, title }) => ({ id, title, text: responses[id] })),
  };
}
