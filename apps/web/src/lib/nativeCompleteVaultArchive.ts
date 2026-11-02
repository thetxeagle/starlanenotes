import {captureNativeArchiveLocalSession,openCurrentNativeArchiveOutcomeReview} from "./nativeArchiveCurrentContext";
import type {NativeArchiveReviewBinding,NativeArchiveOutcomeReport} from "./nativeArchiveOutcomeReview";
const KEY="starlane_native_complete_archive_outcomes_v1";
interface Note{version:1;attempt:string;binding:NativeArchiveReviewBinding;parent:string;leaf:string;state:"publication_unknown"|"published"|"reviewed";operation?:string}
const clone=<T>(v:T):T=>JSON.parse(JSON.stringify(v)) as T;
const equal=(a:unknown,b:unknown)=>JSON.stringify(a,(_k,v:unknown)=>v&&typeof v==="object"&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v)===JSON.stringify(b,(_k,v:unknown)=>v&&typeof v==="object"&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);
const uuid=/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;
const PREFIX=KEY+":";const REVIEW=PREFIX+"legacy-reviewed:";
function checkedNote(n:Note):void{
 if(!n||Object.keys(n).some(k=>!["version","attempt","binding","parent","leaf","state","operation"].includes(k))||n.version!==1||!uuid.test(n.attempt)||!["publication_unknown","published","reviewed"].includes(n.state)||typeof n.parent!=="string"||!n.parent.startsWith("/")||n.parent==="/"||n.parent.split("/").slice(1).some(p=>!p||p==="."||p==="..")||typeof n.leaf!=="string"||!n.leaf||n.leaf.includes("/")||!n.binding||Object.keys(n.binding).length!==3||typeof n.binding.root!=="string"||!n.binding.root.startsWith("/")||typeof n.binding.revision!=="string"||!n.binding.revision||n.binding.revision.length>256||!n.binding.context||Object.keys(n.binding.context).length!==7||n.binding.context.version!==1||n.binding.context.accountId!=="local-user"||n.binding.context.storageMode!=="local"||!["none","icloud-drive","google-drive"].includes(n.binding.context.provider)||!["serverBase","sessionId","cacheScope"].every(k=>typeof(n.binding.context as unknown as Record<string,unknown>)[k]==="string"&&String((n.binding.context as unknown as Record<string,unknown>)[k]).length>0)||n.operation!==undefined&&!uuid.test(n.operation))throw new Error("Retained native archive outcomes malformed; retain evidence");
}
function inventory(){
 if(!Number.isSafeInteger(localStorage.length)||localStorage.length>10000)throw new Error("Native archive outcome storage exceeds enumeration bound");
 const old=localStorage.getItem(KEY);let legacy:Note[]=[];let bytes=old?new TextEncoder().encode(old).length:0;
 if(bytes>1024*1024)throw new Error("Retained native archive outcomes exceed bound; retain evidence");
 if(old!==null){try{legacy=JSON.parse(old);}catch{throw new Error("Retained native archive outcomes malformed; retain evidence");}if(!Array.isArray(legacy)||legacy.length>64||new Set(legacy.map(n=>n?.attempt)).size!==legacy.length)throw new Error("Retained native archive outcomes malformed; retain evidence");legacy.forEach(checkedNote);}
 const current:Note[]=[],reviewed=new Map<string,string>();
 for(let i=0;i<localStorage.length;i++){
  const key=localStorage.key(i);if(!key?.startsWith(PREFIX))continue;const raw=localStorage.getItem(key);if(!raw)throw new Error("Original archive outcome missing");const size=new TextEncoder().encode(raw).length;bytes+=size;if(size>65536||bytes>1024*1024)throw new Error("Retained native archive outcomes exceed bound; retain evidence");
  if(key.startsWith(REVIEW)){const id=key.slice(REVIEW.length),original=legacy.find(n=>n.attempt===id&&n.state==="publication_unknown");if(!original||raw!==JSON.stringify(original)||reviewed.has(id))throw new Error("Original legacy archive review evidence changed; retain it");reviewed.set(id,raw);if(reviewed.size>64)throw new Error("Legacy archive reviews exceed bound");continue;}
  let n:Note;try{n=JSON.parse(raw);}catch{throw new Error("Retained native archive outcomes malformed; retain evidence");}checkedNote(n);if(n.state!=="publication_unknown"||key!==PREFIX+n.attempt||legacy.some(old=>old.attempt===n.attempt)||current.some(old=>old.attempt===n.attempt))throw new Error("Conflicting archive outcome records; retain evidence");current.push(n);if(current.length>64)throw new Error("Native archive outcome count exceeds bound");
 }
 if(bytes>1024*1024)throw new Error("Retained native archive outcomes exceed bound; retain evidence");
 const rows=legacy.filter(n=>n.state==="publication_unknown"&&!reviewed.has(n.attempt)).concat(current);if(rows.length>64)throw new Error("Native archive outcome count exceeds bound");return{rows,legacy,bytes};
}
function notes(){return inventory().rows;}
function put(note:Note){
 const data=inventory(),key=PREFIX+note.attempt,raw=JSON.stringify(note);checkedNote(note);
 if(localStorage.getItem(key)!==null||data.legacy.some(n=>n.attempt===note.attempt))throw new Error("Original archive outcome key already exists; retain evidence");
 const size=new TextEncoder().encode(raw).length;if(data.rows.length>=64||size>65536||data.bytes+size>1024*1024)throw new Error("Native archive outcome write exceeds bound; original evidence retained");
 localStorage.setItem(key,raw);if(localStorage.getItem(key)!==raw)throw new Error("Native archive outcome could not be retained");
}
function removeOriginal(note:Note){
 const data=inventory(),key=PREFIX+note.attempt,raw=JSON.stringify(note),legacy=data.legacy.find(n=>n.attempt===note.attempt);
 if(legacy){if(JSON.stringify(legacy)!==raw)throw new Error("Original legacy archive evidence changed; retain it");if(new TextEncoder().encode(raw).length>65536||data.bytes+new TextEncoder().encode(raw).length>1024*1024)throw new Error("Legacy archive review write exceeds bound; original evidence retained");localStorage.setItem(REVIEW+note.attempt,raw);if(localStorage.getItem(REVIEW+note.attempt)!==raw)throw new Error("Original legacy review could not be retained");return;}
 if(localStorage.getItem(key)!==raw)throw new Error("Original archive outcome evidence changed; retain it");localStorage.removeItem(key);if(localStorage.getItem(key)!==null)throw new Error("Original archive review could not be retained");
}
async function invoke(c:string,a:Record<string,unknown>):Promise<unknown>{const n=(window as unknown as{__TAURI_INTERNALS__?:{invoke(c:string,a:Record<string,unknown>):Promise<unknown>}}).__TAURI_INTERNALS__;if(!n?.invoke)throw new Error("Complete native archive owner unavailable");return n.invoke(c,a);}
export function retainedNativeArchiveOutcomes(){return clone(notes().filter(n=>n.state==="publication_unknown"));}
/** One native-owned chooser; no renderer path can grant destination authority. */
export async function exportCurrentCompleteNativeVaultArchive():Promise<{cancelled:boolean;path?:string}>{return exportArchive();}
async function exportArchive(prior?:{note:Note;report:NativeArchiveOutcomeReport}):Promise<{cancelled:boolean;path?:string}>{
 const session=captureNativeArchiveLocalSession();const verifyPrior=()=>{if(prior&&!notes().some(n=>equal(n,prior.note)))throw new Error("Original retained archive observation changed; retain evidence");};verifyPrior();
 if(notes().some(n=>n.state==="publication_unknown"&&n.attempt!==prior?.note.attempt))throw new Error("Review retained named archive destinations before another export");
 const binding=await session.describe();session.verify();verifyPrior();if(prior&&!equal(binding,prior.note.binding))throw new Error("Current canonical revision differs from original sealed archive; retain evidence");
 const response=await invoke("choose_complete_native_vault_archive",{provider:session.context.provider,context:clone(session.context),current:clone(session.context),originalBinding:clone(binding),...(prior?{priorParentPath:prior.note.parent,priorReport:clone(prior.report)}:{})});
 if(response===null){session.verify();verifyPrior();return{cancelled:true};}
 const r=response as{format:number;selection_token:string;destination:{binding:NativeArchiveReviewBinding;parent_path:string;leaf:string;reexport_provenance?:unknown}};
 if(new TextEncoder().encode(JSON.stringify(response)).length>65536||r?.format!==1||!(/^[a-f0-9]{64}$/).test(r.selection_token)||!r.destination||!equal(r.destination.binding,binding)||typeof r.destination.parent_path!=="string"||!r.destination.parent_path.startsWith("/")||typeof r.destination.leaf!=="string"||!r.destination.leaf||r.destination.leaf.includes("/"))throw new Error("Original native archive destination acknowledgment differs");
 const discard=async()=>{await invoke("discard_complete_native_vault_archive",{selectionToken:r.selection_token,current:clone(session.context)});};
 const note:Note={version:1,attempt:crypto.randomUUID(),binding:clone(binding),parent:r.destination.parent_path,leaf:r.destination.leaf,state:"publication_unknown"};
 try{session.verify();verifyPrior();if(prior){if(r.destination.parent_path===prior.note.parent&&r.destination.leaf===prior.note.leaf)throw new Error("Choose a different archive destination; original evidence remains retained");const stage=prior.report.stage as {sha256?:unknown;length?:unknown};const expected={format:1,original_operation:prior.report.operation,original_archive_sha256:stage?.sha256,byte_length:stage?.length,new_operation_required:true,original_creator_authority:false,publication_retry:false};if(!equal(r.destination.reexport_provenance,expected))throw new Error("Updated native sealed-byte re-export capability required; no new publication submitted");}if(notes().some(n=>n.state==="publication_unknown"&&n.attempt!==prior?.note.attempt))throw new Error("Another archive outcome requires named review");put(note);session.verify();verifyPrior();}catch(e){try{await discard();}catch{}throw e;}
 // Durable unknown BEFORE submission; any error/late session change retains it.
 try{
  const outcome=await invoke("commit_complete_native_vault_archive",{selectionToken:r.selection_token,current:clone(session.context)});session.verify();verifyPrior();
  const a=outcome as{format:number;binding:NativeArchiveReviewBinding;publication:{state:string;path:string;operation:string;byte_length:number;sha256:string}};
  if(a?.format!==1||!equal(a.binding,binding)||a.publication?.state!=="published"||a.publication.path!==`${note.parent}/${note.leaf}`||!uuid.test(a.publication.operation)||!Number.isSafeInteger(a.publication.byte_length)||a.publication.byte_length<0||!(/^[a-f0-9]{64}$/).test(a.publication.sha256))throw new Error("Native archive publication acknowledgment differs");
  if(prior){const stage=prior.report.stage as {sha256?:unknown;length?:unknown};if(a.publication.operation===prior.report.operation||a.publication.sha256!==stage.sha256||a.publication.byte_length!==stage.length)throw new Error("New archive provenance/complete-byte acknowledgment differs");}removeOriginal(note);return{cancelled:false,path:a.publication.path};
 }catch{throw new Error(`Archive publication outcome unknown for ${note.parent}/${note.leaf}; inspect destination and review retained evidence before another export`);}
}
/** Explicit inspection acknowledgment closes only this note; never publishes or retries. */
export async function reviewRetainedNativeArchiveDestination(attempt:string,namedDestination:string){
 const note=notes().find(n=>n.attempt===attempt&&n.state==="publication_unknown");if(!note||namedDestination!==`${note.parent}/${note.leaf}`)throw new Error("Original named archive destination review required");
 const review=await openCurrentNativeArchiveOutcomeReview(note.parent,note.binding);const pending=await review.pending();
 for(const row of pending)if(row.destination===namedDestination&&!row.reviewed)await review.acknowledgeNamedDestination(row.operation,namedDestination);
 // A missing journal is no publication/cleanup authority; user explicitly reviewed the name.
 removeOriginal(note);
}

