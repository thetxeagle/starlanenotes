import { useCallback, useEffect, useRef, useState } from "react";
import { Bell } from "lucide-react";
import { isTauri } from "@/lib/api";
import { dataClient, type NotificationRecord } from "@/lib/dataClient";
import { SYSTEM_NOTIFICATION_EVENT } from "@/lib/systemNotifications";

interface TauriCoreModule {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
}

async function sendNativeNotification(title: string, body: string) {
  if (!isTauri()) return;
  try {
    const coreModulePath = "@tauri-apps/api/core";
    const coreMod = (await import(/* @vite-ignore */ coreModulePath)) as unknown as TauriCoreModule;
    await coreMod.invoke("send_native_notification", { title, body });
  } catch {
    // Not in Tauri or permission denied
  }
}

export function getUnreadNotificationCount(notifications: NotificationRecord[]): number {
  return notifications.filter((notification) => !notification.read).length;
}

export function getNotificationBadgeLabel(unreadCount: number): string {
  return unreadCount > 9 ? "9+" : String(unreadCount);
}

export function createNotificationFetchFailureState(): {
  error: true;
  notifications: NotificationRecord[];
  previousIds: Set<string>;
} {
  return {
    error: true,
    notifications: [],
    previousIds: new Set(),
  };
}

export function getNotificationsAfterMarkAllRead(
  notifications: NotificationRecord[],
): NotificationRecord[] {
  return notifications.map((notification) => ({ ...notification, read: true }));
}

