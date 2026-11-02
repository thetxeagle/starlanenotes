import { describe, expect, it, vi } from "vitest";
import { unzipSync } from "fflate";
import { prepareStreamingZip, type ArchiveEntry } from "./streamingZip";
const entry = (name: string, bytes: Uint8Array): ArchiveEntry => ({ name, byteLength: bytes.length, transfer: async write => { await write(bytes); } });
function sink() {
  const parts: Uint8Array[] = [];
  return { parts, write: vi.fn(async (b: Uint8Array) => { parts.push(b.slice()); }), abort: vi.fn(async () => {parts.length = 0;}), close: vi.fn(async () => {}) };
}
function collect(parts: Uint8Array[]) { const result = new Uint8Array(parts.reduce((n,p)=>n+p.length,0));let at=0;for(const p of parts){result.set(p,at);at+=p.length;}return result; }
describe("streamed portable ZIP ownership", () => {
  it("produces independently readable exact entries and waits for publication", async () => {
    const s=sink(), first=new Uint8Array([0,1,255]), second=new Uint8Array([78,69,87]);
    const prepared=await prepareStreamingZip([entry("assets/old.bin",first),entry("assets/new.bin",second),entry("pages/a.md",new TextEncoder().encode("![old](../assets/old.bin)"))],s,async()=>{});
    expect(s.close).not.toHaveBeenCalled();const decoded=unzipSync(collect(s.parts));expect(decoded["assets/old.bin"]).toEqual(first);expect(decoded["assets/new.bin"]).toEqual(second);
    expect(prepared.byteLength).toBe(collect(s.parts).length);await prepared.publish();expect(s.close).toHaveBeenCalledOnce();
  });
  it("bounds actual large writes",async()=>{const s=sink(),bytes=new Uint8Array(200000).fill(7);await prepareStreamingZip([entry("a",bytes)],s,async()=>{});expect(Math.max(...s.write.mock.calls.map(c=>c[0].length))).toBe(65536);expect(unzipSync(collect(s.parts)).a).toEqual(bytes);});
  it("aborts changed authority after a sink write without closing",async()=>{const s=sink();let valid=true;s.write.mockImplementation(async()=>{valid=false;});await expect(prepareStreamingZip([entry("a",new Uint8Array(1))],s,async()=>{if(!valid)throw Error("revoked");})).rejects.toThrow("revoked");expect(s.abort).toHaveBeenCalledOnce();expect(s.close).not.toHaveBeenCalled();});
  it("aborts truncated selected bytes",async()=>{const s=sink();await expect(prepareStreamingZip([{...entry("a",new Uint8Array(1)),byteLength:2}],s,async()=>{})).rejects.toThrow("incomplete");expect(s.abort).toHaveBeenCalledOnce();});
  it("refuses unsafe names and unsupported ZIP64 before writing",async()=>{for(const e of [entry("../a",new Uint8Array()),{...entry("a",new Uint8Array()),byteLength:0xffffffff}]){const s=sink();await expect(prepareStreamingZip([e],s,async()=>{})).rejects.toThrow();expect(s.write).not.toHaveBeenCalled();}});
  it("retains unknown close outcome and prevents automatic retry",async()=>{const s=sink();s.close.mockRejectedValue(Error("lost"));const p=await prepareStreamingZip([entry("a",new Uint8Array())],s,async()=>{});await expect(p.publish()).rejects.toThrow("unknown");expect(s.abort).not.toHaveBeenCalled();await expect(p.publish()).rejects.toThrow("finalized");});
});
