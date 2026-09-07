import { invoke, isTauri } from '@tauri-apps/api/core';
import { get, writable } from 'svelte/store';

export interface AllocationChoice { mode:'free'|'shrink'|'delete'; minimumBytes:number; maximumBytes:number; recommendedBytes:number; currentPartitionBytes:number|null; windowsVolume:string|null }
export interface DeletionChoice { identifier:string; partitionLabel:string; fileSystem:string; sizeBytes:number }
export interface BootMenu { defaultOs:'omarchy'|'windows'; timeoutSeconds:number }
export interface UsbInspection { eligible:boolean; reasons:string[]; freeBytes:number|null; requiredBytes:number; replacesBootloader:boolean; restoreAvailable:boolean; needsAdministrator:boolean }
export interface SetupChoice { id: string; label: string; detail: string; sizeBytes: number; eligible: boolean; reasons: string[]; allocation:AllocationChoice|null; deletion:DeletionChoice|null; usb?:UsbInspection|null }
export interface SetupSnapshot {
  kind: 'usb' | 'direct' | null; status: string; stage: string; message: string;
  choices: SetupChoice[]; requirements: string[]; bytes: number; totalBytes: number;
  cancelAvailable: boolean; cancelRequested: boolean; error: string | null;
  preparation?: {secureBoot:string;runtimePackaged:boolean;runtimeReady:boolean}|null;
  recovery?: {filesPath:string;mutationStarted:boolean;message:string;cleanup:{complete:boolean;removedBytes?:number}}|null;
  receipt: { receiptPath?: string; receipt?: { mode?:'preserve'|'restore'; message?:string; backupPath?:string; eject?: { status: string; message: string }; bitLockerRestoration?: { required: boolean; verified: boolean } } } | null;
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
  inspect:(kind:'usb'|'direct', elevated=false) => command('inspect_setup', {kind,elevated}),
  inspectUsb:(choiceId:string,elevated=false) => command('inspect_usb_choice',{choiceId,elevated}),
  prepare:(action:'firmware'|'runtime') => command('prepare_setup', {action}),
  start:(choiceId:string, allocationBytes?:number, deleteConfirmation?:string, bootMenu?:BootMenu, usbMode?:'erase'|'preserve'|'restore') => command('start_setup', {choiceId,allocationBytes:allocationBytes===undefined?null:String(allocationBytes),deleteConfirmation:deleteConfirmation ?? null,bootMenu:bootMenu ?? null,usbMode:usbMode ?? null}),
  cancel:() => command('cancel_setup') };