export function NotificationBell() {
  const [notifications, setNotifications] = useState<NotificationRecord[]>([]);
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [unreadOnly, setUnreadOnly] = useState(false);
  const [banner, setBanner] = useState<NotificationRecord | null>(null);
  const prevIdsRef = useRef<Set<string>>(new Set());
  const hasLoadedRef = useRef(false);
  const bannerQueueRef = useRef<NotificationRecord[]>([]);

  const unreadCount = getUnreadNotificationCount(notifications);

  const dismissBanner = useCallback(() => {
    setBanner(bannerQueueRef.current.shift() ?? null);
  }, []);

  const enqueueBanners = useCallback((records: NotificationRecord[]) => {
    if (records.length === 0) return;
    bannerQueueRef.current.push(...records);
    setBanner((current) => current ?? bannerQueueRef.current.shift() ?? null);
  }, []);

  const fetchNotifications = useCallback(async () => {
    setLoading(true);
    setError(false);
    try {
      const data = await dataClient.listNotifications({ limit: 30, unreadOnly });

      // Detect newly arrived unread notifications and trigger native notification
      // only when the window is not focused (i.e., app is in background)
      const newUnread = hasLoadedRef.current
        ? data.filter((n) => !n.read && !prevIdsRef.current.has(n.id))
        : [];
      for (const n of newUnread) {
        if (!document.hasFocus()) {
          await sendNativeNotification(n.title, n.body ?? "");
        }
      }

      enqueueBanners(newUnread);
      prevIdsRef.current = new Set(data.map((n) => n.id));
      hasLoadedRef.current = true;
      setNotifications(data);
    } catch {
      const failureState = createNotificationFetchFailureState();
      setNotifications(failureState.notifications);
      prevIdsRef.current = failureState.previousIds;
      setError(failureState.error);
    } finally {
      setLoading(false);
    }
  }, [enqueueBanners, unreadOnly]);

  useEffect(() => {
    if (!banner) return;
    const timeout = window.setTimeout(dismissBanner, 6000);
    return () => window.clearTimeout(timeout);
  }, [banner, dismissBanner]);

  useEffect(() => {
    fetchNotifications();
    const refreshOnFocus = () => {
      if (document.visibilityState === "visible") void fetchNotifications();
    };
    const interval = setInterval(fetchNotifications, 15_000);
    window.addEventListener("focus", refreshOnFocus);
    document.addEventListener("visibilitychange", refreshOnFocus);
    return () => {
      clearInterval(interval);
      window.removeEventListener("focus", refreshOnFocus);
      document.removeEventListener("visibilitychange", refreshOnFocus);
    };
  }, [fetchNotifications]);

  useEffect(() => {
    const onSystemNotification = (event: Event) => {
      const notification = (event as CustomEvent<NotificationRecord>).detail;
      if (!notification?.id) return;
      prevIdsRef.current.add(notification.id);
      setNotifications((current) => [notification, ...current.filter((item) => item.id !== notification.id)]);
      enqueueBanners([notification]);
    };
    window.addEventListener(SYSTEM_NOTIFICATION_EVENT, onSystemNotification);
    return () => window.removeEventListener(SYSTEM_NOTIFICATION_EVENT, onSystemNotification);
  }, [enqueueBanners]);

  const markAllRead = async () => {
    await dataClient.markAllNotificationsRead();
    setNotifications((prev) => getNotificationsAfterMarkAllRead(prev));
  };

  const handleClick = async (n: NotificationRecord) => {
    if (!n.read) {
      await dataClient.markNotificationRead(n.id);
      setNotifications((prev) =>
        prev.map((notif) => (notif.id === n.id ? { ...notif, read: true } : notif)),
      );
    }
    setOpen(false);
  };

  return (
    <div className="relative">
      {banner && (
        <button
          type="button"
          onClick={dismissBanner}
          className="fixed left-3 right-3 top-[max(0.75rem,env(safe-area-inset-top))] z-[60] max-w-md rounded-button border border-accent/60 bg-surface px-3 py-2 text-left shadow-card md:left-auto md:right-4 md:w-96"
          aria-label="Dismiss notification banner"
        >
          <p className="text-sm font-medium text-text">{banner.title}</p>
          {banner.body && <p className="mt-1 text-xs text-text-muted">{banner.body}</p>}
        </button>
      )}
      {/* Bell button */}
      <button
        onClick={() => setOpen(!open)}
        className="relative flex h-7 w-7 items-center justify-center rounded-button text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
        aria-label="Notifications"
      >
        <Bell size={16} />
        {unreadCount > 0 && (
          <span className="absolute -right-0.5 -top-0.5 flex h-3.5 min-w-[14px] items-center justify-center rounded-full bg-accent px-1 text-[9px] font-medium text-white">
            {getNotificationBadgeLabel(unreadCount)}
          </span>
        )}
      </button>

      {/* Dropdown panel */}
      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div className="absolute right-0 top-full z-50 mt-1 w-80 overflow-hidden rounded-card border border-border bg-surface shadow-card">
            {/* Header */}
            <div className="flex items-center justify-between border-b border-border px-3 py-2">
              <span className="text-sm font-medium text-text">Notifications</span>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => setUnreadOnly((current) => !current)}
                  className={`text-xs hover:underline ${unreadOnly ? "text-primary" : "text-text-muted"}`}
                  aria-pressed={unreadOnly}
                >
                  Unread
                </button>
                {unreadCount > 0 && (
                  <button
                    onClick={markAllRead}
                    className="text-xs text-primary hover:underline"
                  >
                    Mark all read
                  </button>
                )}
              </div>
            </div>

            {/* List */}
            <div className="max-h-80 overflow-y-auto">
              {loading ? (
                <div className="space-y-2 p-3" aria-label="Loading notifications">
                  <div className="h-10 animate-pulse rounded-card border border-border bg-surface-raised" />
                  <div className="h-10 animate-pulse rounded-card border border-border bg-surface-raised" />
                </div>
              ) : error ? (
                <div className="m-3 rounded-card border border-accent/60 p-3 text-sm text-text-muted">
                  <p>Couldn&apos;t load notifications.</p>
                  <button
                    type="button"
                    onClick={() => void fetchNotifications()}
                    className="mt-2 text-xs text-primary hover:underline"
                  >
                    Retry
                  </button>
                </div>
              ) : notifications.length === 0 ? (
                <div className="p-4 text-center text-sm text-text-muted">No notifications</div>
              ) : (
                notifications.map((n) => (
                  <button
                    key={n.id}
                    onClick={() => handleClick(n)}
                    className={`flex w-full gap-2 border-b border-border px-3 py-2 text-left transition-colors duration-100 hover:bg-surface-raised ${
                      n.read ? "" : "bg-primary/5"
                    }`}
                  >
                    {!n.read && (
                      <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-primary" />
                    )}
                    <div className="min-w-0 flex-1">
                      <div className="text-sm text-text">{n.title}</div>
                      {n.body && (
                        <div className="mt-0.5 truncate text-xs text-text-muted">{n.body}</div>
                      )}
                      <div className="mt-1 text-[10px] text-text-muted">
                        {new Date(n.created_at).toLocaleString()}
                      </div>
                    </div>
                  </button>
                ))
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
