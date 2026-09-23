import { invoke, isTauri } from '@tauri-apps/api/core';
import { get, writable } from 'svelte/store';

export interface AllocationChoice { mode:'free'|'shrink'|'delete'; minimumBytes:number; maximumBytes:number; recommendedBytes:number; currentPartitionBytes:number|null; windowsVolume:string|null }
export interface DeletionChoice { identifier:string; partitionLabel:string; fileSystem:string; sizeBytes:number }
export interface BootMenu { defaultOs:'omarchy'|'windows'; timeoutSeconds:number }
export interface UsbInspection { eligible:boolean; reasons:string[]; freeBytes:number|null; requiredBytes:number; replacesBootloader:boolean; needsAdministrator:boolean }
export interface SetupChoice { id: string; label: string; detail: string; sizeBytes: number; eligible: boolean; reasons: string[]; allocation:AllocationChoice|null; deletion:DeletionChoice|null; usb?:UsbInspection|null }
export interface UsbReview { token:string; choiceId:string; mode:'erase'|'preserve'; label:string; detail:string; sizeBytes:number; fileName:string; replacesBootloader:boolean }
export interface SetupSnapshot {
  stagedTesting?:boolean;
  stagedRecovery?:{operations:StagedOperation[];recordErrors:{operationId:string;message:string}[]}|null;
  stagedIso?:{operations?:StagedOperation[];recordErrors?:{operationId:string;message:string}[];choices?:StagedChoice[];blocked?:StagedBlockedDisk[];disks?:StagedDisk[];minimumLinuxBytes?:number;temporaryBytes?:number;secureBoot?:boolean|null;operationId?:string;status?:string;message?:string}|null;
  stagedReview?:{summary:string}|null;
  usbReview?:UsbReview|null;
  bitLocker?:{message:string;reminderRegistered:boolean}|null;
  kind: 'usb' | 'direct' | null; status: string; stage: string; message: string;
  choices: SetupChoice[]; requirements: string[]; bytes: number; totalBytes: number;
  cancelAvailable: boolean; cancelRequested: boolean; error: string | null;
  preparation?: {secureBoot:string;runtimePackaged:boolean;runtimeReady:boolean}|null;
  recovery?: {filesPath:string;mutationStarted:boolean;message:string;cleanup:{complete:boolean;removedBytes?:number}}|null;
  receipt: { receiptPath?: string; recordWarning?: string; receipt?: { mode?:'preserve'; message?:string; backupPath?:string; eject?: { status: string; message: string }; bitLockerRestoration?: { required: boolean; verified: boolean } } } | null;
}
export interface StagedOperation { operationId:string;status:string;diskNumber:number;linuxBytes:number;temporaryBytes:number;message:string }
export interface StagedChoice { diskNumber:number;diskUniqueId:string;diskSizeBytes?:number;label:string;largestFreeAfterStagingBytes:number;target:Record<string,unknown>;encryption?:StagedEncryption[] }
export interface StagedEncryption { driveLetter:string;isOsVolume:boolean;conversionStatus:number;protectionStatus:number;lockStatus:number }
export interface StagedBlockedDisk { diskNumber:number;diskUniqueId:string;diskSizeBytes?:number;reason:string;encryption?:StagedEncryption[] }
export interface StagedResizeQuery { diskNumber:number;diskUniqueId:string;partitionNumber:number;partitionGuid:string }
export interface StagedRegion { kind:'free'|'partition';offsetBytes:number;sizeBytes:number;label:string;partitionNumber?:number;partitionGuid?:string;fileSystem?:string;freeBytes?:number|null;resizeState?:'unchecked'|'checked'|'failed'|'insufficient'|'unavailable';resizeReason?:string;maximumReleaseBytes?:number|null;reserveBytes?:number|null }
export interface StagedDisk { diskNumber:number;diskUniqueId:string;diskSizeBytes:number;unallocatedBytes:number;largestFreeBytes:number;regions:StagedRegion[];encryption?:StagedEncryption[] }
export const setupActive = (status?: string) => status === 'inspecting' || status === 'running';
const state = writable<{ snapshot: SetupSnapshot | null; pending: boolean; error: string | null }>({ snapshot:null, pending:false, error:null });
let epoch = 0; let reading = false;
let commandError: string | null = null;
async function refresh() {
  if (!isTauri() || reading || get(state).pending) return;
  reading = true; const generation = epoch;
  try { const snapshot = await invoke<SetupSnapshot>('setup_status'); if (generation === epoch) state.update(s => ({ ...s, snapshot, error:commandError })); }
  catch (error) { if (generation === epoch) state.update(s => ({ ...s, error:String(error) })); }
  finally { reading=false; }
}
async function command(name: string, args: Record<string, unknown> = {}) {
  if (!isTauri() || get(state).pending) return;
  epoch += 1; commandError=null; state.update(s => ({ ...s, pending:true, error:null }));
  try { const snapshot = await invoke<SetupSnapshot>(name, args); state.update(s => ({ ...s, snapshot })); }
  catch (error) { commandError=String(error); state.update(s => ({ ...s, error:commandError })); }
  finally { state.update(s => ({ ...s, pending:false })); }
}
export const setup = { subscribe:state.subscribe, refresh,
  inspect:(kind:'usb'|'direct') => command('inspect_setup', {kind}),
  inspectUsb:(choiceId:string,elevated=false) => command('inspect_usb_choice',{choiceId,elevated}),
  prepare:(action:'firmware'|'runtime') => command('prepare_setup', {action}),
  prepareBitLocker:(reminderOnly:boolean) => command('prepare_bitlocker', {reminderOnly}),
  stagedIso:(action:'inspect'|'stage'|'status'|'arm'|'firmware'|'cleanup',selection:Record<string,unknown>|null=null,operationId:string|null=null,resize:StagedResizeQuery|null=null) => command('staged_iso',{action,selection,operationId,resize}),
  reviewUsb:(choiceId:string,mode:'erase'|'preserve') => command('review_usb_setup',{choiceId,mode}),
  dismissUsbReview:() => command('dismiss_usb_review'),
  start:(choiceId:string, allocationBytes?:number, deleteConfirmation?:string, bootMenu?:BootMenu, usbMode?:'erase'|'preserve',usbReviewToken?:string) => command('start_setup', {choiceId,allocationBytes:allocationBytes===undefined?null:String(allocationBytes),deleteConfirmation:deleteConfirmation ?? null,bootMenu:bootMenu ?? null,usbMode:usbMode ?? null,usbReviewToken:usbReviewToken ?? null}),
  respondStagedReview:(approved:boolean) => command('respond_staged_review',{approved}),
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
