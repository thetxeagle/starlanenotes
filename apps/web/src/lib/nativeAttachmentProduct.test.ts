import {describe,expect,it} from "vitest";
import {attachmentHistoryHead} from "./nativeAttachmentProduct";
const old="11111111-1111-4111-8111-111111111111",head="22222222-2222-4222-8222-222222222222";
describe("explicit authorized attachment history head",()=>{
 it("keeps visible-count one separate from a proven current revision two",()=>{expect(attachmentHistoryHead({schema_version:2,total:1,versions:[{version_id:head,version_number:2}],current_head:{version_id:head,revision:2}})).toEqual({version_id:head,revision:2});});
 it("does not infer a hidden current head from a visible old version or count",()=>{expect(attachmentHistoryHead({schema_version:2,total:1,versions:[{version_id:old,version_number:1}],current_head:null})).toBeNull();expect(()=>attachmentHistoryHead({schema_version:2,total:1,versions:[{version_id:old,version_number:1}]})).toThrow("Explicit authorized");});
 it("retains explicit unversioned legacy total/revision semantics without changing records",()=>{const original={total:2,versions:[{version_id:old,version_number:1},{version_id:head,version_number:2}]};const raw=JSON.stringify(original);expect(attachmentHistoryHead(original)).toEqual({version_id:head,revision:2});expect(JSON.stringify(original)).toBe(raw);});
 it("does not offer replacement before a legacy last page contains its exact head",()=>{expect(attachmentHistoryHead({total:2,versions:[{version_id:old,version_number:1}]})).toBeNull();});
 it.each([{schema_version:3,total:1,versions:[],current_head:null},{schema_version:2,total:1,versions:[],current_head:{version_id:"bad",revision:2}},{schema_version:2,total:1,versions:[{version_id:head,version_number:2}],current_head:{version_id:head,revision:1}}])("refuses unsupported or contradictory head metadata",v=>{expect(()=>attachmentHistoryHead(v)).toThrow();});
});
