import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { DialogShell } from "@/components/ui/dialog-shell";
import { canReviewGuestAccount, createGuestClassificationSession, createGuestClassificationUncertainty, type GuestAccountRow, type GuestClassificationReview } from "@/lib/guestClassification";

const retainedRequests = new Map<string, ReturnType<typeof createGuestClassificationUncertainty>>();
/** Existing-account classification only. No invitation, role promotion or account creation. */
export function GuestAccountControls({target,actorId,contextKey,assertCurrent,onConfirmed}: {
  target:GuestAccountRow;actorId:string|undefined;contextKey:string;assertCurrent:()=>void;onConfirmed:(id:string)=>void;
}) {
  const key=`${contextKey}/${actorId??""}/${target.id}`;
  const uncertainty=retainedRequests.get(key)??createGuestClassificationUncertainty();retainedRequests.set(key,uncertainty);
  const live=useRef({key,assertCurrent,onConfirmed});live.current={key,assertCurrent,onConfirmed};
  const [session,setSession]=useState<ReturnType<typeof createGuestClassificationSession>|null>(null);
  const [review,setReview]=useState<GuestClassificationReview|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState("");
  const [clock,setClock]=useState(Date.now()),[unknown,setUnknown]=useState(uncertainty.blocked());
  const mounted=useRef(false),generation=useRef(0),sessionKey=useRef<string|null>(null);
  useEffect(()=>{generation.current++;setSession(null);setReview(null);setBusy(false);setError("");setUnknown(uncertainty.blocked());},[key,uncertainty]);
  useEffect(()=>{const lifecycleGeneration=generation;mounted.current=true;return()=>{mounted.current=false;lifecycleGeneration.current++;};},[]);
  useEffect(()=>{if(!session)return;const timer=window.setInterval(()=>setClock(Date.now()),1000);return()=>window.clearInterval(timer);},[session]);
  const close=()=>{generation.current++;setSession(null);setReview(null);setBusy(false);setError("");};
  async function loadReview(active:ReturnType<typeof createGuestClassificationSession>,separate=false) {
    const request=++generation.current;setBusy(true);setError("");setReview(null);
    try {
      const value=await active.review();active.verify();
      if(!mounted.current||request!==generation.current||live.current.key!==key)return;
      if(separate) {active.separateReview();setUnknown(false);}
      setReview(value);
    }catch(reason){try{active.verify();if(mounted.current&&request===generation.current)setError(reason instanceof Error?reason.message:"Account review unavailable.");}catch{/* Never paint a foreign completion. */}}
    finally{if(mounted.current&&request===generation.current)setBusy(false);}
  }
  function open() {
    setError("");
    try {
      assertCurrent();if(!canReviewGuestAccount(target,actorId))throw new Error("Refresh users before reviewing this account.");
      const origin=assertCurrent,originalKey=key;
      const active=createGuestClassificationSession(target.id,()=>{origin();if(live.current.key!==originalKey)throw new Error("Selected account changed. Refresh users.");},uncertainty);
      sessionKey.current=key;setSession(active);setUnknown(uncertainty.blocked());
      if(!uncertainty.blocked())void loadReview(active);
    }catch(reason){setError(reason instanceof Error?reason.message:"Account review unavailable.");}
  }
  async function confirm() {
    if(!session||!review||busy||unknown)return;
    const active=session,request=++generation.current;setBusy(true);setError("");
    try {
      const result=await active.confirm();active.verify();
      if(!mounted.current||request!==generation.current||live.current.key!==key)return;
      setReview(null);setSession(null);setUnknown(false);live.current.onConfirmed(result.user_id);
    }catch(reason){
      if(mounted.current&&request===generation.current&&live.current.key===key) {
        setUnknown(uncertainty.blocked());setReview(null);
        try{active.verify();setError(uncertainty.blocked()?"The account change is unconfirmed. The original request is retained. Refresh users before a separate review; no automatic retry.":reason instanceof Error?reason.message:"Account change unavailable.");}catch{setError("Administrator or server changed. The original request remains unconfirmed; return to its account to review.");}
      }
    }finally{if(mounted.current&&request===generation.current)setBusy(false);}
  }
  let selectionCurrent=true;try{assertCurrent();}catch{selectionCurrent=false;}
  return <>
    {canReviewGuestAccount(target,actorId)&&<Button size="sm" variant="secondary" disabled={busy||!selectionCurrent} onClick={open}>Review guest access</Button>}
    {uncertainty.blocked()&&!session&&<p role="status" className="text-text-muted text-xs">Earlier account change unconfirmed; original request retained for this session.</p>}
    {error&&!session&&<p role="alert" className="text-danger text-xs">{error}</p>}
    {session&&selectionCurrent&&sessionKey.current===key&&<DialogShell labelledBy="guest-account-review-title" maxWidth="md" onClose={close}>
      <div className="border-border border-b px-4 py-3 sm:px-6"><h2 id="guest-account-review-title" className="text-text text-heading-lg font-semibold">Review guest access</h2></div>
      <div className="space-y-3 p-4 sm:p-6">
        <p className="text-text text-sm">{review?.target.display_name??target.display_name} will receive access only to pages explicitly shared with this account.</p>
        {target.email && <p className="text-text-muted break-words text-sm">{target.email}</p>}
        <p className="text-text-muted text-sm">Existing memberships and owned content are retained, but they no longer supply space, group, site-wide or administrator access. This does not share any page, change draft sharing, or enable a public link.</p>
        <p className="text-text-muted text-sm">There is no change-back action in this workflow. You can later deactivate the account or revoke its explicit page access.</p>
        {busy&&<p role="status" className="text-text-muted text-sm">Checking account access…</p>}
        {unknown&&<p role="status" className="text-text-muted text-sm">The earlier request is retained and unconfirmed. A new review is a separate action, not an acknowledgement or retry.</p>}
        {error&&<p role="alert" className="text-danger text-sm">{error}</p>}
        {review&&Date.parse(review.expires_at)<=clock&&<p role="status" className="text-text-muted text-sm">Review expired. Review the current account again.</p>}
      </div>
      <div className="border-border flex flex-wrap justify-end gap-2 border-t px-4 py-3 sm:px-6">
        <Button variant="secondary" onClick={close}>Close</Button>
        <Button variant="secondary" disabled={busy} onClick={()=>void loadReview(session,unknown)}>{unknown?"Start a separate current review":"Review current account"}</Button>
        <Button variant="danger" disabled={busy||unknown||!review||Date.parse(review.expires_at)<=clock} onClick={()=>void confirm()}>Confirm guest access</Button>
      </div>
    </DialogShell>}
  </>;
}
