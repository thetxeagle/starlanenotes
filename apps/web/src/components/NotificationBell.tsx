import { useCallback, useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Bell } from "lucide-react";
import { isTauri } from "@/lib/api";
import { dataClient, type NotificationRecord } from "@/lib/dataClient";
import { SYSTEM_NOTIFICATION_EVENT } from "@/lib/systemNotifications";
import { Button, RESPONSIVE_TOUCH_ACTION_CLASS } from "@/components/ui/button";
import {
  DialogBody,
  DialogCloseAction,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";

interface TauriCoreModule {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
}

export interface NotificationBellProps {
  touchTarget?: boolean;
  presentation?: "popover" | "dialog";
  className?: string;
  iconClassName?: string;
  containerClassName?: string;
  onOpenChange?: (open: boolean) => void;
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

export function isCurrentNotificationRequest(
  requestId: number,
  currentRequestId: number,
): boolean {
  return requestId === currentRequestId;
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

export async function settleMarkAllNotificationsRead(
  request: Promise<void>,
  onSuccess: () => void,
  onError: () => void,
  onComplete: () => void,
): Promise<void> {
  try {
    await request;
    onSuccess();
  } catch {
    onError();
  } finally {
    onComplete();
  }
}

/** A banner is presentation-only; dismissing it must never mutate the inbox. */
export function getNextNotificationBanner(
  queuedNotifications: NotificationRecord[],
): NotificationRecord | null {
  return queuedNotifications.shift() ?? null;
}

export interface NotificationBannerQueue {
  active: NotificationRecord | null;
  queued: NotificationRecord[];
}

/**
 * Add records without mutating React state updater inputs. React development
 * Strict Mode may call an updater twice, so consuming a ref-backed queue from
 * inside an updater can otherwise discard the visible banner.
 */
export function getNotificationBannerQueueAfterEnqueue(
  queue: NotificationBannerQueue,
  records: NotificationRecord[],
): NotificationBannerQueue {
  if (records.length === 0) return queue;
  const queued = [...queue.queued, ...records];
  if (queue.active) return { active: queue.active, queued };
  const [active = null, ...remaining] = queued;
  return { active, queued: remaining };
}

/** Advance only the transient banner queue; durable inbox records stay untouched. */
export function getNotificationBannerQueueAfterDismiss(
  queue: NotificationBannerQueue,
): NotificationBannerQueue {
  const [active = null, ...queued] = queue.queued;
  return { active, queued };
}

/** Keep one durable inbox row per record when an immediate system event arrives. */
export function getNotificationsAfterSystemEvent(
  notifications: NotificationRecord[],
  notification: NotificationRecord,
): NotificationRecord[] {
  return [notification, ...notifications.filter((item) => item.id !== notification.id)];
}

export function getNotificationBellButtonClass(touchTarget = false): string {
  return `relative flex ${touchTarget ? "h-11 w-11" : "h-7 w-7"} items-center justify-center rounded-button text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text`;
}

export function getNotificationBellIconSize(touchTarget = false): number {
  return touchTarget ? 20 : 16;
}

export function getNotificationCenterActionClass(touchTarget = false): string {
  return `rounded-button text-text-muted hover:bg-surface-raised hover:text-text text-xs transition-colors duration-100 ${
    touchTarget ? "min-h-11 px-2" : "px-1 py-1"
  }`;
}

export function getNotificationCenterCloseClass(touchTarget = false): string {
  return `rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex items-center justify-center transition-colors duration-100 ${
    touchTarget ? "h-11 w-11" : "h-8 w-8"
  }`;
}

export function getNotificationBannerClass(touchTarget = false): string {
  const base =
    "fixed left-3 right-3 top-[max(0.75rem,env(safe-area-inset-top))] z-[60] max-h-[calc(100dvh-max(1.5rem,calc(env(safe-area-inset-top)+1.5rem)))] overflow-y-auto overscroll-contain break-words rounded-button border border-accent/60 bg-surface px-3 py-2 text-left shadow-card";
  return touchTarget ? base : `${base} max-w-md md:left-auto md:right-4 md:w-96`;
}

export function getNotificationCenterPanelClass(
  _touchTarget = false,
  presentation: "popover" | "dialog" = "popover",
): string {
  return presentation === "dialog"
    ? "overflow-hidden"
    : "w-[min(22rem,calc(100vw-1rem))] overflow-hidden p-0";
}

export function getNotificationCenterListClass(touchTarget = false): string {
  const touchList =
    "max-h-[calc(100dvh-max(9rem,calc(env(safe-area-inset-top)+8.5rem)))] overflow-y-auto";
  return touchTarget ? touchList : `${touchList} md:max-h-80`;
}

export function getNotificationBannerDismissLabel(notification: NotificationRecord): string {
  return `Dismiss notification: ${notification.title}`;
}

export function NotificationBanner({
  notification,
  touchTarget = false,
  onDismiss,
}: {
  notification: NotificationRecord;
  touchTarget?: boolean;
  onDismiss: () => void;
}) {
  return (
    <div role="status" aria-live="polite" aria-atomic="true">
      <button
        type="button"
        onClick={onDismiss}
        className={getNotificationBannerClass(touchTarget)}
        aria-label={getNotificationBannerDismissLabel(notification)}
      >
        <p className="text-sm font-medium text-text">{notification.title}</p>
        {notification.body && <p className="mt-1 text-xs text-text-muted">{notification.body}</p>}
      </button>
    </div>
  );
}

export function NotificationFetchError({ onRetry }: { onRetry: () => void }) {
  return (
    <div role="alert" className="m-3 rounded-card border border-danger/30 bg-danger/10 p-3 text-sm text-text">
      <p>Couldn&apos;t load notifications.</p>
      <Button
        type="button"
        variant="secondary"
        size="sm"
        className={`mt-2 ${RESPONSIVE_TOUCH_ACTION_CLASS}`}
        onClick={onRetry}
      >
        Retry
      </Button>
    </div>
  );
}

export function NotificationBell({
  touchTarget = false,
  presentation = "popover",
  className = "",
  iconClassName = "",
  containerClassName = "",
  onOpenChange,
}: NotificationBellProps) {
  const [notifications, setNotifications] = useState<NotificationRecord[]>([]);
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [markingAllRead, setMarkingAllRead] = useState(false);
  const [markAllReadError, setMarkAllReadError] = useState(false);
  const [markingReadIds, setMarkingReadIds] = useState<Set<string>>(new Set());
  const [markReadError, setMarkReadError] = useState(false);
  const [unreadOnly, setUnreadOnly] = useState(false);
  const [banner, setBanner] = useState<NotificationRecord | null>(null);
  const prevIdsRef = useRef<Set<string>>(new Set());
  const hasLoadedRef = useRef(false);
  const bannerQueueRef = useRef<NotificationBannerQueue>({ active: null, queued: [] });
  const notificationRequestIdRef = useRef(0);
  const notificationPanelId = useId();

  const unreadCount = getUnreadNotificationCount(notifications);

  const dismissBanner = useCallback(() => {
    const next = getNotificationBannerQueueAfterDismiss(bannerQueueRef.current);
    bannerQueueRef.current = next;
    setBanner(next.active);
  }, []);

  const enqueueBanners = useCallback((records: NotificationRecord[]) => {
    if (records.length === 0) return;
    const next = getNotificationBannerQueueAfterEnqueue(bannerQueueRef.current, records);
    bannerQueueRef.current = next;
    setBanner(next.active);
  }, []);

  const closeNotificationCenter = useCallback(() => setOpen(false), []);

  useEffect(() => {
    onOpenChange?.(open);
    return () => onOpenChange?.(false);
  }, [onOpenChange, open]);

  const fetchNotifications = useCallback(async () => {
    const requestId = ++notificationRequestIdRef.current;
    setLoading(true);
    setError(false);
    try {
      const data = await dataClient.listNotifications({ limit: 30, unreadOnly });
      if (!isCurrentNotificationRequest(requestId, notificationRequestIdRef.current)) return;

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
      if (!isCurrentNotificationRequest(requestId, notificationRequestIdRef.current)) return;
      const failureState = createNotificationFetchFailureState();
      setNotifications(failureState.notifications);
      prevIdsRef.current = failureState.previousIds;
      setError(failureState.error);
    } finally {
      if (isCurrentNotificationRequest(requestId, notificationRequestIdRef.current)) {
        setLoading(false);
      }
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
      notificationRequestIdRef.current += 1;
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
      setNotifications((current) => getNotificationsAfterSystemEvent(current, notification));
      enqueueBanners([notification]);
    };
    window.addEventListener(SYSTEM_NOTIFICATION_EVENT, onSystemNotification);
    return () => window.removeEventListener(SYSTEM_NOTIFICATION_EVENT, onSystemNotification);
  }, [enqueueBanners]);

  const markAllRead = async () => {
    if (markingAllRead) return;
    setMarkingAllRead(true);
    setMarkAllReadError(false);
    await settleMarkAllNotificationsRead(
      dataClient.markAllNotificationsRead(),
      () => setNotifications((prev) => getNotificationsAfterMarkAllRead(prev)),
      () => setMarkAllReadError(true),
      () => setMarkingAllRead(false),
    );
  };

  const handleClick = async (n: NotificationRecord) => {
    if (n.read) {
      closeNotificationCenter();
      return;
    }
    if (markingReadIds.has(n.id)) return;
    setMarkingReadIds((current) => new Set(current).add(n.id));
    setMarkReadError(false);
    try {
      await dataClient.markNotificationRead(n.id);
      setNotifications((prev) =>
        prev.map((notif) => (notif.id === n.id ? { ...notif, read: true } : notif)),
      );
      closeNotificationCenter();
    } catch {
      setMarkReadError(true);
    } finally {
      setMarkingReadIds((current) => {
        const next = new Set(current);
        next.delete(n.id);
        return next;
      });
    }
  };

  const notificationCenterContent = (
    <>
      <DialogHeader className="px-3 py-2">
        <DialogTitle id={`${notificationPanelId}-title`} className="text-sm">
          Notifications
        </DialogTitle>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => setUnreadOnly((current) => !current)}
            className={`${getNotificationCenterActionClass(touchTarget)} ${
              unreadOnly ? "text-primary" : ""
            }`}
            aria-pressed={unreadOnly}
          >
            Unread
          </button>
          {unreadCount > 0 && (
            <button
              type="button"
              onClick={() => void markAllRead()}
              disabled={markingAllRead}
              aria-busy={markingAllRead}
              className={`${getNotificationCenterActionClass(touchTarget)} text-primary disabled:pointer-events-none disabled:opacity-60`}
            >
              {markingAllRead ? "Marking…" : "Mark all read"}
            </button>
          )}
          <DialogCloseAction
            label="Close notifications"
            className={getNotificationCenterCloseClass(touchTarget)}
            onClick={closeNotificationCenter}
          />
        </div>
      </DialogHeader>

      <DialogBody className={`p-0 ${getNotificationCenterListClass(touchTarget)}`}>
        {(markAllReadError || markReadError) && (
          <div
            role="alert"
            className="m-3 rounded-card border border-danger/30 bg-danger/10 p-3 text-sm text-text"
          >
            {markReadError
              ? "Couldn't mark that notification as read. Try again."
              : "Couldn't mark notifications as read. Try again."}
          </div>
        )}
        {loading ? (
          <div className="space-y-2 p-3" aria-label="Loading notifications">
            <div className="h-10 animate-pulse rounded-card border border-border bg-surface-raised" />
            <div className="h-10 animate-pulse rounded-card border border-border bg-surface-raised" />
          </div>
        ) : error ? (
          <NotificationFetchError onRetry={() => void fetchNotifications()} />
        ) : notifications.length === 0 ? (
          <div className="p-4 text-center text-sm text-text-muted">No notifications</div>
        ) : (
          notifications.map((n) => (
            <button
              key={n.id}
              type="button"
              onClick={() => void handleClick(n)}
              disabled={markingReadIds.has(n.id)}
              aria-busy={markingReadIds.has(n.id)}
              aria-label={`${n.read ? "Read" : "Unread"} notification: ${n.title}`}
              className={`flex w-full gap-2 border-b border-border px-3 py-2 text-left transition-colors duration-100 hover:bg-surface-raised ${
                n.read ? "" : "bg-primary/5"
              } disabled:pointer-events-none disabled:opacity-60`}
            >
              {!n.read && (
                <span
                  className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-primary"
                  aria-hidden="true"
                />
              )}
              <div className="min-w-0 flex-1">
                <div className="text-sm text-text">{n.title}</div>
                {n.body && <div className="mt-0.5 truncate text-xs text-text-muted">{n.body}</div>}
                <div className="mt-1 text-[10px] text-text-muted">
                  {new Date(n.created_at).toLocaleString()}
                </div>
              </div>
            </button>
          ))
        )}
      </DialogBody>
    </>
  );

  const notificationTrigger = (
    <button
      type="button"
      onClick={presentation === "dialog" ? () => setOpen((current) => !current) : undefined}
      className={`${getNotificationBellButtonClass(touchTarget)} ${className}`}
      aria-label="Notifications"
      aria-haspopup="dialog"
      aria-expanded={open}
      aria-controls={notificationPanelId}
    >
      <Bell size={getNotificationBellIconSize(touchTarget)} className={iconClassName} />
      {unreadCount > 0 && (
        <span className="absolute -right-0.5 -top-0.5 flex h-3.5 min-w-[14px] items-center justify-center rounded-full bg-accent px-1 text-[9px] font-medium text-white">
          {getNotificationBadgeLabel(unreadCount)}
        </span>
      )}
    </button>
  );

  const dialogCenter =
    presentation === "dialog" && open ? (
      <DialogShell
        labelledBy={`${notificationPanelId}-title`}
        maxWidth="sm"
        onClose={closeNotificationCenter}
      >
        <div id={notificationPanelId} className={getNotificationCenterPanelClass(touchTarget, presentation)}>
          {notificationCenterContent}
        </div>
      </DialogShell>
    ) : null;

  return (
    <div className={`relative ${containerClassName}`}>
      {banner && (
        <NotificationBanner
          notification={banner}
          touchTarget={touchTarget}
          onDismiss={dismissBanner}
        />
      )}
      {presentation === "popover" ? (
        <Popover modal open={open} onOpenChange={setOpen}>
          <PopoverTrigger asChild>{notificationTrigger}</PopoverTrigger>
          <PopoverContent
            id={notificationPanelId}
            align="end"
            side="bottom"
            sideOffset={6}
            role="dialog"
            aria-modal="true"
            aria-labelledby={`${notificationPanelId}-title`}
            className={getNotificationCenterPanelClass(touchTarget, presentation)}
          >
            {notificationCenterContent}
          </PopoverContent>
        </Popover>
      ) : (
        notificationTrigger
      )}
      {dialogCenter && typeof document !== "undefined"
        ? createPortal(dialogCenter, document.body)
        : dialogCenter}
    </div>
  );
}
