import { dataClient, type NotificationRecord } from "@/lib/dataClient";

export const SYSTEM_NOTIFICATION_EVENT = "starlane:notification-recorded";

export function emitTransientSystemNotification(input: {
  title: string;
  body?: string;
}): NotificationRecord {
  const now = new Date().toISOString();
  const notification: NotificationRecord = {
    id: `transient-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    notification_type: "system",
    title: input.title,
    body: input.body,
    read: false,
    created_at: now,
  };
  if (typeof window !== "undefined") {
    window.dispatchEvent(
      new CustomEvent<NotificationRecord>(SYSTEM_NOTIFICATION_EVENT, { detail: notification }),
    );
  }
  return notification;
}

export async function recordSystemNotification(input: {
  title: string;
  body?: string;
}): Promise<NotificationRecord | null> {
  const notification = await dataClient.createSystemNotification(input);
  if (notification && typeof window !== "undefined") {
    window.dispatchEvent(
      new CustomEvent<NotificationRecord>(SYSTEM_NOTIFICATION_EVENT, { detail: notification }),
    );
  }
  return notification;
}
