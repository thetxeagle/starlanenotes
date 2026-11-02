import { ApplicationRestoreHistorySummary } from "./ApplicationRestoreHistorySummary";
import { StorageIntegrityPanel } from "./StorageIntegrityPanel";
import { useEffect, useState, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import {
  readStorageHealthObservation,
  storageQuantity,
  storageStatus,
  type StorageHealthReport,
  type StorageHealthObservation,
} from "@/lib/adminStorageHealth";

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-1 py-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,2fr)]">
      <dt className="text-text-muted">{label}</dt>
      <dd className="text-text min-w-0 break-words">{children}</dd>
    </div>
  );
}
function Recovery({ status, completed_at }: StorageHealthReport["backup"]) {
  return (
    <>
      {storageStatus(status)}
      {status === "recorded_completed" && completed_at && (
        <time className="text-text-muted block" dateTime={completed_at}>
          {new Date(completed_at).toLocaleString()}
        </time>
      )}
    </>
  );
}
export function StorageHealthPanel() {
  const [revision, setRevision] = useState(0);
  const [observation, setObservation] = useState<StorageHealthObservation | null>(null);
  const [pending, setPending] = useState(true);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    const controller = new AbortController();
    const timer = window.setTimeout(() => controller.abort(), 10000);
    let active = true;
    setObservation(null);
    setPending(true);
    setFailed(false);
    void readStorageHealthObservation(controller.signal)
      .then((value) => {
        if (active && value.isCurrent()) setObservation(value);
      })
      .catch(() => {
        if (active) setFailed(true);
      })
      .finally(() => {
        window.clearTimeout(timer);
        if (active) setPending(false);
      });
    return () => {
      active = false;
      controller.abort();
      window.clearTimeout(timer);
    };
  }, [revision]);
  useEffect(() => {
    if (!observation) return;
    const timer = window.setInterval(() => {
      if (!observation.isCurrent()) {
        setObservation(null);
        setFailed(true);
      }
    }, 500);
    return () => window.clearInterval(timer);
  }, [observation]);
  // Match the existing integrity panel's context lifetime. A render after a
  // credential/origin change cannot display an earlier observation's quantities.
  const report = observation?.isCurrent() ? observation.report : null;
  const knownUsage = report?.attachment_capacity?.readonly_known_usage;
  return (
    <>
      <section
        aria-labelledby="server-storage-health-title"
        aria-busy={pending}
        className="rounded-card border-border border p-3"
      >
        <div className="flex items-start justify-between gap-3">
          <div>
            <h2 id="server-storage-health-title" className="text-text text-sm font-medium">
              Server storage status
            </h2>
            <p className="text-text-muted mt-1 text-xs">
              Database availability and file access are separate from verified recovery.
            </p>
          </div>
          <Button
            size="sm"
            variant="secondary"
            disabled={pending}
            onClick={() => setRevision((value) => value + 1)}
          >
            Refresh status
          </Button>
        </div>
        {pending && (
          <p role="status" className="text-text-muted mt-3 text-sm">
            Checking server storage…
          </p>
        )}
        {failed && (
          <p role="alert" className="text-danger mt-3 text-sm">
            Storage status is unavailable. Check your connection and sign-in, then retry.
          </p>
        )}
        {report && (
          <>
            <dl className="divide-border mt-3 divide-y text-sm">
              <Row label="Database">
                {report.database.engine === "sqlite"
                  ? "SQLite"
                  : report.database.engine === "postgresql"
                    ? "PostgreSQL"
                    : "Unknown"}{" "}
                · {storageStatus(report.database.connection_status)}
              </Row>
              <Row label="Migration history">
                {storageStatus(report.database.migration_history.status)}
                {report.database.migration_history.applied_count !== null &&
                  ` · ${report.database.migration_history.applied_count} / ${report.database.migration_history.expected_count} records`}
              </Row>
              <Row label="File storage">
                {report.files.backend === "local" ? "Local filesystem" : "Unknown"} ·{" "}
                {storageStatus(report.files.read_status)}
              </Row>
              <Row label="Server file location">
                {report.files.location.includes("://")
                  ? "Unavailable"
                  : report.files.location || "Unavailable"}
              </Row>
              <Row label="File integrity">{storageStatus(report.files.integrity_status)}</Row>
              <Row label="Recorded attachments">
                {report.recorded_attachment_usage.status === "available"
                  ? `${storageQuantity(report.recorded_attachment_usage.attachments)} attachments · ${storageQuantity(report.recorded_attachment_usage.bytes, true)}`
                  : "Unavailable"}
              </Row>
              <Row label="Upload limit">
                {storageQuantity(report.limits.max_upload_bytes, true)}
              </Row>
              <Row label="Storage quota">
                {report.attachment_capacity ? (
                  <>
                    {storageStatus(report.attachment_capacity.quota_status)}
                    {report.attachment_capacity.quota_status === "configured" &&
                      ` · ${storageQuantity(report.attachment_capacity.quota_bytes, true)}`}
                  </>
                ) : (
                  storageStatus(report.limits.quota_status)
                )}
              </Row>
              {report.attachment_capacity && (
                <>
                  <Row label="Logical retained assets">
                    {storageQuantity(report.attachment_capacity.logical_retained_bytes, true)}
                  </Row>
                  <Row label="Charged retained bytes">
                    {storageQuantity(report.attachment_capacity.charged_retained_bytes, true)}
                  </Row>
                  <Row label="Pending reservation maxima">
                    {storageQuantity(report.attachment_capacity.reserved_maximum_bytes, true)}
                  </Row>
                  <Row label="Total attachment charge">
                    {storageQuantity(report.attachment_capacity.charged_total_bytes, true)}
                  </Row>
                  {knownUsage && (
                    <>
                      <Row label="Known recovery usage">
                        {knownUsage.status === "observed" ? "Observed" : "Unavailable"}
                        {knownUsage.observed_at && (
                          <time className="text-text-muted block" dateTime={knownUsage.observed_at}>
                            {new Date(knownUsage.observed_at).toLocaleString()}
                          </time>
                        )}
                        <p className="text-text-muted mt-1 text-xs">
                          Logical retained assets plus original pending reservations. Complete
                          reference coverage is unknown. This observation cannot authorize new
                          uploads or establish disk usage or free quota.
                        </p>
                      </Row>
                      <Row label="Recovery logical floor">
                        {storageQuantity(knownUsage.logical_retained_floor_bytes, true)}
                      </Row>
                      <Row label="Original pending maximum">
                        {storageQuantity(knownUsage.original_pending_maximum_bytes, true)}
                      </Row>
                      <Row label="Known recovery sum">
                        {storageQuantity(knownUsage.conservative_known_sum_bytes, true)}
                      </Row>
                    </>
                  )}
                </>
              )}
              <Row label="Last byte backup">
                <Recovery {...report.backup} />
              </Row>
              <Row label="Last verified application restore">
                <Recovery {...report.verified_restore} />
              </Row>
              <Row label="Last isolated application test">
                <ApplicationRestoreHistorySummary value={report.application_restore_history} />
              </Row>
            </dl>
            <p className="text-text-muted mt-3 text-xs">
              Recorded attachments count live database records. Attachment charge includes validated
              retained copies and a conservative logical floor; pending reservations use their
              maximum bytes. Charge is separate from raw disk usage and free space. Unavailable
              accounting does not prove absence or release retained bytes. Matching migration
              history does not verify every table or file. Unknown recovery means no confirmed
              result is available.
            </p>
            {report.database.migration_history.status === "mismatch" && (
              <p className="text-warning mt-2 text-sm">
                Migration history differs. Check the installed server version and migration records
                before changing storage.
              </p>
            )}
            {report.files.read_status === "unavailable" && (
              <p className="text-warning mt-2 text-sm">
                Server files could not be read. Check the configured directory and filesystem
                permissions.
              </p>
            )}
            <p className="text-text-muted mt-2 text-xs">
              Observed{" "}
              <time dateTime={report.observed_at}>
                {new Date(report.observed_at).toLocaleString()}
              </time>
            </p>
          </>
        )}
      </section>
      <StorageIntegrityPanel />
    </>
  );
}
