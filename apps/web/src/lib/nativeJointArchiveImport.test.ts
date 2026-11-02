import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LocalVaultSnapshot } from "./localVault";
import type { NativeArchivePlan, NativeArchiveReceipt } from "./nativeArchiveReferences";
const clone=<T>(value:T):T=>value === undefined ? value : JSON.parse(JSON.stringify(value));
const binding={provider:"none",database_path:"/original/.starlane/starlane-notes.sqlite3",database_instance_id:"b".repeat(64)};
const descriptor={schema_version:1,provider:"none",store_kind:"sqlite",database_path:binding.database_path,supports_snapshot_bridge:true,supports_atomic_snapshot_commit:true,supports_operation_queue:true,supports_lossless_replay_queue:true,supports_reviewed_move_evidence:true,supports_reviewed_move_destination_binding:true};
function initial():LocalVaultSnapshot {return {schema_version:1,spaces:[{id:"local-space",name:"Personal Space",created_by:"local-user",created_at:"old",updated_at:"old"}],folders:[],documents:[{id:"local-welcome",space_id:"local-space",title:"Old",content:"old canonical body",published_at:"old",created_by:"local-user",created_at:"old",updated_at:"old"}],drafts:[],assets:[],templates:[],property_schemas:[],property_values:[],sync_state:{state:"idle",updated_at:"old",queued_operations:[]},canonical_revision:"old",mirror_pending:false};}
let client:typeof import("./dataClient")["dataClient"];
let current:LocalVaultSnapshot;
let imported:LocalVaultSnapshot;
let plan:NativeArchivePlan;
let receipt:NativeArchiveReceipt|undefined;
let originalRequest:unknown;
let hook:(command:string)=>void;
let loseReply:boolean;
let invoke:ReturnType<typeof vi.fn>;
let genericDescriptor:unknown;
let changePlan:(value:NativeArchivePlan)=>void;
let changeReceipt:(value:NativeArchiveReceipt)=>void;
beforeEach(async()=>{
  vi.resetModules();vi.useFakeTimers({toFake:["Date"]});vi.setSystemTime(new Date("2026-10-09T12:00:00Z"));
  const storage=new Map<string,string>();const local={getItem:(key:string)=>storage.get(key)??null,setItem:(key:string,value:string)=>storage.set(key,value),removeItem:(key:string)=>storage.delete(key),clear:()=>storage.clear(),key:(index:number)=>[...storage.keys()][index]??null,get length(){return storage.size;}};
  current=initial();imported=clone(current);delete imported.canonical_revision;delete imported.mirror_pending;receipt=undefined;originalRequest=undefined;loseReply=false;hook=()=>{};genericDescriptor=descriptor;changePlan=()=>{};changeReceipt=()=>{};
  invoke=vi.fn(async(command:string,args?:Record<string,unknown>)=>{
    hook(command);
    if(command==="describe_native_local_database"){if(genericDescriptor instanceof Error)throw genericDescriptor;return genericDescriptor;}
    if(command==="read_native_local_database_snapshot")return clone(current);
    if(command==="sync_native_local_database_mirror")return undefined;
    if(command==="commit_native_local_database_snapshot"){current={...(args!.snapshot as LocalVaultSnapshot),canonical_revision:"setup",mirror_pending:false};return {canonical_committed:true,canonical_revision:"setup",mirror_pending:false,mirror_error:null};}
    if(command==="list_native_vault_archive_imports")return {binding,operations:originalRequest?[{operation_id:"prepared",request_fingerprint:"f".repeat(64),canonical_committed:!!receipt,retained_abandoned:false,intent:clone(originalRequest)}]:[],next_cursor:null};
    if(command==="prepare_vault_archive_import")return {handle:"prepared",snapshot_json:JSON.stringify(imported)};
    if(command==="cancel_vault_archive_import")return undefined;
    if(command==="plan_native_vault_archive_import"){
      plan={schema_version:1,operation_id:"prepared",plan_digest:"a".repeat(64),binding,expected_revision:current.canonical_revision??null,original_snapshot_json:JSON.stringify(imported),destination_snapshot:clone(current),versions:imported.assets.flatMap((asset,index)=>asset.source==="tauri"?[{index,id:asset.id,size:4,sha256:"c".repeat(64),vault_path:`/original/assets/version-${asset.id}`,url:`asset://localhost/original/assets/version-${asset.id}`}]:[])};changePlan(plan);return clone(plan);
    }
    if(command==="commit_native_vault_archive_import"){
      originalRequest={plan:clone(plan),candidate:clone(args!.candidate),reference_changes:clone(args!.referenceChanges)};
      receipt={schema_version:1,canonical_committed:true,operation_id:"prepared",request_fingerprint:"f".repeat(64),binding,canonical_revision:"import-A",candidate:clone(args!.candidate as LocalVaultSnapshot),reference_changes:clone(args!.referenceChanges as NativeArchiveReceipt["reference_changes"]),versions:clone(plan.versions)};
      changeReceipt(receipt);current={...clone(receipt.candidate),canonical_revision:"import-A",mirror_pending:false};if(loseReply)throw new Error("QA lost acknowledgement after native commit");return {...clone(receipt),current_snapshot:clone(current),mirror_pending:false};
    }
    if(command==="inspect_native_vault_archive_import")return {binding,operation_id:"prepared",request_fingerprint:"f".repeat(64),intent:clone(originalRequest),receipt:clone(receipt),resolution:null,current_snapshot:clone(current)};
    throw new Error(`Unexpected native command ${command}`);
  });
  (globalThis as {window?:unknown}).window={addEventListener:vi.fn(),localStorage:local,__TAURI_INTERNALS__:{invoke}};(globalThis as {localStorage?:unknown}).localStorage=local;
  const launch=await import("./launchMode");const providers=await import("./storageProviders");launch.setCurrentLaunchMode("local");providers.setSyncProvider("none");client=(await import("./dataClient")).dataClient;
  await client.exportLocalVaultSnapshot();invoke.mockClear();
});
afterEach(()=>{vi.useRealTimers();vi.restoreAllMocks();delete(globalThis as {window?:unknown}).window;delete(globalThis as {localStorage?:unknown}).localStorage;});
describe("actual native joint archive importer",()=>{
  it("consumes zero asset prepare/plan/joint receipt without generic save or restoration fallback",async()=>{
    genericDescriptor = new Error("Lossless descriptor unavailable on empty queue");
    await client.importLocalVaultArchive("/selected.zip");expect(invoke.mock.calls.map(([name])=>name)).toEqual(["list_native_vault_archive_imports","prepare_vault_archive_import","plan_native_vault_archive_import","commit_native_vault_archive_import","cancel_vault_archive_import"]);expect(receipt?.candidate).toEqual(imported);expect((await client.exportLocalVaultSnapshot()).documents[0].content).toBe(imported.documents[0].content);
  });
  it("preserves raw property strings IDs and metadata and remaps real Markdown plus cover/mixed asset indices",async()=>{
    imported.property_schemas=[{id:"raw:schema",space_id:"local-space",name:" Raw ",property_type:"select",options:{options:[" one "]},sort_order:1}];imported.property_values=[{id:"raw:value/id",document_id:"local-welcome",property_id:"raw:schema",value:" one "}];imported.assets=[{id:"remote",name:"remote",size:1,source:"browser",url:"https://example.test/remote",created_at:"raw"},{id:"media",name:"file.png",size:99,source:"tauri",url:"asset://localhost/original",vault_path:"/original",created_at:"raw"}];imported.documents[0].content="🌙 ![x](asset://localhost/original) and <asset://localhost/original>";imported.documents[0].cover_image="asset://localhost/original";
    await client.importLocalVaultArchive("/selected.zip");expect(receipt?.candidate.property_schemas).toEqual(imported.property_schemas);expect(receipt?.candidate.property_values).toEqual(imported.property_values);expect(receipt?.candidate.assets[0]).toEqual(imported.assets[0]);expect(receipt?.candidate.assets[1]).toEqual({...imported.assets[1],size:4,vault_path:"/original/assets/version-media",url:"asset://localhost/original/assets/version-media"});expect(receipt?.candidate.documents[0].content).toBe("🌙 ![x](asset://localhost/original/assets/version-media) and <asset://localhost/original/assets/version-media>");expect(receipt?.reference_changes).toHaveLength(3);
  });
  it("retains unrelated raw destination and imported queue origins and exact duplicates",async()=>{
    const op={id:"original-operation",entity_type:"document",entity_id:"local-welcome",action:"update_content",queued_at:"old",replay_provider:"google-drive" as const,server_context:{server_base:"https://original.invalid",account_id:"original",session_scope:"sid"},payload:{content:"Original\r\nbody",source:{published_at:null}},future_operation:{retained:true}};
    current.sync_state={state:"idle",updated_at:"old",queued_operations:[op]};imported.sync_state={state:"idle",updated_at:"old",future_state:{retained:true},queued_operations:[clone(op)]} as typeof imported.sync_state;
    await client.importLocalVaultArchive("/selected.zip");expect(receipt?.candidate.sync_state?.queued_operations).toEqual([op]);expect(receipt?.candidate.sync_state).toMatchObject({future_state:{retained:true}});
  });
  it("reconciles lost ACK only with full original receipt and publishes current later B",async()=>{
    loseReply=true;hook=command=>{if(command==="inspect_native_vault_archive_import")current={...current,canonical_revision:"later-B",documents:current.documents.map(row=>({...row,content:"later canonical B"}))};};await client.importLocalVaultArchive("/selected.zip");expect(receipt?.candidate.documents[0].content).toBe(imported.documents[0].content);expect((await client.exportLocalVaultSnapshot()).documents[0].content).toBe("later canonical B");expect(invoke.mock.calls.filter(([name])=>name==="commit_native_vault_archive_import")).toHaveLength(1);
  });
  it.each(["invalid schema type","fractional schema order","invalid number value","foreign schema","invalid select value","invalid relation target"])("preflights %s before private extraction plan and canonical commit",async(kind)=>{
    await client.createPropertySchema("local-space", { name:"Retained", property_type:"text" });
    invoke.mockClear();
    const before=await client.exportLocalVaultSnapshot();const cache=await client.listPropertySchemas("local-space");const persist=vi.spyOn(client,"importLocalVaultSnapshot");invoke.mockClear();imported.property_schemas=[{id:"number",space_id:kind==="foreign schema"?"other":"local-space",name:"Number",property_type:kind==="invalid select value"?"select":kind==="invalid relation target"?"relation":"number",sort_order:kind==="fractional schema order"?1.5:0,...(kind==="invalid schema type"?{property_type:null}:{}),...(kind==="invalid select value"?{options:["allowed"]}:{})}] as LocalVaultSnapshot["property_schemas"];imported.property_values=[{id:"raw:value",document_id:"local-welcome",property_id:"number",value:kind==="invalid number value"?"Infinity":kind==="invalid select value"?"hidden":kind==="invalid relation target"?"missing-target":"01"}];await expect(client.importLocalVaultArchive("/selected.zip")).rejects.toThrow();expect(invoke.mock.calls.map(([name])=>name)).toEqual(["list_native_vault_archive_imports","prepare_vault_archive_import","cancel_vault_archive_import"]);expect(await client.exportLocalVaultSnapshot()).toEqual(before);expect(await client.listPropertySchemas("local-space")).toEqual(cache);expect(persist).not.toHaveBeenCalled();
  });
  it.each(["bad-state","bad-queue","missing-id","missing-time","duplicate","bad-provider","bad-context"])("rejects malformed raw archive queue %s before plan or save",async(kind)=>{
    const before=await client.exportLocalVaultSnapshot();const persist=vi.spyOn(client,"importLocalVaultSnapshot");invoke.mockClear();const op:Record<string,unknown>={id:"op",entity_type:"document",entity_id:"local-welcome",action:"update_content",queued_at:"old",payload:{content:"Original\r\nbody"}};const sync:Record<string,unknown>={state:"idle",updated_at:"old",queued_operations:[op]};if(kind==="bad-state")sync.state="invalid";if(kind==="bad-queue")sync.queued_operations=null;if(kind==="missing-id")delete op.id;if(kind==="missing-time")delete op.queued_at;if(kind==="duplicate")sync.queued_operations=[op,op];if(kind==="bad-provider")op.replay_provider="unrecognized";if(kind==="bad-context")op.server_context={account_id:"old"};imported.sync_state=sync as unknown as LocalVaultSnapshot["sync_state"];
    await expect(client.importLocalVaultArchive("/selected.zip")).rejects.toThrow();expect(invoke.mock.calls.map(([name])=>name)).toEqual(["list_native_vault_archive_imports","prepare_vault_archive_import","cancel_vault_archive_import"]);expect(await client.exportLocalVaultSnapshot()).toEqual(before);expect(persist).not.toHaveBeenCalled();
  });
  it.each(["provider","mode"])("retains whole cache and queue when %s changes during capability wait",async(kind)=>{
    const before=await client.exportLocalVaultSnapshot();imported.sync_state={state:"idle",updated_at:"old",queued_operations:[{id:"op",entity_type:"document",entity_id:"local-welcome",action:"update_content",queued_at:"old"}]};const providers=await import("./storageProviders"),launch=await import("./launchMode");hook=command=>{if(command==="describe_native_local_database"){if(kind==="provider")providers.setSyncProvider("google-drive");else launch.setCurrentLaunchMode("server");}};await expect(client.importLocalVaultArchive("/selected.zip")).rejects.toThrow(/context changed/);providers.setSyncProvider("none");launch.setCurrentLaunchMode("local");expect(await client.exportLocalVaultSnapshot()).toEqual(before);expect(invoke.mock.calls.some(([name])=>name==="commit_native_vault_archive_import"||name==="restore_validated_vault_archive_import")).toBe(false);
  });
  it("fails old native command availability without invoking the unsafe legacy restore",async()=>{
    const before=await client.exportLocalVaultSnapshot();invoke.mockClear();invoke.mockImplementation(async(name:string)=>{throw new Error(`Unknown safe command ${name}`);});await expect(client.importLocalVaultArchive("/legacy.zip")).rejects.toThrow(/Unknown safe command/);expect(invoke.mock.calls.map(([name])=>name)).toEqual(["list_native_vault_archive_imports"]);invoke.mockClear();invoke.mockImplementation(async(name:string)=>name==="read_native_local_database_snapshot"?clone(current):descriptor);expect(await client.exportLocalVaultSnapshot()).toEqual(before);
  });
  it("rejects a conflicting original queue ID before joint dispatch",async()=>{
    const op={id:"same",entity_type:"document",entity_id:"local-welcome",action:"update_content",queued_at:"old",payload:{content:"A"}};current.sync_state={state:"idle",updated_at:"old",queued_operations:[op]};imported.sync_state={state:"idle",updated_at:"old",queued_operations:[{...op,payload:{content:"other"}}]};await expect(client.importLocalVaultArchive("/selected.zip")).rejects.toThrow(/queue identities conflict/);expect(invoke.mock.calls.some(([name])=>name==="commit_native_vault_archive_import")).toBe(false);
  });
  it.each([
    ["unknown command",new Error("Unknown describe command")], ["null",null], ["array",[]], ["empty",{}],
    ["missing lossless",{...descriptor,supports_lossless_replay_queue:undefined}], ["string lossless",{...descriptor,supports_lossless_replay_queue:"true"}],
    ["string schema",{...descriptor,schema_version:"1"}], ["future schema",{...descriptor,schema_version:2}],
    ["foreign provider",{...descriptor,provider:"google-drive"}], ["legacy store",{...descriptor,store_kind:"snapshot-json"}],
    ["non-atomic",{...descriptor,supports_atomic_snapshot_commit:false}], ["no queue",{...descriptor,supports_operation_queue:false}],
  ])("rejects archive lossless descriptor %s without plan/save/cache mutation",async(_name,value)=>{
    const before=await client.exportLocalVaultSnapshot();const persist=vi.spyOn(client,"importLocalVaultSnapshot");
    imported.sync_state={state:"idle",updated_at:"old",queued_operations:[{id:"old-op",entity_type:"document",entity_id:"local-welcome",action:"update_content",queued_at:"old-time",payload:null}]};
    invoke.mockClear();genericDescriptor=value;await expect(client.importLocalVaultArchive("/selected.zip")).rejects.toThrow(/Update the desktop shell/);
    expect(invoke.mock.calls.map(([name])=>name)).toEqual(["list_native_vault_archive_imports","prepare_vault_archive_import","describe_native_local_database","cancel_vault_archive_import"]);
    genericDescriptor=descriptor;expect(await client.exportLocalVaultSnapshot()).toEqual(before);expect(persist).not.toHaveBeenCalled();
  });
  it("retains unresolved original intent and blocks another import before preparing files",async()=>{
    const before=await client.exportLocalVaultSnapshot();invoke.mockClear();originalRequest={original:"pending request"};
    await expect(client.importLocalVaultArchive("/selected.zip")).rejects.toThrow(/prior archive import/);
    expect(invoke.mock.calls.map(([name])=>name)).toEqual(["list_native_vault_archive_imports"]);
    expect(await client.exportLocalVaultSnapshot()).toEqual(before);
  });
  it("inspects restart evidence without saving or adopting its historical candidate",async()=>{
    originalRequest={original:"retained request"};const before=await client.exportLocalVaultSnapshot();
    const inventory=await client.listLocalVaultArchiveImports();
    const inspected=await client.inspectLocalVaultArchiveImport(binding,"prepared","f".repeat(64));
    expect(inventory.operations[0].intent).toEqual(originalRequest);expect(inspected.intent).toEqual(originalRequest);
    expect(invoke.mock.calls.some(([name])=>name==="commit_native_vault_archive_import"||name==="commit_native_local_database_snapshot")).toBe(false);
    expect(await client.exportLocalVaultSnapshot()).toEqual(before);
  });
  it("rejects native plan original identity drift before joint dispatch or cache change",async()=>{
    const before=await client.exportLocalVaultSnapshot();changePlan=value=>{value.original_snapshot_json="{}";};
    await expect(client.importLocalVaultArchive("/selected.zip")).rejects.toThrow();
    expect(invoke.mock.calls.some(([name])=>name==="commit_native_vault_archive_import")).toBe(false);expect(await client.exportLocalVaultSnapshot()).toEqual(before);
  });
  it("refuses mismatched full receipt after a commit without inventing acknowledgement",async()=>{
    const before=await client.getDocumentContent("local-space","local-welcome");changeReceipt=value=>{value.reference_changes=[{collection:"documents",index:0,field:"content",offset_unit:"utf16_code_units",start:0,end:1,original:"x",replacement:"y"}];};
    await expect(client.importLocalVaultArchive("/selected.zip")).rejects.toThrow();
    expect(invoke.mock.calls.filter(([name])=>name==="commit_native_vault_archive_import")).toHaveLength(1);
    // RAM remains the original editor state even when the real host has an unknown committed outcome.
    expect(await client.getDocumentContent("local-space","local-welcome")).toEqual(before);
  });
  it("native reset refusal retains exact RAM queue history and disk evidence without fallback",async()=>{
    current.sync_state={state:"idle",updated_at:"raw",queued_operations:[{id:"retained-work",entity_type:"document",entity_id:"local-welcome",action:"update_content",queued_at:"original",payload:{content:"retained bytes"},replay_provider:"none"}]};
    current.reviewed_move_records=[{id:"12345678-1234-1234-1234-123456789abc",schema_version:1,provider:"none",actor_id:"local-user",moved_at:"2026-10-09T12:00:00Z",intent:{source_space_id:"local-space",root:{kind:"document",id:"local-welcome"},destination:{space_id:"destination",folder_id:null,parent_id:null}},decisions:{properties:[],workflows:[],tags:[],relations:[],identifiers:[],maintenance:[],detach_hierarchy:[],preserve_previous_draft_placement:true},before:{documents:clone(current.documents)},after:{documents:clone(current.documents)},preserved:[],removed:[]}];
    const {createLocalVaultAdapter}=await import("./localVault");const vault=createLocalVaultAdapter("none");
    const before=await vault.loadSnapshot(),disk=clone(current);
    const storage=window.localStorage;const beforeStorage=Array.from({length:storage.length},(_,index)=>{const key=storage.key(index)!;return [key,storage.getItem(key)];});
    invoke.mockClear();hook=command=>{if(command==="reset_native_local_database")throw new Error("Archive intent retained; reset refused");};
    await expect(vault.reset()).rejects.toThrow(/reset refused/);
    expect(await vault.loadSnapshot()).toEqual(before);expect(current).toEqual(disk);
    expect(Array.from({length:storage.length},(_,index)=>{const key=storage.key(index)!;return [key,storage.getItem(key)];})).toEqual(beforeStorage);
    expect(invoke.mock.calls.map(([name])=>name)).toEqual(["reset_native_local_database"]);
  });
  it("holds remapped queued body/source before joint dispatch and preserves raw work",async()=>{
    const before=await client.exportLocalVaultSnapshot();imported.assets=[{id:"media",name:"file.png",size:99,source:"tauri",url:"asset://localhost/original",vault_path:"/original",created_at:"raw"}];
    imported.documents[0].content="![x](asset://localhost/original)";
    imported.sync_state={state:"idle",updated_at:"old",queued_operations:[{id:"body-op",entity_type:"document",entity_id:"local-welcome",action:"update_content",queued_at:"original",payload:{content:"![x](asset://localhost/original)",source:{raw:"original"}}}]};
    const original=clone(imported);await expect(client.importLocalVaultArchive("/selected.zip")).rejects.toThrow(/Queued body or source/);
    expect(imported).toEqual(original);expect(invoke.mock.calls.some(([name])=>name==="commit_native_vault_archive_import")).toBe(false);expect(await client.exportLocalVaultSnapshot()).toEqual(before);
  });

});
