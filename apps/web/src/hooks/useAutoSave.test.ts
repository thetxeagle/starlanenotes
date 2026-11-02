import { afterEach, describe, expect, it, vi } from "vitest";
import { createAutoSaveController, type SaveStatus } from "./useAutoSave";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("useAutoSave controller", () => {
  afterEach(() => vi.useRealTimers());

  it("debounces changes and persists only the latest content", async () => {
    vi.useFakeTimers();
    const save = vi.fn(async (_content: string) => undefined);
    const statuses: SaveStatus[] = [];
    const controller = createAutoSaveController(save, (status) => statuses.push(status), 100);

    controller.scheduleAutoSave("older");
    controller.scheduleAutoSave("latest");
    await vi.advanceTimersByTimeAsync(99);
    expect(save).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(save).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenCalledWith("latest");
    expect(statuses).toContain("saved");
    controller.dispose();
  });

  it("awaits an in-flight save before persisting the requested latest content", async () => {
    vi.useFakeTimers();
    const firstSave = deferred<void>();
    let activeSaves = 0;
    let maximumConcurrentSaves = 0;
    const save = vi.fn(async (content: string) => {
      activeSaves += 1;
      maximumConcurrentSaves = Math.max(maximumConcurrentSaves, activeSaves);
      try {
        if (content === "first") await firstSave.promise;
      } finally {
        activeSaves -= 1;
      }
    });
    const controller = createAutoSaveController(save, () => undefined, 100);

    controller.scheduleAutoSave("first");
    await vi.advanceTimersByTimeAsync(100);
    expect(save).toHaveBeenCalledWith("first");

    const explicitSave = controller.saveNow("latest");
    expect(save).toHaveBeenCalledTimes(1);
    firstSave.resolve();

    await expect(explicitSave).resolves.toBe(true);
    expect(save).toHaveBeenNthCalledWith(2, "latest");
    expect(maximumConcurrentSaves).toBe(1);
    controller.dispose();
  });

  it("does not let a pre-reset flush save content from the next draft", async () => {
    vi.useFakeTimers();
    const firstSave = deferred<void>();
    const save = vi.fn(async (content: string) => {
      if (content === "old draft") await firstSave.promise;
    });
    const controller = createAutoSaveController(save, () => undefined, 100);

    controller.scheduleAutoSave("old draft");
    await vi.advanceTimersByTimeAsync(100);
    const oldFlush = controller.saveNow("old draft latest");

    controller.resetAutoSave("new draft baseline");
    controller.scheduleAutoSave("new draft content");
    firstSave.resolve();
    await expect(oldFlush).resolves.toBe(false);
    expect(save.mock.calls.map(([content]) => content)).toEqual(["old draft"]);

    await vi.advanceTimersByTimeAsync(100);
    expect(save.mock.calls.map(([content]) => content)).toEqual(["old draft", "new draft content"]);
    controller.dispose();
  });

  it("cancels a pending debounce when saveNow starts", async () => {
    vi.useFakeTimers();
    const save = vi.fn(async (_content: string) => undefined);
    const controller = createAutoSaveController(save, () => undefined, 100);

    controller.scheduleAutoSave("scheduled");
    await expect(controller.saveNow("explicit")).resolves.toBe(true);
    await vi.advanceTimersByTimeAsync(100);

    expect(save).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenCalledWith("explicit");
    controller.dispose();
  });

  it("retries only the latest unsaved content after a failed save", async () => {
    vi.useFakeTimers();
    const save = vi
      .fn<(content: string) => Promise<void>>()
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValue(undefined);
    const statuses: SaveStatus[] = [];
    const controller = createAutoSaveController(save, (status) => statuses.push(status), 100);

    controller.scheduleAutoSave("failed-old-content");
    await vi.advanceTimersByTimeAsync(100);
    expect(statuses).toContain("error");

    controller.scheduleAutoSave("latest-content");
    await vi.advanceTimersByTimeAsync(100);
    expect(save.mock.calls.map(([content]) => content)).toEqual([
      "failed-old-content",
      "latest-content",
    ]);
    controller.dispose();
  });

  it("reset clears pending work and prevents an old status timer changing a newer save", async () => {
    vi.useFakeTimers();
    const nextSave = deferred<void>();
    const save = vi
      .fn<(content: string) => Promise<void>>()
      .mockResolvedValueOnce(undefined)
      .mockImplementationOnce(() => nextSave.promise);
    const statuses: SaveStatus[] = [];
    const controller = createAutoSaveController(save, (status) => statuses.push(status), 100);

    controller.scheduleAutoSave("abandoned debounce");
    controller.resetAutoSave("new draft baseline");
    await vi.advanceTimersByTimeAsync(1_000);
    expect(save).not.toHaveBeenCalled();

    controller.scheduleAutoSave("old draft edit");
    await vi.advanceTimersByTimeAsync(100);
    expect(statuses[statuses.length - 1]).toBe("saved");
    await vi.advanceTimersByTimeAsync(1_000);

    controller.resetAutoSave("another draft baseline");
    expect(statuses[statuses.length - 1]).toBe("idle");

    const newSave = controller.saveNow("new draft edit");
    await vi.advanceTimersByTimeAsync(2_000);
    expect(statuses[statuses.length - 1]).toBe("saving");
    nextSave.resolve();
    await expect(newSave).resolves.toBe(true);
    expect(save.mock.calls.map(([content]) => content)).toEqual([
      "old draft edit",
      "new draft edit",
    ]);
    controller.dispose();
  });

  it("cancels timers on disposal", async () => {
    vi.useFakeTimers();
    const save = vi.fn(async (_content: string) => undefined);
    const controller = createAutoSaveController(save, () => undefined, 100);

    controller.scheduleAutoSave("unsaved");
    controller.dispose();
    await vi.advanceTimersByTimeAsync(1_000);

    expect(save).not.toHaveBeenCalled();
  });
  it("holds failed CAS generations including edits waiting behind an in-flight save until explicit reset", async () => {
    vi.useFakeTimers();
    const first = deferred<void>();
    const save = vi.fn().mockImplementationOnce(() => first.promise).mockResolvedValue(undefined);
    const statuses: SaveStatus[] = [];
    const controller = createAutoSaveController(save, status => statuses.push(status), 100, () => false);
    const old = controller.saveNow("original A");
    const newer = controller.saveNow("current B remains dirty");
    first.reject(new Error("source_conflict"));
    expect(await old).toBe(false); expect(await newer).toBe(false);
    controller.scheduleAutoSave("newer C still held");
    await vi.advanceTimersByTimeAsync(1000);
    expect(save).toHaveBeenCalledTimes(1); expect(statuses[statuses.length - 1]).toBe("error");
    expect(await controller.saveNow("explicit click cannot bypass hold")).toBe(false);
    controller.resetAutoSave("explicitly reviewed baseline");
    expect(await controller.saveNow("reviewed merge")).toBe(true);
    expect(save).toHaveBeenLastCalledWith("reviewed merge");
    controller.dispose();
  });
  it("suspends queued dispatch while retaining the in-flight own ACK and newest paused proposal", async () => {
    vi.useFakeTimers();
    const first = deferred<void>();
    const save = vi.fn().mockImplementationOnce(() => first.promise).mockResolvedValue(undefined);
    const statuses: SaveStatus[] = [];
    const controller = createAutoSaveController(save, status => statuses.push(status), 100);
    const ownAck = controller.saveNow("A sent");
    const waiting = controller.saveNow("B queued");
    controller.suspendAutoSave();
    const pausedStatuses = statuses.length;
    controller.scheduleAutoSave("C newest");
    expect(await controller.saveNow("C latest manual proposal")).toBe(false);
    first.resolve();
    expect(await ownAck).toBe(true);
    expect(await waiting).toBe(false);
    await vi.advanceTimersByTimeAsync(1000);
    expect(save.mock.calls.map(([content]) => content)).toEqual(["A sent"]);
    expect(statuses.slice(pausedStatuses)).not.toContain("saved");
    controller.resumeAutoSave();
    await vi.advanceTimersByTimeAsync(99);
    expect(save).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(save.mock.calls.map(([content]) => content)).toEqual(["A sent", "C latest manual proposal"]);
    expect(statuses[statuses.length - 1]).toBe("saved");
    controller.dispose();
  });

  it("retains the exact in-flight ACK baseline without showing Saved during peer review", async () => {
    vi.useFakeTimers();
    const first = deferred<void>();
    const save = vi.fn(() => first.promise);
    const statuses: SaveStatus[] = [];
    const controller = createAutoSaveController(save, status => statuses.push(status), 100);
    const ownAck = controller.saveNow("only A in flight");
    controller.suspendAutoSave();
    const boundary = statuses.length;
    first.resolve();
    expect(await ownAck).toBe(true);
    await vi.advanceTimersByTimeAsync(1000);
    expect(statuses.slice(boundary)).not.toContain("saved");
    expect(statuses[statuses.length - 1]).toBe("idle");
    controller.resumeAutoSave();
    await vi.advanceTimersByTimeAsync(1000);
    expect(save).toHaveBeenCalledTimes(1); // Genuine own ACK survived the pause.
    expect(statuses).not.toContain("saved");
    controller.dispose();
  });

  it("does not let a pre-suspension waiting flush bypass the resume delay", async () => {
    vi.useFakeTimers();
    const first = deferred<void>();
    const save = vi.fn().mockImplementationOnce(() => first.promise).mockResolvedValue(undefined);
    const controller = createAutoSaveController(save, () => undefined, 100);
    const ownAck = controller.saveNow("A sent");
    const waiting = controller.saveNow("B waiting");
    controller.suspendAutoSave();
    controller.scheduleAutoSave("C latest");
    controller.resumeAutoSave();
    first.resolve();
    expect(await ownAck).toBe(true);
    expect(await waiting).toBe(false);
    expect(save).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(99);
    expect(save).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(save).toHaveBeenLastCalledWith("C latest");
    controller.dispose();
  });

  it("preserves a held failure through pause/resume and requires the existing explicit reset", async () => {
    vi.useFakeTimers();
    const first = deferred<void>();
    const save = vi.fn().mockImplementationOnce(() => first.promise).mockResolvedValue(undefined);
    const statuses: SaveStatus[] = [];
    const controller = createAutoSaveController(save, status => statuses.push(status), 100, () => false);
    const ownAttempt = controller.saveNow("original source-bound request");
    controller.suspendAutoSave();
    first.reject(new Error("source_conflict"));
    expect(await ownAttempt).toBe(false);
    controller.scheduleAutoSave("latest held proposal");
    controller.resumeAutoSave();
    await vi.advanceTimersByTimeAsync(1000);
    expect(save).toHaveBeenCalledTimes(1);
    expect(statuses[statuses.length - 1]).toBe("error");
    expect(await controller.saveNow("manual held proposal")).toBe(false);
    controller.suspendAutoSave();
    controller.resetAutoSave("reviewed new baseline");
    expect(await controller.saveNow("explicitly reviewed edit")).toBe(true);
    expect(save).toHaveBeenLastCalledWith("explicitly reviewed edit");
    controller.dispose();
  });

  it("cancels debounce/retry/saved timers without fabricating a saved baseline", async () => {
    vi.useFakeTimers();
    const statuses: SaveStatus[] = [];
    const save = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(undefined);
    const controller = createAutoSaveController(save, status => statuses.push(status), 100);
    expect(await controller.saveNow("needs retry")).toBe(false);
    controller.suspendAutoSave();
    await vi.advanceTimersByTimeAsync(1000);
    expect(save).toHaveBeenCalledTimes(1);
    controller.resumeAutoSave();
    await vi.advanceTimersByTimeAsync(100);
    expect(save).toHaveBeenCalledTimes(2);
    expect(statuses[statuses.length - 1]).toBe("saved");
    controller.suspendAutoSave();
    const count = statuses.length;
    await vi.advanceTimersByTimeAsync(3000);
    expect(statuses).toHaveLength(count);
    controller.scheduleAutoSave("paused debounce");
    controller.resumeAutoSave();
    controller.suspendAutoSave();
    await vi.advanceTimersByTimeAsync(1000);
    expect(save).toHaveBeenCalledTimes(2);
    controller.resumeAutoSave();
    await vi.advanceTimersByTimeAsync(100);
    expect(save).toHaveBeenLastCalledWith("paused debounce");
    controller.dispose();
  });

  it("reset clears suspension for the new context while late old ACK/status stays ignored", async () => {
    vi.useFakeTimers();
    const first = deferred<void>();
    const save = vi.fn().mockImplementationOnce(() => first.promise).mockResolvedValue(undefined);
    const statuses: SaveStatus[] = [];
    const controller = createAutoSaveController(save, status => statuses.push(status), 100);
    const old = controller.saveNow("old request");
    controller.suspendAutoSave();
    controller.scheduleAutoSave("old paused proposal");
    controller.resetAutoSave("new selected baseline");
    const boundary = statuses.length;
    first.resolve(); await old;
    expect(statuses).toHaveLength(boundary);
    controller.scheduleAutoSave("new selection proposal");
    await vi.advanceTimersByTimeAsync(100);
    expect(save.mock.calls.map(([content]) => content)).toEqual(["old request", "new selection proposal"]);
    controller.dispose();
  });

  it("disposal while suspended cancels resume and ignores in-flight status changes", async () => {
    vi.useFakeTimers();
    const first = deferred<void>();
    const save = vi.fn(() => first.promise);
    const statuses: SaveStatus[] = [];
    const controller = createAutoSaveController(save, status => statuses.push(status), 100);
    const request = controller.saveNow("A");
    controller.suspendAutoSave(); controller.scheduleAutoSave("B");
    controller.dispose(); const boundary = statuses.length;
    controller.resumeAutoSave(); first.resolve(); await request;
    expect(await controller.saveNow("C")).toBe(false);
    await vi.advanceTimersByTimeAsync(1000);
    expect(save).toHaveBeenCalledTimes(1); expect(statuses).toHaveLength(boundary);
  });

});
