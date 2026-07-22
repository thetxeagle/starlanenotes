import { describe, expect, it } from "vitest";
import {
  createNotificationFetchFailureState,
  getNotificationBadgeLabel,
  getNotificationsAfterMarkAllRead,
  getUnreadNotificationCount,
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

  it("clears stale rows and native-notification IDs after fetch failures", () => {
    const failureState = createNotificationFetchFailureState();

    expect(failureState.error).toBe(true);
    expect(failureState.notifications).toEqual([]);
    expect([...failureState.previousIds]).toEqual([]);
  });

  it("marks every visible notification row as read after mark-all-read succeeds", () => {
    const current = [notification("read", true), notification("unread", false)];
    const next = getNotificationsAfterMarkAllRead(current);

    expect(next.map((item) => item.read)).toEqual([true, true]);
    expect(next.map((item) => item.id)).toEqual(["read", "unread"]);
    expect(current.map((item) => item.read)).toEqual([true, false]);
  });
});