/** Inspect exact native operations; names identify review, never source or publication grants. */
export async function inspectRetainedNativeArchiveDestination(attempt:string,namedDestination:string){
 const note=notes().find(n=>n.attempt===attempt);if(!note||namedDestination!==`${note.parent}/${note.leaf}`)throw new Error("Exact retained named archive required");
 const session=captureNativeArchiveLocalSession(),current=await session.describe();session.verify();
 const review=await openCurrentNativeArchiveOutcomeReview(note.parent,note.binding);const rows=await review.pending();session.verify();
 if(!notes().some(n=>equal(n,note)))throw new Error("Original retained archive observation changed");
 return{currentMatches:equal(current,note.binding),operations:clone(rows.filter(r=>r.destination===namedDestination))};
}
/** NEW chooser/current capture/new operation. Original note and native evidence are retained. */
export async function createCurrentNativeArchiveReexport(attempt:string,operation:string){
 const note=notes().find(n=>n.attempt===attempt);if(!note||!uuid.test(operation))throw new Error("Exact inspected original operation required");
 const inspected=await inspectRetainedNativeArchiveDestination(attempt,`${note.parent}/${note.leaf}`);const report=inspected.operations.find(r=>r.operation===operation);
 if(!inspected.currentMatches||!report||!["sealed_unpublished","unresolved_output_absent"].includes(report.state))throw new Error("Complete original sealed observation and unchanged current source required");
 if(!notes().some(n=>equal(n,note)))throw new Error("Original retained archive observation changed");
 return exportArchive({note:clone(note),report:clone(report)});
}
