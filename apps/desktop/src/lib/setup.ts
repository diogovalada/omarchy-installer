import { invoke, isTauri } from '@tauri-apps/api/core';
import { get, writable } from 'svelte/store';

export interface AllocationChoice { mode:'free'|'shrink'|'delete'; minimumBytes:number; maximumBytes:number; recommendedBytes:number; currentPartitionBytes:number|null; windowsVolume:string|null }
export interface DeletionChoice { identifier:string; partitionLabel:string; fileSystem:string; sizeBytes:number }
export interface BootMenu { defaultOs:'omarchy'|'windows'; timeoutSeconds:number }
export interface UsbInspection { eligible:boolean; reasons:string[]; freeBytes:number|null; requiredBytes:number; replacesBootloader:boolean; needsAdministrator:boolean }
export interface SetupChoice { id: string; label: string; detail: string; sizeBytes: number; eligible: boolean; reasons: string[]; allocation:AllocationChoice|null; deletion:DeletionChoice|null; usb?:UsbInspection|null }
export interface UsbReview { token:string; choiceId:string; mode:'erase'|'preserve'; label:string; detail:string; sizeBytes:number; fileName:string; replacesBootloader:boolean }
export interface SetupSnapshot {
  usbReview?:UsbReview|null;
  kind: 'usb' | 'direct' | null; status: string; stage: string; message: string;
  choices: SetupChoice[]; requirements: string[]; bytes: number; totalBytes: number;
  cancelAvailable: boolean; cancelRequested: boolean; error: string | null;
  preparation?: {secureBoot:string;runtimePackaged:boolean;runtimeReady:boolean}|null;
  recovery?: {filesPath:string;mutationStarted:boolean;message:string;cleanup:{complete:boolean;removedBytes?:number}}|null;
  receipt: { receiptPath?: string; recordWarning?: string; receipt?: { mode?:'preserve'; message?:string; backupPath?:string; eject?: { status: string; message: string }; bitLockerRestoration?: { required: boolean; verified: boolean } } } | null;
}
export const setupActive = (status?: string) => status === 'inspecting' || status === 'running';
const state = writable<{ snapshot: SetupSnapshot | null; pending: boolean; error: string | null }>({ snapshot:null, pending:false, error:null });
let epoch = 0; let reading = false;
async function refresh() {
  if (!isTauri() || reading || get(state).pending) return;
  reading = true; const generation = epoch;
  try { const snapshot = await invoke<SetupSnapshot>('setup_status'); if (generation === epoch) state.update(s => ({ ...s, snapshot, error:null })); }
  catch (error) { if (generation === epoch) state.update(s => ({ ...s, error:String(error) })); }
  finally { reading=false; }
}
async function command(name: string, args: Record<string, unknown> = {}) {
  if (!isTauri() || get(state).pending) return;
  epoch += 1; state.update(s => ({ ...s, pending:true, error:null }));
  try { const snapshot = await invoke<SetupSnapshot>(name, args); state.update(s => ({ ...s, snapshot })); }
  catch (error) { state.update(s => ({ ...s, error:String(error) })); }
  finally { state.update(s => ({ ...s, pending:false })); }
}
export const setup = { subscribe:state.subscribe, refresh,
  inspect:(kind:'usb'|'direct') => command('inspect_setup', {kind}),
  inspectUsb:(choiceId:string,elevated=false) => command('inspect_usb_choice',{choiceId,elevated}),
  prepare:(action:'firmware'|'runtime') => command('prepare_setup', {action}),
  reviewUsb:(choiceId:string,mode:'erase'|'preserve') => command('review_usb_setup',{choiceId,mode}),
  dismissUsbReview:() => command('dismiss_usb_review'),
  start:(choiceId:string, allocationBytes?:number, deleteConfirmation?:string, bootMenu?:BootMenu, usbMode?:'erase'|'preserve',usbReviewToken?:string) => command('start_setup', {choiceId,allocationBytes:allocationBytes===undefined?null:String(allocationBytes),deleteConfirmation:deleteConfirmation ?? null,bootMenu:bootMenu ?? null,usbMode:usbMode ?? null,usbReviewToken:usbReviewToken ?? null}),
  cancel:() => command('cancel_setup') };

export function usbIssueSummary(inspection:UsbInspection):string {
  const layout=inspection.reasons.some(reason=>/partition|layout|FAT32|NTFS/i.test(reason) && !/free space|MiB free|access|denied|administrator/i.test(reason));
  const space=inspection.reasons.some(reason=>/free space|MiB free/i.test(reason));
  if(layout && space) return 'Unsupported drive layout and not enough free space.';
  if(layout) return 'Unsupported drive layout.';
  if(space) return 'Not enough free space.';
  if(inspection.needsAdministrator) return 'Administrator access is needed to check compatibility.';
  return 'USB compatibility could not be confirmed. See details.';
}
