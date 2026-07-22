import { useCallback, useEffect, useRef, useState } from "react";
import { getApiBase } from "@/lib/api";

const PING_INTERVAL = 30_000;

export function useOnlineStatus(): boolean {
  const [online, setOnline] = useState(navigator.onLine);
  const intervalRef = useRef<ReturnType<typeof setInterval>>();

  const pingServer = useCallback(async () => {
    try {
      const res = await fetch(`${getApiBase()}/auth/config`, {
        method: "GET",
        signal: AbortSignal.timeout(5000),
      });
      setOnline(res.ok);
    } catch {
      setOnline(false);
    }
  }, []);

  useEffect(() => {
    const handleOnline = () => {
      setOnline(true);
      pingServer();
    };
    const handleOffline = () => setOnline(false);

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
