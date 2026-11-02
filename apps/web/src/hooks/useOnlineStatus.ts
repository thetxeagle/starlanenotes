import { useCallback, useEffect, useRef, useState } from "react";
import { getApiBase } from "@/lib/api";

const PING_INTERVAL = 30_000;
const PING_TIMEOUT = 8_000;
const OFFLINE_FAILURE_THRESHOLD = 2;

export function useOnlineStatus(): boolean {
  const [online, setOnline] = useState(navigator.onLine);
  const intervalRef = useRef<ReturnType<typeof setInterval> | undefined>(undefined);
  const failedPingsRef = useRef(0);

  const pingServer = useCallback(async () => {
    try {
      const res = await fetch(`${getApiBase()}/auth/config`, {
        method: "GET",
        signal: AbortSignal.timeout(PING_TIMEOUT),
      });
      if (res.ok) {
        failedPingsRef.current = 0;
        setOnline(true);
        return;
      }
      failedPingsRef.current += 1;
      if (failedPingsRef.current >= OFFLINE_FAILURE_THRESHOLD) setOnline(false);
    } catch {
      failedPingsRef.current += 1;
      if (failedPingsRef.current >= OFFLINE_FAILURE_THRESHOLD) setOnline(false);
    }
  }, []);

  useEffect(() => {
    const handleOnline = () => {
      failedPingsRef.current = 0;
      setOnline(true);
      pingServer();
    };
    const handleOffline = () => {
      failedPingsRef.current = OFFLINE_FAILURE_THRESHOLD;
      setOnline(false);
    };

    window.addEventListener("online", handleOnline);
    window.addEventListener("offline", handleOffline);

    pingServer();
    intervalRef.current = setInterval(pingServer, PING_INTERVAL);

    return () => {
      window.removeEventListener("online", handleOnline);
      window.removeEventListener("offline", handleOffline);
      clearInterval(intervalRef.current);
    };
  }, [pingServer]);

  return online;
}
