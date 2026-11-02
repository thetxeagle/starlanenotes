export interface ArchiveWritable {
  write(bytes: Uint8Array): Promise<void>;
  abort(reason?: unknown): Promise<void>;
  close(): Promise<void>;
}
export interface ArchiveEntry {
  name: string;
  byteLength: number;
  transfer(write: (bytes: Uint8Array) => Promise<void>): Promise<void>;
}
export interface PreparedArchive {
  byteLength: number;
  publish(): Promise<void>;
  discard(): Promise<void>;
}
const MAX = 0xffffffff, CHUNK = 65536;
const crcTable = new Uint32Array(256);
for (let n = 0; n < 256; n++) {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  crcTable[n] = c;
}
function fields(size: number, values: [number, number, number][]): Uint8Array {
  const bytes = new Uint8Array(size), view = new DataView(bytes.buffer);
  for (const [offset, width, value] of values) {
    if (width === 2) view.setUint16(offset, value, true);
    else view.setUint32(offset, value, true);
  }
  return bytes;
}
/** Stored ZIP; no renderer binary aggregation. Caller owns destination selection and outcome journaling. */
export async function prepareStreamingZip(entries: readonly ArchiveEntry[], destination: ArchiveWritable,
  verifyCurrent: () => Promise<void>): Promise<PreparedArchive> {
  let finalized = false, offset = 0;
  const abort = async (reason?: unknown) => {
    if (!finalized) { finalized = true; await destination.abort(reason); }
  };
  try {
    // Central directory and entry names are bounded independently of streamed body sizes.
    if (entries.length > 4096) throw new Error("Archive exceeds bounded entry count");
    const names = new Set<string>();
    const prepared = entries.map(entry => {
      const name = new TextEncoder().encode(entry.name);
      if (!entry.name || entry.name.startsWith("/") || entry.name.includes("\\") ||
        entry.name.split("/").some(part => !part || part === "." || part === "..") ||
        /[\x00-\x1f\x7f]/.test(entry.name) || name.length > 1024 || names.has(entry.name))
        throw new Error("Archive requires unique safe bounded entry names");
      if (!Number.isSafeInteger(entry.byteLength) || entry.byteLength < 0 || entry.byteLength > MAX)
        throw new Error("Archive entry size requires ZIP64 or is invalid");
      names.add(entry.name);
      return {entry, name, crc: 0, start: 0};
    });
    const expected = prepared.reduce((sum, p) => sum + p.entry.byteLength + 30 + p.name.length + 16 + 46 + p.name.length, 22);
    if (expected > MAX) throw new Error("Archive exceeds supported ZIP size; ZIP64 required");
    const write = async (bytes: Uint8Array) => {
      for (let at = 0; at < bytes.length; at += CHUNK) {
        await verifyCurrent();
        const part = bytes.subarray(at, at + CHUNK);
        await destination.write(part);
        offset += part.length;
        await verifyCurrent();
      }
    };
    for (const p of prepared) {
      p.start = offset;
      await write(fields(30, [[0,4,0x04034b50],[4,2,20],[6,2,0x0808],[12,2,33],[26,2,p.name.length]]));
      await write(p.name);
      let crc = 0xffffffff, size = 0;
      await p.entry.transfer(async bytes => {
        size += bytes.length;
        if (size > p.entry.byteLength) throw new Error("Archive entry exceeds exact selected length");
        for (const byte of bytes) crc = crcTable[(crc ^ byte) & 255] ^ (crc >>> 8);
        await write(bytes);
      });
      await verifyCurrent();
      if (size !== p.entry.byteLength) throw new Error("Archive entry is incomplete");
      p.crc = (crc ^ 0xffffffff) >>> 0;
      await write(fields(16, [[0,4,0x08074b50],[4,4,p.crc],[8,4,size],[12,4,size]]));
    }
    const directory = offset;
    for (const p of prepared) {
      await write(fields(46, [[0,4,0x02014b50],[4,2,20],[6,2,20],[8,2,0x0808],[14,2,33],
        [16,4,p.crc],[20,4,p.entry.byteLength],[24,4,p.entry.byteLength],[28,2,p.name.length],[42,4,p.start]]));
      await write(p.name);
    }
    const directoryLength = offset - directory;
    await write(fields(22, [[0,4,0x06054b50],[8,2,prepared.length],[10,2,prepared.length],
      [12,4,directoryLength],[16,4,directory]]));
    await verifyCurrent();
    return { byteLength: offset, discard: () => abort(), publish: async () => {
      if (finalized) throw new Error("Archive already finalized");
      try { await verifyCurrent(); } catch (error) { await abort(error); throw error; }
      finalized = true;
      try { await destination.close(); await verifyCurrent(); }
      catch { throw new Error("Archive publication outcome unknown; inspect selected destination before retrying"); }
    }};
  } catch (error) { await abort(error).catch(() => undefined); throw error; }
}
