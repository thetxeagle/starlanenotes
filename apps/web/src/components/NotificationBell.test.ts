import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  createNotificationFetchFailureState,
  getNotificationBadgeLabel,
  getNotificationBellButtonClass,
  getNotificationBellIconSize,
  getNotificationBannerDismissLabel,
  getNotificationBannerQueueAfterDismiss,
  getNotificationBannerQueueAfterEnqueue,
  getNotificationBannerClass,
  getNotificationCenterActionClass,
  getNotificationCenterCloseClass,
  getNotificationCenterListClass,
  getNotificationCenterPanelClass,
  getNextNotificationBanner,
  getNotificationsAfterMarkAllRead,
  getNotificationsAfterSystemEvent,
  getUnreadNotificationCount,
  isCurrentNotificationRequest,
  NotificationBanner,
  NotificationFetchError,
  settleMarkAllNotificationsRead,
} from "./NotificationBell";
import type { NotificationRecord } from "@/lib/dataClient";

function notification(id: string, read: boolean): NotificationRecord {
  return {
    id,
    notification_type: "comment",
    title: `Notification ${id}`,
    read,
    created_at: "2026-07-14T00:00:00Z",
  };
}

describe("NotificationBell state helpers", () => {
  it("accepts only the latest notification request completion", () => {
    expect(isCurrentNotificationRequest(4, 4)).toBe(true);
    expect(isCurrentNotificationRequest(3, 4)).toBe(false);
  });

  it("counts only unread notifications for the bell badge", () => {
    expect(
      getUnreadNotificationCount([
        notification("read", true),
        notification("unread-1", false),
        notification("unread-2", false),
      ]),
    ).toBe(2);
  });

  it("caps the visible notification bell badge label", () => {
    expect(getNotificationBadgeLabel(1)).toBe("1");
    expect(getNotificationBadgeLabel(9)).toBe("9");
    expect(getNotificationBadgeLabel(10)).toBe("9+");
    expect(getNotificationBadgeLabel(42)).toBe("9+");
  });

  it("uses a touch-sized bell only for compact mobile headers", () => {
    expect(getNotificationBellButtonClass()).toContain("h-7 w-7");
    expect(getNotificationBellButtonClass(true)).toContain("h-11 w-11");
    expect(getNotificationBellIconSize()).toBe(16);
    expect(getNotificationBellIconSize(true)).toBe(20);
  });

  it("keeps notification-center actions touch-safe only on compact mobile shells", () => {
    expect(getNotificationCenterActionClass()).toContain("px-1 py-1");
    expect(getNotificationCenterActionClass(true)).toContain("min-h-11");
    expect(getNotificationCenterCloseClass()).toContain("h-8 w-8");
    expect(getNotificationCenterCloseClass(true)).toContain("h-11 w-11");
  });

  it("keeps notification banners and inbox content viewport-bounded", () => {
    const desktopBanner = getNotificationBannerClass();
    const touchBanner = getNotificationBannerClass(true);
    const desktopPanel = getNotificationCenterPanelClass();
    const dialogPanel = getNotificationCenterPanelClass(false, "dialog");
    const desktopList = getNotificationCenterListClass();
    const touchList = getNotificationCenterListClass(true);

    expect(desktopBanner).toContain("md:left-auto");
    expect(desktopBanner).toContain("md:w-96");
    expect(desktopBanner).toContain("max-h-[calc(100dvh-");
    expect(desktopBanner).toContain("overflow-y-auto");
    expect(desktopBanner).toContain("overscroll-contain");
    expect(desktopBanner).toContain("break-words");
    expect(touchBanner).not.toContain("md:left-auto");
    expect(touchBanner).toContain("fixed left-3 right-3");
    expect(desktopPanel).toContain("w-[min(22rem,calc(100vw-1rem))]");
    expect(desktopPanel).toContain("overflow-hidden");
    expect(desktopPanel).not.toContain("fixed");
    expect(desktopPanel).not.toContain("absolute");
    expect(dialogPanel).toBe("overflow-hidden");
    expect(desktopList).toContain("md:max-h-80");
    expect(touchList).not.toContain("md:max-h-80");
  });

  it("uses the shared anchored popover and dialog owners while preserving mobile portal escape", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/NotificationBell.tsx"), "utf8");

    expect(source).toContain("<Popover modal open={open} onOpenChange={setOpen}>");
    expect(source).toContain("<PopoverTrigger asChild>");
    expect(source).toContain("<PopoverContent");
    expect(source).toContain('aria-modal="true"');
    expect(source).toContain("<DialogShell");
    expect(source).toContain("<DialogHeader");
    expect(source).toContain("<DialogBody");
    expect(source).toContain("createPortal(dialogCenter, document.body)");
    expect(source).toContain("onOpenChange?.(open)");
    expect(source).not.toContain("fixed inset-0 z-40");
  });

  it("announces each transient banner while retaining a descriptive dismiss action", () => {
    const record = notification("mention", false);
    const html = renderToStaticMarkup(
      createElement(NotificationBanner, {
        notification: record,
        onDismiss: () => undefined,
      }),
    );

    expect(getNotificationBannerDismissLabel(record)).toBe("Dismiss notification: Notification mention");
    expect(html).toContain('role="status"');
    expect(html).toContain('aria-live="polite"');
    expect(html).toContain('aria-atomic="true"');
    expect(html).toContain('aria-label="Dismiss notification: Notification mention"');
    expect(html).toContain("Notification mention");
  });

  it("clears stale rows and native-notification IDs after fetch failures", () => {
    const failureState = createNotificationFetchFailureState();

    expect(failureState.error).toBe(true);
    expect(failureState.notifications).toEqual([]);
    expect([...failureState.previousIds]).toEqual([]);
  });

  it("presents fetch failure as an accessible error with a touch-safe retry", () => {
    const html = renderToStaticMarkup(
      createElement(NotificationFetchError, { onRetry: () => undefined }),
    );

    expect(html).toContain('role="alert"');
    expect(html).toContain("border-danger/30");
    expect(html).toContain("bg-danger/10");
    expect(html).toContain("min-h-11 sm:min-h-0");
    expect(html).not.toContain("border-accent");
  });

  it("marks every visible notification row as read after mark-all-read succeeds", () => {
    const current = [notification("read", true), notification("unread", false)];
    const next = getNotificationsAfterMarkAllRead(current);

    expect(next.map((item) => item.read)).toEqual([true, true]);
    expect(next.map((item) => item.id)).toEqual(["read", "unread"]);
    expect(current.map((item) => item.read)).toEqual([true, false]);
  });

  it("settles mark-all-read success without publishing an error", async () => {
    const events: string[] = [];

    await settleMarkAllNotificationsRead(
      Promise.resolve(),
      () => events.push("success"),
      () => events.push("error"),
      () => events.push("complete"),
    );

    expect(events).toEqual(["success", "complete"]);
  });

  it("settles mark-all-read failures with feedback and releases the busy state", async () => {
    const events: string[] = [];

    await settleMarkAllNotificationsRead(
      Promise.reject(new Error("offline")),
      () => events.push("success"),
      () => events.push("error"),
      () => events.push("complete"),
    );

    expect(events).toEqual(["error", "complete"]);
  });

  it("exposes mark-all-read busy and failure states in the notification center", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/NotificationBell.tsx"), "utf8");

    expect(source).toContain("disabled={markingAllRead}");
    expect(source).toContain("aria-busy={markingAllRead}");
    expect(source).toContain('{markingAllRead ? "Marking…" : "Mark all read"}');
    expect(source).toContain("Couldn't mark notifications as read. Try again.");
  });

  it("names desktop notification rows with their read state", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/NotificationBell.tsx"), "utf8");

    expect(source).toContain('aria-label={`${n.read ? "Read" : "Unread"} notification: ${n.title}`}');
    expect(source).toContain('aria-hidden="true"');
  });

  it("bounds desktop per-row read mutations and keeps failures open for retry", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/NotificationBell.tsx"), "utf8");

    expect(source).toContain("if (markingReadIds.has(n.id)) return;");
    expect(source).toContain("disabled={markingReadIds.has(n.id)}");
    expect(source).toContain("aria-busy={markingReadIds.has(n.id)}");
    expect(source).toContain("Couldn't mark that notification as read. Try again.");
  });

  it("dismisses only the presentation banner and leaves its durable inbox row intact", () => {
    const first = notification("first", false);
    const next = notification("next", false);
    const bannerQueue = [next];
    const inbox = [first, next];

    expect(getNextNotificationBanner(bannerQueue)).toEqual(next);
    expect(bannerQueue).toEqual([]);
    expect(inbox).toEqual([first, next]);
  });

  it("uses a pure queue transition so Strict Mode cannot consume a newly queued banner twice", () => {
    const first = notification("first", false);
    const second = notification("second", false);
    const empty = { active: null, queued: [] };
    const enqueued = getNotificationBannerQueueAfterEnqueue(empty, [first, second]);

    expect(enqueued).toEqual({ active: first, queued: [second] });
    expect(empty).toEqual({ active: null, queued: [] });
    expect(getNotificationBannerQueueAfterDismiss(enqueued)).toEqual({
      active: second,
      queued: [],
    });
  });

  it("puts an immediate system event at the top of the inbox without duplicating it", () => {
    const existing = notification("existing", false);
    const updatedExisting = { ...existing, title: "Updated notification" };
    const another = notification("another", true);

    expect(getNotificationsAfterSystemEvent([existing, another], updatedExisting)).toEqual([
      updatedExisting,
      another,
    ]);
  });

});
