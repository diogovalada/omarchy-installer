import type { StorageChoice } from './storageChoices';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { get, writable } from 'svelte/store';

export interface ApplePlan { deletion?:{identifier:string;sourceIdentifier:string;sizeBytes:string}; headline: string; subheadline: string; omarchyBytes: string; macOSBytes: string; bindingDigest: string; facts: {label:string;value:string}[]; artifacts: {role:string;fileName:string;expectedBytes:string}[] }
interface AppleNative {
  storageChoices?:StorageChoice[];
  phase:string; busy:boolean; canExecute:boolean; canRetryRecovery:boolean; cancelAvailable:boolean; hasExecutionStarted:boolean;
  helper:{enabled:boolean;summary:string}; hostSummary?:string; host?:{model:string;chip:string};
  plan?:ApplePlan; blockingReason?:string; admissionBlock?:{code:string;message:string};
  failure?:{headline:string;detail:string;remedy?:string;technicalDetail?:string};
  progress?:{stage?:string;phaseTitle?:string;rows?:{fileName:string;bytesCompleted:string;totalBytes:string;phase:string}[];feed?:{id:string;kind:string;text:string}[]};
  handoff?:{headline:string;subheadline:string;explainer:string;hint:string;steps:{number:number;title:string;detail:string}[]};
  credentials?:{rejected:boolean}; completion?:{headline:string;subheadline:string;verified:{label:string;value:string}[]};
}
export interface AppleSnapshot { revision:number; status:string; available:boolean; host_os:string; host_architecture:string; busy:boolean; pending_intent:string|null; probe:{packagingReady:boolean;blockers:{code:string;message:string}[]}|null; native:AppleNative|null; plan:ApplePlan|null; error:{code:string;message:string}|null; outcome_unknown:boolean }
type Intent = {kind:'connect'|'inspect'|'prepare_plan'|'choose_storage'|'review_plan'|'approve_plan'|'execute'|'retry_recovery'|'cancel'|'refresh';allocation_bytes?:string;choice_id?:string;confirmation?:string};
const state = writable<{snapshot:AppleSnapshot|null;pending:boolean;error:string|null}>({snapshot:null,pending:false,error:null});
let epoch=0; let reading=false;
async function refresh() {
  if (!isTauri() || reading || get(state).pending) return;
  reading=true; const generation=epoch;
  try { const snapshot=await invoke<AppleSnapshot>('apple_setup_status'); if (generation===epoch) state.update(s=>({...s,snapshot,error:null})); }
  catch(error) { if(generation===epoch) state.update(s=>({...s,error:String(error)})); }
  finally {reading=false;}
}
async function action(intent:Intent) {
  if (!isTauri() || get(state).pending) return;
  if (!get(state).snapshot) await refresh();
  const current=get(state).snapshot; if(!current) return;
  epoch+=1; state.update(s=>({...s,pending:true,error:null}));
  try {const snapshot=await invoke<AppleSnapshot>('apple_setup_action',{action:{expected_revision:current.revision,intent}}); state.update(s=>({...s,snapshot}));}
  catch(error) {state.update(s=>({...s,error:String(error)}));}
  finally {state.update(s=>({...s,pending:false}));}
}
export const appleSetup={subscribe:state.subscribe,refresh,action};
