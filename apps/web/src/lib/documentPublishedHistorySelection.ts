import { isApiError } from "./api";
import type { PublicationSnapshotSummary, PublicationSnapshotDetail, PublicationSnapshotRaw, createPublicationHistoryReader } from "./documentPublishedHistory";
export type PublicationHistoryStatus = "idle" | "loading" | "ready" | "denied" | "offline" | "unavailable";
export interface PublicationHistorySelectionState {
  records: PublicationSnapshotSummary[];
  listStatus: PublicationHistoryStatus;
  previewOpen: boolean;
  previewStatus: PublicationHistoryStatus;
  selected: PublicationSnapshotSummary | null;
  detail: PublicationSnapshotDetail | null;
  raw: PublicationSnapshotRaw | null;
  rawLoading: boolean;
}
const empty = ():PublicationHistorySelectionState => ({records:[],listStatus:"idle",previewOpen:false,previewStatus:"idle",selected:null,detail:null,raw:null,rawLoading:false});
const errorStatus = (error:unknown):PublicationHistoryStatus => isApiError(error) && [401,403].includes(error.status) ? "denied" : error instanceof Error && /^Reconnect/.test(error.message) ? "offline" : "unavailable";
/** Generation fences supplement server/account admission; route changes never reuse old detail. */
export function createPublicationHistorySelection(reader:ReturnType<typeof createPublicationHistoryReader>, changed:(state:PublicationHistorySelectionState)=>void) {
  let scope:{space:string;document:string}|null=null;
  let listEpoch=0,previewEpoch=0,rawEpoch=0;
  let state=empty();
  const emit=()=>changed(structuredClone(state));
  return {
    setScope(space:string,document:string){scope={space,document};listEpoch++;previewEpoch++;rawEpoch++;state=empty();emit();},
    reset(){scope=null;listEpoch++;previewEpoch++;rawEpoch++;state=empty();emit();},
    snapshot(){return structuredClone(state);},
    async refresh(){
      if(!scope)return;
      const selectedScope={...scope},ticket=++listEpoch;
      previewEpoch++;rawEpoch++;state=empty();state.listStatus="loading";emit();
      try{const records=await reader.list(selectedScope.space,selectedScope.document);if(ticket!==listEpoch)return;state.records=records;state.listStatus="ready";emit();}
      catch(error){if(ticket!==listEpoch)return;state.records=[];state.listStatus=errorStatus(error);emit();}
    },
    async preview(summary:PublicationSnapshotSummary){
      if(!scope)return;
      const selectedScope={...scope},selected=structuredClone(summary),ticket=++previewEpoch;
      rawEpoch++;state={...state,previewOpen:true,previewStatus:"loading",selected,detail:null,raw:null,rawLoading:false};emit();
      try{const detail=await reader.get(selectedScope.space,selectedScope.document,selected.id,selected);if(ticket!==previewEpoch)return;state.detail=detail;state.previewStatus="ready";emit();}
      catch(error){if(ticket!==previewEpoch)return;const status=errorStatus(error);state={...state,selected:null,detail:null,raw:null,previewStatus:status};if(status==="denied"){state.records=[];state.listStatus="denied";}emit();}
    },
    async readRaw():Promise<PublicationSnapshotRaw|null>{
      if(!scope||!state.selected||state.previewStatus!=="ready"||state.rawLoading)return null;
      const selectedScope={...scope},selected=structuredClone(state.selected),previewTicket=previewEpoch,ticket=++rawEpoch;
      state.rawLoading=true;emit();
      try{const raw=await reader.raw(selectedScope.space,selectedScope.document,selected);if(ticket!==rawEpoch||previewTicket!==previewEpoch)return null;state.raw=raw;state.rawLoading=false;emit();return structuredClone(raw);}
      catch(error){if(ticket!==rawEpoch||previewTicket!==previewEpoch)return null;const status=errorStatus(error);state={...state,selected:null,detail:null,raw:null,rawLoading:false,previewStatus:status};if(status==="denied"){state.records=[];state.listStatus="denied";}emit();return null;}
    },
    close(){previewEpoch++;rawEpoch++;state={...state,previewOpen:false,previewStatus:"idle",selected:null,detail:null,raw:null,rawLoading:false};emit();},
  };
}
