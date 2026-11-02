import { useEffect, useState } from "react";
import { capturePageMetadataLoadGuard } from "@/lib/pageMetadataLoad";
import { dataClient } from "@/lib/dataClient";

/** Current metadata only. A responsible person/team supplies no access or draft authority. */
export function PageResponsibilitySummary({
  spaceId,
  docId,
  refreshKey,
}: {
  spaceId: string;
  docId: string;
  refreshKey: boolean;
}) {
  const [summary, setSummary] = useState<{ person: string; workflow?: string } | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  useEffect(() => {
    let current = true;
    const requireSameSession = capturePageMetadataLoadGuard();
    setSummary(null);
    setUnavailable(false);
    // A catalog failure must not hide the distinction between an assignment
    // whose label is unavailable and a page known to be unassigned.
    void Promise.all([
      dataClient.getDocumentMaintenanceTargets(spaceId, docId).catch(() => null),
      dataClient.getDocumentWorkflow(spaceId, docId).catch(() => "unavailable" as const),
    ])
      .then(async ([targets, workflow]) => {
        const page = await dataClient.getDocument(spaceId, docId);
        requireSameSession();
        if (!current) return;
        const person = page.maintenance_user_id
          ? (targets?.users.find((user) => user.id === page.maintenance_user_id)?.name ??
            "Person unavailable")
          : page.maintenance_group_id
            ? (targets?.groups.find((group) => group.id === page.maintenance_group_id)?.name ??
              "Team unavailable")
            : "Unassigned";
        setSummary({
          person,
          workflow:
            workflow === "unavailable"
              ? "Unavailable"
              : workflow?.current_stage_name || workflow?.status || undefined,
        });
      })
      .catch(() => {
        if (current) {
          setSummary(null);
          setUnavailable(true);
        }
      });
    return () => {
      current = false;
    };
  }, [spaceId, docId, refreshKey]);
  return (
    <span className="text-text-muted text-xs break-words">
      Responsible: {summary?.person ?? (unavailable ? "Unavailable" : "Loading…")}
      {summary?.workflow && <> · Workflow: {summary.workflow}</>}
    </span>
  );
}
