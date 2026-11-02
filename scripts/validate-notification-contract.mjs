import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = process.cwd();
const read = (path) => readFileSync(resolve(root, path), "utf8");
const failures = [];

function requireText(path, pattern, message) {
  if (!pattern.test(read(path))) failures.push(`${path}: ${message}`);
}

const notifications = read("server/src/notifications/mod.rs");
const emittedTypes = [
  "comment",
  "comment_reply",
  "workflow_approval",
  "workflow_delegate",
  "workflow_escalate",
];

for (const notificationType of new Set(emittedTypes)) {
  if (!notifications.includes(`("${notificationType}",`)) {
    failures.push(`server notification type '${notificationType}' is emitted but has no inbox preference.`);
  }
}

requireText("server/src/notifications/mod.rs", /\/notifications\/system/, "missing authenticated system-activity route.");
requireText("apps/web/src/components/NotificationBell.tsx", /SYSTEM_NOTIFICATION_EVENT/, "does not present immediately recorded system activity.");
requireText("apps/web/src/components/NotificationBell.tsx", /setTimeout\(dismissBanner, 6000\)/, "banner must auto-dismiss without changing inbox state.");
requireText("apps/web/src/components/NotificationBell.tsx", /getNextNotificationBanner/, "banner queue advancement is not isolated from durable inbox state.");
requireText("apps/web/src/components/NotificationBell.tsx", /getNotificationsAfterSystemEvent/, "immediate system activity can duplicate inbox records.");
requireText("apps/web/src/components/NotificationBell.test.ts", /dismisses only the presentation banner/, "missing regression coverage for durable inbox records after banner dismissal.");
requireText("apps/web/src/components/NotificationBell.test.ts", /without duplicating it/, "missing regression coverage for immediate notification deduplication.");
requireText("apps/web/src/components/NotificationBell.tsx", /md:left-auto md:right-4/, "desktop banner must be top-right while mobile remains top-safe-area.");
requireText("apps/web/src/components/NotificationBell.tsx", /<Popover\s+modal/, "notification center must contain focus and restore its bell trigger.");
requireText("apps/web/src/components/layout/TopBar.tsx", /<NotificationBell\s*\/>/, "desktop app header is missing a notification bell beside search.");
requireText("apps/web/src/components/layout/TopBar.tsx", /<MobileWorkspaceHeader/, "responsive mobile web must use the shared mobile header.");
requireText("apps/web/src/components/layout/MobileAppLayout.tsx", /<MobileWorkspaceHeader/, "native mobile must use the shared mobile header.");
requireText("apps/web/src/components/layout/MobileWorkspaceHeader.tsx", /<NotificationBell\s+touchTarget\s*\/>[\s\S]{0,500}aria-label="Open search"/, "shared mobile header must keep the touch-sized notification bell adjacent to search.");
requireText("apps/desktop/src-tauri/src/lib.rs", /native_notifications::send_native_notification/, "mobile shell does not register native notification IPC.");
requireText("apps/desktop/src-tauri/src/desktop.rs", /native_notifications::send_native_notification/, "desktop shell does not register native notification IPC.");
requireText("public-docs/api/notifications.md", /Record System Activity/, "public notification API documentation omits system activity.");
requireText("internal-docs/api/notifications.md", /Record System Activity/, "internal notification API documentation omits system activity.");

if (failures.length > 0) {
  console.error("Notification contract validation failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("Notification contract validation passed.");
