import { useCallback, useEffect, useRef, useState } from "react";

export type SaveStatus = "idle" | "saving" | "saved" | "error";

export const DEFAULT_AUTOSAVE_DELAY_MS = 30_000;
const SAVED_STATUS_DURATION_MS = 2_000;

export interface AutoSaveController {
  scheduleAutoSave: (content: string) => void;
  saveNow: (content: string) => Promise<boolean>;
  resetAutoSave: (content: string) => void;
  suspendAutoSave: () => void;
  resumeAutoSave: () => void;
  dispose: () => void;
}

/** Pure timer/save coordinator used by the hook and its focused tests. */
export function createAutoSaveController(
  saveFn: (content: string) => Promise<void>,
  onStatusChange: (status: SaveStatus) => void,
  delayMs = DEFAULT_AUTOSAVE_DELAY_MS,
  shouldRetryError: (error: unknown) => boolean = () => true,
): AutoSaveController {
  let latestContent = "";
  let latestVersion = 0;
  let savedVersion = 0;
  let generation = 0;
  let statusVersion = 0;
  let inFlight: Promise<boolean> | null = null;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let statusTimer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;
  let held = false;
  let suspended = false;
  // A waiting flush may not cross a pause, even when resumed before its await settles.
  let suspensionVersion = 0;

  const clearTimer = (timer: ReturnType<typeof setTimeout> | undefined) => {
    if (timer !== undefined) clearTimeout(timer);
  };

  const clearDebounceAndRetry = () => {
    clearTimer(debounceTimer);
    clearTimer(retryTimer);
    debounceTimer = undefined;
    retryTimer = undefined;
  };

  const clearStatusTimer = () => {
    clearTimer(statusTimer);
    statusTimer = undefined;
    statusVersion += 1;
  };

  const setStatus = (status: SaveStatus) => {
    if (!disposed) onStatusChange(status);
  };

  const scheduleRetry = (saveGeneration: number) => {
    clearTimer(retryTimer);
    retryTimer = undefined;
    if (suspended || held || disposed) return;
    retryTimer = setTimeout(() => {
      retryTimer = undefined;
      if (disposed || suspended || generation !== saveGeneration) return;
      void flushLatest();
    }, delayMs);
  };

  const persist = (content: string, version: number, saveGeneration: number) => {
    clearStatusTimer();
    setStatus("saving");

    let saveRequest: Promise<void>;
    try {
      // Invoke now so this write captures the current draft identity before a
      // later reset can switch the callback to another draft.
      saveRequest = saveFn(content);
    } catch (error) {
      saveRequest = Promise.reject(error);
    }

    const request = Promise.resolve()
      .then(() => saveRequest)
      .then((result) => result)
      .then(
        () => {
          if (!disposed && generation === saveGeneration) {
            savedVersion = version;
            if (!suspended && latestVersion === version) {
              clearStatusTimer();
              setStatus("saved");
              const completedStatusVersion = statusVersion;
              statusTimer = setTimeout(() => {
                if (
                  !disposed &&
                  !suspended &&
                  generation === saveGeneration &&
                  statusVersion === completedStatusVersion &&
                  latestVersion === savedVersion
                ) {
                  statusTimer = undefined;
                  setStatus("idle");
                }
              }, SAVED_STATUS_DURATION_MS);
            }
          }
          return true;
        },
        (error: unknown) => {
          if (!disposed && generation === saveGeneration && !shouldRetryError(error)) {
            held = true;
            clearDebounceAndRetry();
            clearStatusTimer();
            setStatus("error");
          } else if (!disposed && generation === saveGeneration && latestVersion === version) {
            clearStatusTimer();
            setStatus("error");
            scheduleRetry(saveGeneration);
          }
          return false;
        },
      )
      .finally(() => {
        inFlight = null;
      });

    inFlight = request;
    return request;
  };

  const flushLatest = async (): Promise<boolean> => {
    const flushGeneration = generation;
    const flushSuspensionVersion = suspensionVersion;
    clearTimer(debounceTimer);
    debounceTimer = undefined;

    if (suspended || disposed) return false;
    while (inFlight) {
      await inFlight;
      if (disposed || suspended || generation !== flushGeneration || suspensionVersion !== flushSuspensionVersion || held) return false;
    }
    if (disposed || suspended || generation !== flushGeneration || suspensionVersion !== flushSuspensionVersion || held) return false;

    clearTimer(retryTimer);
    retryTimer = undefined;
    if (savedVersion === latestVersion) return true;
    return persist(latestContent, latestVersion, generation);
  };

  return {
    scheduleAutoSave(content) {
      if (disposed) return;
      latestContent = content;
      latestVersion += 1;
      clearDebounceAndRetry();
      clearStatusTimer();
      if (held) { setStatus("error"); return; }
      setStatus("idle");
      if (suspended) return;
      debounceTimer = setTimeout(() => {
        debounceTimer = undefined;
        void flushLatest();
      }, delayMs);
    },
    async saveNow(content) {
      if (disposed) return false;
      latestContent = content;
      latestVersion += 1;
      clearDebounceAndRetry();
      return flushLatest();
    },
    resetAutoSave(content) {
      if (disposed) return;
      generation += 1;
      held = false;
      suspended = false;
      latestContent = content;
      latestVersion += 1;
      savedVersion = latestVersion;
      clearDebounceAndRetry();
      clearStatusTimer();
      setStatus("idle");
    },
    suspendAutoSave() {
      if (disposed || suspended) return;
      suspended = true;
      suspensionVersion += 1;
      clearDebounceAndRetry();
      clearStatusTimer();
      // Peer review is not a save acknowledgement; its notice belongs to the caller.
      setStatus(held ? "error" : "idle");
    },
    resumeAutoSave() {
      if (disposed || !suspended) return;
      suspended = false;
      if (held) { setStatus("error"); return; }
      setStatus("idle");
      if (savedVersion === latestVersion) return;
      debounceTimer = setTimeout(() => {
        debounceTimer = undefined;
        void flushLatest();
      }, delayMs);
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      clearDebounceAndRetry();
      clearStatusTimer();
    },
  };
}

