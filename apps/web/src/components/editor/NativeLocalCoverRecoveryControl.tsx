import { useEffect, useRef, useState } from "react";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Button } from "@/components/ui/button";
import { DialogBody, DialogDescription, DialogFooter, DialogHeader, DialogShell, DialogTitle } from "@/components/ui/dialog-shell";
import type { RetainedNativeLocalCoverReview } from "@/lib/nativeLocalCover";

export interface NativeLocalCoverRecovery {
 load:()=>Promise<RetainedNativeLocalCoverReview[]>;
 recover:(operationId:string,mode:"check"|"resume")=>Promise<void>;
 discard?:(operationId:string)=>Promise<void>;
 checkDiscard?:(operationId:string)=>Promise<void>;
}
/** Review is explicit and page-owned; journal labels do not grant native custody. */
export function NativeLocalCoverRecoveryControl({ actions }: {actions:NativeLocalCoverRecovery}) {
 const [open,setOpen]=useState(false),[rows,setRows]=useState<RetainedNativeLocalCoverReview[]|null>(null),[error,setError]=useState<string|null>(null),[message,setMessage]=useState<string|null>(null),[busy,setBusy]=useState(false);
 const [confirmation,setConfirmation]=useState<RetainedNativeLocalCoverReview|null>(null);
 const held=useRef(false),generation=useRef(0),mounted=useRef(true),owner=useRef(actions);owner.current=actions;
 useEffect(()=>{mounted.current=true;generation.current++;setRows(null);setError(null);setMessage(null);setConfirmation(null);setOpen(false);setBusy(false);return()=>{mounted.current=false;};},[actions]);
 const verify=()=>{if(!mounted.current||owner.current!==actions)throw new Error("Original cover review changed; reopen it for this draft");};
 const act=async(run:()=>Promise<void>)=>{
  if(held.current)return;held.current=true;setBusy(true);setError(null);setMessage(null);const current=++generation.current;
  try{verify();await run();}catch(error){if(mounted.current&&current===generation.current){setRows(null);setError(error instanceof Error?error.message:"Original cover recovery unavailable; retain its evidence");}}
  finally{held.current=false;if(mounted.current&&current===generation.current)setBusy(false);}
 };
 const load=async()=>{const next=await actions.load();verify();setRows(next);};
 return <div className="mt-2">
  <Button size="sm" variant="ghost" onClick={()=>{setOpen(true);setRows(null);void act(load);}}>Review original cover operations</Button>
  {open&&<DialogShell labelledBy="native-cover-review-title" describedBy="native-cover-review-description" maxWidth="md" onClose={()=>{if(!busy){generation.current++;setOpen(false);}}}>
   <DialogHeader><div><DialogTitle id="native-cover-review-title">Original cover operations</DialogTitle><DialogDescription id="native-cover-review-description">Check the original winner before resuming an interrupted File. Recovery keeps the original operation. Discard is a separate confirmed action for proven idle partial bytes; it never removes a saved cover.</DialogDescription></div></DialogHeader>
   <DialogBody className="space-y-4">
    {busy&&<p role="status" className="text-text-muted text-sm">Checking original cover evidence…</p>}
    {rows?.length===0&&<p className="text-text-muted text-sm">No retained original cover operation is available in this session for this draft.</p>}
    {rows?.map(row=><div key={row.operationId} className="border-border space-y-2 border-b pb-3">
     <p className="text-text break-words text-sm">{row.name}</p><p className="text-text-muted break-all text-xs">Original operation {row.operationId}</p>
     <p className="text-text-muted text-xs">{{prepared:"Original File retained",transfer_unknown:"Original result needs checking",canonical_committed:"Original winner acknowledged",adopted:"Acknowledged cover absent from this draft",discard_unknown:"Original discard result needs review",discarded:"Original staged bytes discarded"}[row.state]} · {row.state==="discarded"?`${(row.discardedBytes??0).toLocaleString()} staged bytes discarded; operation evidence retained`:`${row.received.toLocaleString()} of ${row.byteLength.toLocaleString()} bytes acknowledged`}</p>
     <div className="flex flex-wrap gap-2">
      {row.state!=="discarded"&&<Button size="sm" variant="secondary" disabled={busy} onClick={()=>void act(async()=>{await actions.recover(row.operationId,"check");verify();await load();setMessage("Original acknowledged cover saved to this draft.");})}>Check original winner</Button>}
      {row.originalFileRetained&&["prepared","transfer_unknown"].includes(row.state)&&<Button size="sm" variant="secondary" disabled={busy} onClick={()=>void act(async()=>{await actions.recover(row.operationId,"resume");verify();await load();setMessage("Original File resumed and its acknowledged cover saved to this draft.");})}>Resume original File</Button>}
      {actions.checkDiscard&&["discard_unknown","discarded"].includes(row.state)&&<Button size="sm" variant="secondary" disabled={busy} onClick={()=>void act(async()=>{await actions.checkDiscard!(row.operationId);verify();await load();setMessage("Original native discard result verified. Operation evidence remains retained.");})}>Check original discard</Button>}
      {actions.discard&&["prepared","transfer_unknown","discard_unknown"].includes(row.state)&&<Button size="sm" variant="danger" disabled={busy} onClick={()=>setConfirmation(row)}>{row.state==="discard_unknown"?"Retry original discard":"Discard original partial upload"}</Button>}
     </div>
    </div>)}
    {message&&<p role="status" className="text-text text-sm">{message}</p>}{error&&<p role="alert" className="text-danger text-sm">{error}</p>}
   </DialogBody>
   <DialogFooter><Button variant="secondary" disabled={busy} onClick={()=>void act(load)}>Refresh original operations</Button><Button disabled={busy} onClick={()=>{generation.current++;setOpen(false);}}>Close</Button></DialogFooter>
  </DialogShell>}
  {confirmation&&<ConfirmDialog labelledBy="native-cover-discard-title" title="Discard original partial upload?" destructive confirmLabel="Discard original staged bytes" message={`Discard only the proven idle staged bytes for “${confirmation.name}”? Saved covers and immutable intent/chunk evidence remain. The cached File is released only after a verified native result. Active writers, current references and unproven states refuse. This cannot be undone.`} onCancel={()=>setConfirmation(null)} onConfirm={()=>{const operation=confirmation.operationId;setConfirmation(null);void act(async()=>{await actions.discard!(operation);verify();await load();setMessage("Original staged bytes discarded. Intent and chunk evidence remain retained.");});}}/>}
 </div>;
}
