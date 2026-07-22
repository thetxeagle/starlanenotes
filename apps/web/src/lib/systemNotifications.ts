import { dataClient, type NotificationRecord } from "@/lib/dataClient";

export const SYSTEM_NOTIFICATION_EVENT = "starlane:notification-recorded";

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
