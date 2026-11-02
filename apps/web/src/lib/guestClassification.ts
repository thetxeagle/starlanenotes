import { api, ApiError, getApiBase } from "./api";
import { capturePageMetadataLoadGuard } from "./pageMetadataLoad";
import { getCurrentLaunchMode } from "./launchMode";
export interface GuestAccountRow { id:string;email:string;display_name:string;role:string;is_active:boolean;created_at:string;account_kind?:"member"|"guest"; }
export interface GuestClassificationReview { schema_version:1;target:{id:string;display_name:string;account_kind:"member"};source_digest:string;expires_at:string; }
export interface GuestClassificationConfirmed { confirmed:true;user_id:string;account_kind:"guest"; }
const invalid=():never=>{throw new Error("Account access could not be verified. Keep the original request and refresh users.");};
const row=(v:unknown):Record<string,unknown>=>v&&typeof v==="object"&&!Array.isArray(v)?v as Record<string,unknown>:invalid();
const exact=(v:Record<string,unknown>,keys:string[])=>Object.keys(v).length===keys.length&&Object.keys(v).every(k=>keys.includes(k));
const string=(v:unknown):v is string=>typeof v==="string"&&v.trim().length>0;
export function canReviewGuestAccount(target:Pick<GuestAccountRow,"id"|"role"|"is_active"|"account_kind">,actorId:string|undefined) {return !!actorId&&target.id!==actorId&&target.is_active&&target.account_kind==="member"&&(target.role==="member"||target.role==="viewer");}
/** Kind is independent of role. Omitted old-server kind alone retains compatibility. */
export function canUseHumanAdministration(role:string|undefined,kind:unknown) {return role==="admin"&&(kind===undefined||kind==="member");}
export function accountKindLabel(kind:unknown) {return kind==="guest"?"Guest":kind===undefined||kind==="member"?"Member":"Unavailable";}
export function parseGuestAccountRows(value:unknown):GuestAccountRow[] {
  if(!Array.isArray(value))return invalid();const ids=new Set<string>();
  for(const valueRow of value) {const r=row(valueRow);if(!string(r.id)||ids.has(r.id)||typeof r.email!=="string"||typeof r.display_name!=="string"||!string(r.role)||typeof r.is_active!=="boolean"||typeof r.created_at!=="string"||(Object.prototype.hasOwnProperty.call(r,"account_kind")&&r.account_kind!=="member"&&r.account_kind!=="guest"))return invalid();ids.add(r.id);}
  return value as GuestAccountRow[];
}
function expiry(value:unknown):value is string {
  if(typeof value!=="string")return false;const m=/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(?:Z|([+-])(\d{2}):(\d{2}))$/.exec(value);if(!m)return false;
  const [year,month,day,hour,minute,second]=m.slice(1,7).map(Number);
  return month>=1&&month<=12&&day>=1&&day<=new Date(Date.UTC(year,month,0)).getUTCDate()&&hour<=23&&minute<=59&&second<=59&&(!m[7]||Number(m[8])<=23&&Number(m[9])<=59)&&Number.isFinite(Date.parse(value));
}
export function parseGuestClassificationReview(value:unknown,id:string):GuestClassificationReview {
  const r=row(value),t=row(r.target);if(!exact(r,["schema_version","target","source_digest","expires_at"])||r.schema_version!==1||!exact(t,["id","display_name","account_kind"])||t.id!==id||typeof t.display_name!=="string"||t.account_kind!=="member"||typeof r.source_digest!=="string"||!/^[0-9a-f]{64}$/.test(r.source_digest)||!expiry(r.expires_at))return invalid();return r as unknown as GuestClassificationReview;
}
export function parseGuestClassificationConfirmed(value:unknown,id:string):GuestClassificationConfirmed {const r=row(value);if(!exact(r,["confirmed","user_id","account_kind"])||r.confirmed!==true||r.user_id!==id||r.account_kind!=="guest")return invalid();return r as unknown as GuestClassificationConfirmed;}
/** Direct fresh transport: no cache, refresh, queue, mirror or automatic resend. */
export function captureGuestClassificationTransport(current:()=>void) {
  const context=capturePageMetadataLoadGuard(),token=api.getToken(),base=getApiBase();
  const verify=()=>{context();current();if(getCurrentLaunchMode()!=="server"||api.getToken()!==token||getApiBase()!==base)throw new Error("Administrator or server changed. Reopen users.");};verify();if(!token)throw new ApiError("Sign in as an administrator.",401);
  async function request<T>(path:string,method:"GET"|"POST",body:unknown,parse:(v:unknown)=>T):Promise<T> {
    verify();const copied=body===undefined?undefined:JSON.stringify(body);
    try {const response=await fetch(`${base}${path}`,{method,cache:"no-store",redirect:"error",headers:{Authorization:`Bearer ${token}`,...(copied===undefined?{}:{"Content-Type":"application/json"})},...(copied===undefined?{}:{body:copied})});verify();const value:unknown=await response.json();verify();if(response.status!==200)throw new ApiError("Account change was not confirmed. Keep the request and refresh users.",response.status);const result=parse(value);verify();return result;}
    catch(error){verify();throw error;}
  }
  return {verify,users:()=>request("/admin/users","GET",undefined,parseGuestAccountRows),review:(id:string)=>request(`/admin/users/${encodeURIComponent(id)}/guest/review`,"POST",{},v=>parseGuestClassificationReview(v,id)),confirm:(review:GuestClassificationReview)=>request(`/admin/users/${encodeURIComponent(review.target.id)}/guest/confirm`,"POST",{source_digest:review.source_digest,confirm:true},v=>parseGuestClassificationConfirmed(v,review.target.id))};
}
/** Retained in-memory evidence. A fresh review is never acknowledgement of old dispatch. */
export function createGuestClassificationUncertainty() {
  const retained:GuestClassificationReview[]=[];let blocked=false;
  return {blocked:()=>blocked,retained:()=>structuredClone(retained),dispatched(review:GuestClassificationReview){if(blocked)throw new Error("Review the current account and explicitly start a separate review.");retained.push(structuredClone(review));blocked=true;},confirmed(review:GuestClassificationReview){const last=retained[retained.length-1];if(!last||last.source_digest!==review.source_digest||last.target.id!==review.target.id)invalid();retained.pop();blocked=false;},separateReview(){blocked=false;}};
}
/** Modal controller: at most one confirmation for the exact reviewed source. */
export function createGuestClassificationSession(id:string,current:()=>void,uncertainty=createGuestClassificationUncertainty()) {
  const transport=captureGuestClassificationTransport(current);let review:GuestClassificationReview|null=null,dispatching=false;
  return {verify:transport.verify,uncertainty,async review(){if(dispatching)throw new Error("Account change is pending.");review=null;const value=await transport.review(id);review=structuredClone(value);return structuredClone(value);},separateReview(){transport.verify();if(!review)throw new Error("Review the current account first.");uncertainty.separateReview();return structuredClone(review);},async confirm(now=Date.now()) {
    transport.verify();if(!review||dispatching||uncertainty.blocked())throw new Error("Start an explicit account review before confirming.");if(Date.parse(review.expires_at)<=now)throw new Error("Review expired. Review the current account again.");
    const original=structuredClone(review);dispatching=true;uncertainty.dispatched(original);review=null;
    try {const result=await transport.confirm(original);transport.verify();uncertainty.confirmed(original);return result;}finally{dispatching=false;}
  }};
}
