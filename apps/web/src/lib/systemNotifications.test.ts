import { afterEach, describe, expect, it, vi } from "vitest";
import type { NotificationRecord } from "@/lib/dataClient";

const { createSystemNotification } = vi.hoisted(() => ({
  createSystemNotification: vi.fn(),
}));

vi.mock("@/lib/dataClient", () => ({
  dataClient: { createSystemNotification },
}));

import {
  emitTransientSystemNotification,
  recordSystemNotification,
  SYSTEM_NOTIFICATION_EVENT,
} from "./systemNotifications";

const recordedNotification: NotificationRecord = {
  id: "notification-profile-saved",
  notification_type: "system",
  title: "Starlane Notes",
  body: "Profile updated.",
  read: false,
  created_at: "2026-07-23T00:00:00Z",
};

class TestCustomEvent<T> extends Event {
  detail: T;

  constructor(type: string, init: CustomEventInit<T>) {
    super(type);
    this.detail = init.detail as T;
  }
}

describe("recordSystemNotification", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("creates the durable record before publishing the immediate banner event", async () => {
    const notificationWindow = new EventTarget();
    vi.stubGlobal("window", notificationWindow);
    vi.stubGlobal("CustomEvent", TestCustomEvent);
    createSystemNotification.mockResolvedValue(recordedNotification);

    const published: NotificationRecord[] = [];
    notificationWindow.addEventListener(SYSTEM_NOTIFICATION_EVENT, (event) => {
      published.push((event as TestCustomEvent<NotificationRecord>).detail);
    });

    await expect(
      recordSystemNotification({ title: "Starlane Notes", body: "Profile updated." }),
    ).resolves.toEqual(recordedNotification);

    expect(createSystemNotification).toHaveBeenCalledWith({
      title: "Starlane Notes",
      body: "Profile updated.",
    });
    expect(published).toEqual([recordedNotification]);
  });

  it("does not publish a banner event when the authenticated inbox declines the record", async () => {
    const notificationWindow = new EventTarget();
    vi.stubGlobal("window", notificationWindow);
    vi.stubGlobal("CustomEvent", TestCustomEvent);
    createSystemNotification.mockResolvedValue(null);
    const listener = vi.fn();
    notificationWindow.addEventListener(SYSTEM_NOTIFICATION_EVENT, listener);

    await expect(recordSystemNotification({ title: "Starlane Notes" })).resolves.toBeNull();

    expect(listener).not.toHaveBeenCalled();
  });
});

describe("emitTransientSystemNotification", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("publishes a banner-only notification without calling the durable API", () => {
    const notificationWindow = new EventTarget();
    vi.stubGlobal("window", notificationWindow);
    vi.stubGlobal("CustomEvent", TestCustomEvent);
    const published: NotificationRecord[] = [];
    notificationWindow.addEventListener(SYSTEM_NOTIFICATION_EVENT, (event) => {
      published.push((event as TestCustomEvent<NotificationRecord>).detail);
    });

    const notification = emitTransientSystemNotification({
      title: "Changes saved",
      body: "Stored on this device.",
    });

    expect(notification.id).toMatch(/^transient-/);
    expect(notification.read).toBe(false);
    expect(published).toEqual([notification]);
  });
});
