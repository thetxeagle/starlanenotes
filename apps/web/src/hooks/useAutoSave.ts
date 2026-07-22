import { useCallback, useEffect, useRef, useState } from "react";

export type SaveStatus = "idle" | "saving" | "saved" | "error";

export function useAutoSave(
  saveFn: (content: string) => Promise<void>,
  delayMs = 2000,
) {
  const [status, setStatus] = useState<SaveStatus>("idle");
  const timeoutRef = useRef<ReturnType<typeof setTimeout>>();
  const latestContent = useRef<string>("");
  const isSaving = useRef(false);

  const save = useCallback(
    async (content: string) => {
      if (isSaving.current) return;
      isSaving.current = true;
      setStatus("saving");

      try {
        await saveFn(content);
        setStatus("saved");
        // Reset to idle after 2s
        setTimeout(() => setStatus("idle"), 2000);
      } catch {
        setStatus("error");
      } finally {
        isSaving.current = false;
      }
    },
    [saveFn],
  );

  const scheduleAutoSave = useCallback(
    (content: string) => {
      latestContent.current = content;
      setStatus("idle");

      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }

      timeoutRef.current = setTimeout(() => {
        save(latestContent.current);
      }, delayMs);
    },
    [save, delayMs],
  );

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, []);

  return { status, scheduleAutoSave, saveNow: save };
}
