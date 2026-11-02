import {describe,it,expect} from "vitest";
import {checkedNativeLocalCoverAcknowledgement,checkedNativeLocalCoverResumeProgress,checkedNativeLocalCoverDiscardAcknowledgement} from "./nativeLocalCover";
const op="11111111-1111-4111-8111-111111111111";
const original={operation_id:op,name:"same-name.png",mime_type:"image/png",byte_length:3,space_id:"space",document_id:"page",draft_id:"draft",original_binding:{context:{version:1 as const,serverBase:"local-vault",accountId:"local-user",sessionId:op,cacheScope:"local",storageMode:"local" as const,provider:"none" as const},root:"/original/vault",revision:"before"}};
const clone=<T>(v:T):T=>JSON.parse(JSON.stringify(v)) as T;
function ack(){
 const asset={id:`native-cover-${op}`,name:original.name,mime_type:original.mime_type,size:3,url:`starlane-local-attachment://${op}`,vault_path:`/original/vault/assets/local-covers/native-cover-${op}.bin`,source:"tauri" as const,space_id:"space",document_id:"page",created_at:"2026-10-14T00:00:00Z",native_local_version_provenance:{format:1,version_id:op,producer_operation_id:op,byte_length:3,binary_sha256:"a".repeat(64),original_filename:original.name,mime_type:original.mime_type,uploader:"local-user"}};
 return{format:1,operation_id:op,canonical_committed:true,asset,canonical_snapshot:{schema_version:1,spaces:[],folders:[],documents:[],drafts:[],assets:[asset],canonical_revision:"current"},receipt_json:JSON.stringify({operation_id:op,request:clone(original),canonical_committed:true,asset})};
}
describe("native local original acknowledgement",()=>{
 it("accepts current original with a historical winner and reordered native JSON keys",()=>{
  const value=ack();const request=Object.fromEntries(Object.entries(original).reverse());value.receipt_json=JSON.stringify({asset:value.asset,canonical_committed:true,request,operation_id:op});expect(checkedNativeLocalCoverAcknowledgement(value,original).asset.url).toBe(`starlane-local-attachment://${op}`);
 });
 it.each(["operation_id","name","mime_type","byte_length","space_id","document_id","draft_id","original_binding"])("rejects a receipt substituted for original %s",field=>{
  const value=ack(),request=clone(original) as unknown as Record<string,unknown>;request[field]="foreign";value.receipt_json=JSON.stringify({operation_id:op,request,canonical_committed:true,asset:value.asset});expect(()=>checkedNativeLocalCoverAcknowledgement(value,original)).toThrow();
 });
 it.each(["path","version","producer","length","filename","page","space","class","canonical","unknown-root"])("rejects incompatible %s evidence",change=>{
  const value=ack();switch(change){case"path":value.asset.vault_path="/borrowed/equal-bytes.png";break;case"version":value.asset.native_local_version_provenance.version_id="22222222-2222-4222-8222-222222222222";break;case"producer":value.asset.native_local_version_provenance.producer_operation_id="22222222-2222-4222-8222-222222222222";break;case"length":value.asset.size=4;break;case"filename":value.asset.name="foreign.png";break;case"page":value.asset.document_id="foreign";break;case"space":value.asset.space_id="foreign";break;case"class":Object.assign(value.asset,{server_version_provenance:{format:1}});break;case"canonical":value.canonical_snapshot.assets=[];break;default:Object.assign(value,{unexpected:true});}
  // Even self-consistent copied JSON cannot replace the expected original request.
  value.receipt_json=JSON.stringify({operation_id:op,request:original,canonical_committed:true,asset:value.asset});expect(()=>checkedNativeLocalCoverAcknowledgement(value,original)).toThrow();
 });
});

describe("original resume acknowledgement",()=>{
 const progress=()=>({format:1,operation_id:op,producer_token:"a".repeat(64),received:65536,next_index:1});
 it("accepts a lost client chunk ACK only at the same original native progress",()=>{expect(checkedNativeLocalCoverResumeProgress(progress(),op,65539,0)).toEqual({offset:65536,index:1,token:"a".repeat(64)});});
 it("rejects a client progress rollback and retains the original instead of restarting",()=>{expect(()=>checkedNativeLocalCoverResumeProgress({...progress(),received:0,next_index:0},op,65539,65536)).toThrow("retain evidence");});
 it.each([{...progress(),operation_id:"22222222-2222-4222-8222-222222222222"},{...progress(),received:65540},{...progress(),received:65536,next_index:2},{...progress(),producer_token:"/borrowed/path"},{...progress(),unexpected:"permission"}])("rejects substituted operation/progress/token shape",value=>{expect(()=>checkedNativeLocalCoverResumeProgress(value,op,65539,0)).toThrow();});
});


describe("original native partial discard acknowledgement",()=>{
 const stamp=(length:number,links:number)=>({device:1,inode:2,uid:1000,mode:33152,links,length,ctime:1,ctime_nsec:0,mtime:1,mtime_nsec:0});
 async function discarded(){
  const requestJson=JSON.stringify(original);const bytes=await crypto.subtle.digest("SHA-256",new TextEncoder().encode(requestJson));const hash=Array.from(new Uint8Array(bytes),byte=>byte.toString(16).padStart(2,"0")).join("");const body={format:1,state:"discarded_original_payload",operation_id:op,request_sha256:hash,plan_sha256:"a".repeat(64),payload_after:stamp(3,0),binding:clone(original.original_binding)};const raw=JSON.stringify(body);const receipt={format:1,state:"discarded_original_payload",operation_id:op,native_completion_json:raw,native_completion_stamp:stamp(new TextEncoder().encode(raw).length,1)};return{format:1,state:"discarded_original_payload",operation_id:op,request_sha256:hash,request_json:requestJson,receipt_json:JSON.stringify(receipt)};
 }
 it("checks byte-exact request hash and exact original context before releasing a cached File",async()=>{expect(await checkedNativeLocalCoverDiscardAcknowledgement(await discarded(),original)).toEqual({discardedBytes:3});});
 it.each(["operation","request","hash","body-state","payload-links","payload-length","context","native-result-length","native-result-alias","extra-authority"])("refuses substituted %s acknowledgement",async fault=>{
  const value=await discarded(),receipt=JSON.parse(value.receipt_json),body=JSON.parse(receipt.native_completion_json);
  switch(fault){case"operation":value.operation_id="22222222-2222-4222-8222-222222222222";break;case"request":value.request_json=JSON.stringify({...original,name:"foreign.png"});break;case"hash":value.request_sha256="0".repeat(64);body.request_sha256=value.request_sha256;break;case"body-state":body.state="retained_review";break;case"payload-links":body.payload_after.links=1;break;case"payload-length":body.payload_after.length=4;break;case"context":body.binding.context.sessionId="foreign";break;case"native-result-alias":receipt.native_completion_stamp.links=2;break;case"extra-authority":Object.assign(body,{serve_authority:true});break;}
  receipt.native_completion_json=JSON.stringify(body);receipt.native_completion_stamp.length=new TextEncoder().encode(receipt.native_completion_json).length+(fault==="native-result-length"?1:0);value.receipt_json=JSON.stringify(receipt);await expect(checkedNativeLocalCoverDiscardAcknowledgement(value,original)).rejects.toThrow();
 });
});