export function useAutoSave(
  saveFn: (content: string) => Promise<void>,
  delayMs = DEFAULT_AUTOSAVE_DELAY_MS,
  shouldRetryError: (error: unknown) => boolean = () => true,
) {
  const [status, setStatus] = useState<SaveStatus>("idle");
  const saveFnRef = useRef(saveFn);
  saveFnRef.current = saveFn;
  const retryPolicyRef = useRef(shouldRetryError);
  retryPolicyRef.current = shouldRetryError;

  const controllerRef = useRef<AutoSaveController | null>(null);
  const createController = useCallback(
    () => createAutoSaveController((content) => saveFnRef.current(content), setStatus, delayMs, (error) => retryPolicyRef.current(error)),
    [delayMs, setStatus],
  );
  if (!controllerRef.current) controllerRef.current = createController();

  useEffect(() => {
    if (!controllerRef.current) controllerRef.current = createController();
    const controller = controllerRef.current;
    return () => {
      controller?.dispose();
      if (controllerRef.current === controller) controllerRef.current = null;
    };
  }, [createController]);

  const scheduleAutoSave = useCallback((content: string) => {
    controllerRef.current?.scheduleAutoSave(content);
  }, []);
  const saveNow = useCallback((content: string) => {
    return controllerRef.current?.saveNow(content) ?? Promise.resolve(false);
  }, []);
  const resetAutoSave = useCallback((content: string) => {
    controllerRef.current?.resetAutoSave(content);
  }, []);

  const suspendAutoSave = useCallback(() => {
    controllerRef.current?.suspendAutoSave();
  }, []);
  const resumeAutoSave = useCallback(() => {
    controllerRef.current?.resumeAutoSave();
  }, []);

  return {
    status,
    scheduleAutoSave,
    saveNow,
    resetAutoSave,
    suspendAutoSave,
    resumeAutoSave,
  };
}
