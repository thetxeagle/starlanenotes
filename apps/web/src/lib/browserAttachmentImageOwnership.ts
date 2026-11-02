/** Only this versioned namespace participates in cross-context collection. Legacy files have no lease. */
const DIRECTORY = "starlane-image-previews-v1";
const OWNED_NAME = /^image-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const lockName = (name: string) => "starlane-image-preview-v1:" + name;
export type BrowserImagePreviewLease = {
  root: FileSystemDirectoryHandle;
  name: string;
  release(): Promise<void>;
};
function locks(): LockManager | undefined {
  return typeof navigator === "undefined" ? undefined : navigator.locks;
}
/** A live URL holds its browser-owned lock even while suspended. No timeout or heartbeat authorizes deletion. */
export async function ownBrowserImagePreview(
  root: FileSystemDirectoryHandle,
): Promise<BrowserImagePreviewLease> {
  const manager = locks();
  if (!manager?.request) {
    const name = "starlane-preview-" + crypto.randomUUID();
    return { root, name, release: () => root.removeEntry(name) };
  }
  const directory = await root.getDirectoryHandle(DIRECTORY, { create: true });
  const name = "image-" + crypto.randomUUID();
  let unlock!: () => void;
  let acquired!: () => void;
  let failed!: (error: unknown) => void;
  const held = new Promise<void>((resolve) => {
    unlock = resolve;
  });
  const ready = new Promise<void>((resolve, reject) => {
    acquired = resolve;
    failed = reject;
  });
  const request = manager.request(lockName(name), { mode: "exclusive" }, async () => {
    acquired();
    await held;
  });
  void request.catch(failed);
  await ready;
  let releasing: Promise<void> | null = null;
  return {
    root: directory,
    name,
    release: () =>
      (releasing ??= (async () => {
        try {
          await directory.removeEntry(name);
        } finally {
          unlock();
          await request;
        }
      })()),
  };
}
export type BrowserImageCollection = {
  removed: number;
  busy: number;
  examined: number;
  supported: boolean;
};
/** At most128 entries per pass. Exclusive ifAvailable grant is ownership proof; null means preserve. */
export async function collectAbandonedBrowserImagePreviews(
  root: FileSystemDirectoryHandle,
): Promise<BrowserImageCollection> {
  const result: BrowserImageCollection = { removed: 0, busy: 0, examined: 0, supported: false };
  const manager = locks();
  if (!manager?.request) return result;
  result.supported = true;
  let directory: FileSystemDirectoryHandle;
  try {
    directory = await root.getDirectoryHandle(DIRECTORY);
  } catch (error) {
    if ((error as DOMException)?.name === "NotFoundError") return result;
    throw error;
  }
  const entries = (
    directory as FileSystemDirectoryHandle & {
      entries(): AsyncIterableIterator<[string, FileSystemHandle]>;
    }
  ).entries();
  for await (const [name, entry] of entries) {
    if (result.examined++ >= 128) {
      result.examined = 128;
      break;
    }
    if (entry.kind !== "file" || !OWNED_NAME.test(name)) continue;
    await manager.request(
      lockName(name),
      { mode: "exclusive", ifAvailable: true },
      async (lock) => {
        if (!lock) {
          result.busy++;
          return;
        }
        try {
          await directory.removeEntry(name);
          result.removed++;
        } catch (error) {
          if ((error as DOMException)?.name !== "NotFoundError") throw error;
        }
      },
    );
  }
  return result;
}
