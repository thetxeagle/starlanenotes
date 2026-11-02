import { afterEach, describe, expect, it, vi } from "vitest";
import { dataClient } from "@/lib/dataClient";
import { createAdminArchiveRecoverySession } from "./AdminPage";
import type { NativeArchiveBinding, NativeArchiveInventoryRow } from "@/lib/nativeArchiveReferences";
const binding: NativeArchiveBinding={provider:"none",database_path:"/original/.starlane/starlane-notes.sqlite3",database_instance_id:"a".repeat(64)};
const row: NativeArchiveInventoryRow={operation_id:"original-operation",request_fingerprint:"b".repeat(64),canonical_committed:false,retained_abandoned:false,intent:{original:"raw"}};
afterEach(()=>vi.restoreAllMocks());
describe("actual Storage archive recovery controller",()=>{
  it("inspects the exact original evidence and makes no abandonment write before confirmation",async()=>{
    const inspect=vi.spyOn(dataClient,"inspectLocalVaultArchiveImport").mockResolvedValue({binding,operation_id:row.operation_id,request_fingerprint:row.request_fingerprint,intent:row.intent,receipt:null,resolution:null,current_snapshot:null});
    const abandon=vi.spyOn(dataClient,"abandonLocalVaultArchiveImport").mockResolvedValue();
    const original=JSON.stringify({binding,row});const session=await createAdminArchiveRecoverySession(binding,row,()=>{});
    expect(inspect).toHaveBeenCalledWith(binding,row.operation_id,row.request_fingerprint);expect(abandon).not.toHaveBeenCalled();expect(JSON.stringify({binding,row})).toBe(original);
    await session.abandon();expect(abandon).toHaveBeenCalledTimes(1);expect(abandon).toHaveBeenCalledWith(binding,row.operation_id,row.request_fingerprint);await expect(session.abandon()).rejects.toThrow(/resolution request/);expect(abandon).toHaveBeenCalledTimes(1);
  });
  it("rejects original context changes after evidence inspection and before confirmation",async()=>{
    let current=true;vi.spyOn(dataClient,"inspectLocalVaultArchiveImport").mockResolvedValue({binding,operation_id:row.operation_id,request_fingerprint:row.request_fingerprint,intent:row.intent,receipt:null,resolution:null,current_snapshot:null});const abandon=vi.spyOn(dataClient,"abandonLocalVaultArchiveImport").mockResolvedValue();
    const session=await createAdminArchiveRecoverySession(binding,row,()=>{if(!current)throw new Error("Original workspace changed");});current=false;await expect(session.abandon()).rejects.toThrow(/workspace changed/);expect(abandon).not.toHaveBeenCalled();
  });
  it("does not offer abandonment when the original operation already has a receipt",async()=>{
    vi.spyOn(dataClient,"inspectLocalVaultArchiveImport").mockResolvedValue({binding,operation_id:row.operation_id,request_fingerprint:row.request_fingerprint,intent:row.intent,receipt:{schema_version:1,canonical_committed:true,operation_id:row.operation_id,request_fingerprint:row.request_fingerprint,binding,canonical_revision:"A",candidate:{schema_version:1,spaces:[],folders:[],documents:[],assets:[],templates:[]},reference_changes:[],versions:[]},resolution:null,current_snapshot:null});
    const abandon=vi.spyOn(dataClient,"abandonLocalVaultArchiveImport").mockResolvedValue();await expect(createAdminArchiveRecoverySession(binding,row,()=>{})).rejects.toThrow(/no longer unresolved/);expect(abandon).not.toHaveBeenCalled();
  });
});
