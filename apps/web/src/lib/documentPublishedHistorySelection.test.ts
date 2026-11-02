import {describe,expect,it,vi} from "vitest";
import {ApiError} from "./api";
import {createPublicationHistorySelection} from "./documentPublishedHistorySelection";
import {snapshotDetail,snapshotSummary} from "./documentPublishedHistory.fixtures";
function deferred<T>(){let resolve!:(value:T)=>void;let reject!:(reason:unknown)=>void;const promise=new Promise<T>((a,b)=>{resolve=a;reject=b;});return{promise,resolve,reject};}
function fixture(){const reader={list:vi.fn(),get:vi.fn(),raw:vi.fn()};const changed=vi.fn();return{reader,changed,selection:createPublicationHistorySelection(reader,changed)};}
describe("publication history selection fences",()=>{
 it("clears content immediately when page/context scope changes and ignores late list acknowledgement",async()=>{
  const f=fixture();f.selection.setScope("space","page");const old=deferred<typeof snapshotSummary[]>();f.reader.list.mockReturnValueOnce(old.promise);const first=f.selection.refresh();f.selection.setScope("other","next");f.reader.list.mockResolvedValueOnce([]);await f.selection.refresh();old.resolve([snapshotSummary]);await first;expect(f.selection.snapshot().records).toEqual([]);expect(f.selection.snapshot().listStatus).toBe("ready");
 });
 it("does not let a late previous selection replace the newer detail",async()=>{
  const f=fixture();f.selection.setScope("space","page");const a=deferred<typeof snapshotDetail>();f.reader.get.mockReturnValueOnce(a.promise);const first=f.selection.preview(snapshotSummary);const other={...snapshotSummary,id:"other:publication",capture_operation_id:"other",publication_operation_id:"other"};f.reader.get.mockResolvedValueOnce({...snapshotDetail,...other,content:snapshotDetail.content});await f.selection.preview(other);a.resolve(snapshotDetail);await first;expect(f.selection.snapshot().detail?.id).toBe(other.id);expect(f.reader.get).toHaveBeenLastCalledWith("space","page",other.id,other);
 });
 it("ignores raw bytes after closing or switching source before acknowledgement",async()=>{
  const f=fixture();f.selection.setScope("space","page");f.reader.get.mockResolvedValue(snapshotDetail);await f.selection.preview(snapshotSummary);const pending=deferred<{bytes:Uint8Array;text:string}>();f.reader.raw.mockReturnValueOnce(pending.promise);const read=f.selection.readRaw();f.selection.close();pending.resolve({bytes:new Uint8Array([1]),text:"old"});expect(await read).toBeNull();expect(f.selection.snapshot().raw).toBeNull();expect(f.selection.snapshot().previewOpen).toBe(false);
 });
 it("clears full history on late revocation and never presents denial as an empty list",async()=>{
  const f=fixture();f.selection.setScope("space","page");f.reader.list.mockResolvedValue([snapshotSummary]);await f.selection.refresh();f.reader.get.mockRejectedValue(new ApiError("Edit revoked",403));await f.selection.preview(snapshotSummary);const state=f.selection.snapshot();expect(state.listStatus).toBe("denied");expect(state.previewStatus).toBe("denied");expect(state.detail).toBeNull();expect(state.records).toEqual([]);
 });
 it("holds live history offline without stale rows and permits explicit fresh retry",async()=>{const f=fixture();f.selection.setScope("space","page");f.reader.list.mockRejectedValueOnce(Error("Reconnect to verify"));await f.selection.refresh();expect(f.selection.snapshot().listStatus).toBe("offline");f.reader.list.mockResolvedValueOnce([snapshotSummary]);await f.selection.refresh();expect(f.selection.snapshot().records).toEqual([snapshotSummary]);});
 it("does not expose internal mutable state or share caller snapshot mutation",async()=>{const f=fixture();f.selection.setScope("space","page");const input=structuredClone(snapshotSummary);const p=deferred<typeof snapshotDetail>();f.reader.get.mockReturnValueOnce(p.promise);const opening=f.selection.preview(input);input.content.title="mutated";const state=f.selection.snapshot();state.selected!.content.title="also mutated";expect(f.selection.snapshot().selected!.content.title).toBe(snapshotSummary.content.title);p.resolve(snapshotDetail);await opening;});
});
