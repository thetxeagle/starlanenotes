import {captureNativeAttachmentDelivery,supportsNativeAttachmentDelivery} from './nativeAttachmentCaller';
async function invoke<T>(command:string,args:Record<string,unknown>):Promise<T>{
 const internals=(window as unknown as {__TAURI_INTERNALS__?:{invoke:<T>(name:string,args:Record<string,unknown>)=>Promise<T>}}).__TAURI_INTERNALS__;
 if(!internals?.invoke)throw new Error('Updated native shell required');return internals.invoke<T>(command,args);
}
export async function previewNativeAttachment(operationId:string,offset:number,requested:number):Promise<Uint8Array>{
 if(!Number.isSafeInteger(offset)||offset<0||!Number.isSafeInteger(requested)||requested<0||requested>65536)throw new Error('Preview range must be at most64KiB');
 const binding=captureNativeAttachmentDelivery(operationId);const supported=await supportsNativeAttachmentDelivery();binding.verify();if(!supported)throw new Error('Native attachment delivery unavailable; original request retained');
 const reply=await invoke<{format:number;offset:number;bytes:number[];preview_only:boolean}>('read_native_attachment_preview',{request:binding.request,current:binding.current,offset,requested});binding.verify();
 if(reply.format!==1||reply.offset!==offset||reply.preview_only!==true||!Array.isArray(reply.bytes)||reply.bytes.length>requested||reply.bytes.some(n=>!Number.isInteger(n)||n<0||n>255))throw new Error('Native preview response differs');
 return Uint8Array.from(reply.bytes);
}
export type NativeExportResult={format:1;cancelled:true}|{format:1;cancelled:false;byte_length:number;path:string};
const EXPORT_STATE='starlane_native_attachment_export_unknown_v1:';
/** Unknown ACK never silently starts another export; user must inspect chosen destination. */
export function nativeExportNeedsReview(operationId:string):boolean{return localStorage.getItem(EXPORT_STATE+operationId)!==null;}
export function acknowledgeNativeExportDestinationReview(operationId:string):void{const binding=captureNativeAttachmentDelivery(operationId);binding.verify();localStorage.removeItem(EXPORT_STATE+operationId);binding.verify();}
export async function exportNativeAttachment(operationId:string):Promise<NativeExportResult>{
 if(nativeExportNeedsReview(operationId))throw new Error('Previous export outcome unknown; inspect the chosen destination before retrying');
 const binding=captureNativeAttachmentDelivery(operationId);const supported=await supportsNativeAttachmentDelivery();binding.verify();if(!supported)throw new Error('Native attachment delivery unavailable; original request retained');
 const chosen=await invoke<{format:number;cancelled:boolean;destination_token?:string}>('choose_native_attachment_export_destination',{request:binding.request,current:binding.current});
 const token=chosen.destination_token;
 try {
   binding.verify();
   if(chosen.format!==1||typeof chosen.cancelled!=='boolean')throw new Error('Native destination acknowledgement invalid');
   if(chosen.cancelled)return {format:1,cancelled:true};
   if(typeof token!=='string'||!/^[a-f0-9]{64}$/.test(token))throw new Error('Original native destination token invalid');
   const fresh=captureNativeAttachmentDelivery(operationId);binding.verify();fresh.verify();
   const state=JSON.stringify({format:1,operation_id:operationId,destination_token:token});localStorage.setItem(EXPORT_STATE+operationId,state);if(localStorage.getItem(EXPORT_STATE+operationId)!==state)throw new Error('Export uncertainty record could not be retained');
   const result=await invoke<{format:number;cancelled:boolean;path?:string;byte_length?:number}>('commit_native_attachment_export',{request:fresh.request,current:fresh.current,destinationToken:token});fresh.verify();binding.verify();
   if(result.format!==1||result.cancelled!==false||typeof result.path!=='string'||!Number.isSafeInteger(result.byte_length)||result.byte_length!==JSON.parse(binding.request.server_version_raw).byte_length)throw new Error('Native export acknowledgement invalid');localStorage.removeItem(EXPORT_STATE+operationId);fresh.verify();return {format:1,cancelled:false,path:result.path,byte_length:result.byte_length as number};
 } finally {
   if(typeof token==='string')await invoke<void>('discard_native_attachment_export_selection',{token}).catch(()=>undefined);
 }
}

/** Complete image via public WebKit/GIO held FD; never a whole-file JS buffer. */
export async function imageNativeAttachment(operationId:string):Promise<string>{
 const binding=captureNativeAttachmentDelivery(operationId);
 const capability=await invoke<Record<string,unknown>>('get_native_attachment_capabilities',{});binding.verify();
 if(capability?.format!==1||capability.complete_version_image_stream!==true)throw new Error('Complete native image transport unavailable');
 const response=await invoke<{format:number;url:string;complete_image:boolean}>('open_native_attachment_image',{request:binding.request,current:binding.current});binding.verify();
 if(response.format!==1||response.complete_image!==true||!/^starlane-owned-image:\/\/image\/[a-f0-9]{64}$/.test(response.url))throw new Error('Original native image response differs');return response.url;
}
