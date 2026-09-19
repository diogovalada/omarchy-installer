import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, expect, it, vi } from 'vitest';
import StagedInstaller from './StagedInstaller.svelte';
import { setup } from './setup';
const calls=vi.hoisted(()=>({stagedIso:vi.fn(),prepareBitLocker:vi.fn()}));
vi.mock('./setup',async importOriginal=>{
  const actual=await importOriginal<typeof import('./setup')>();
  const {writable}=await import('svelte/store');
  return {...actual,setup:{...writable({pending:false,snapshot:null}),...calls}};
});
const state=setup as typeof setup & {set:(value:unknown)=>void};
beforeEach(()=>{vi.clearAllMocks();state.set({pending:false,snapshot:null});});
it('shows nothing and does not elevate when no temporary installer exists',()=>{
  render(StagedInstaller,{recoveryOnly:true});
  expect(calls.stagedIso).not.toHaveBeenCalled();
  expect(screen.queryByRole('region',{name:'Temporary installer'})).not.toBeInTheDocument();
  expect(screen.queryByRole('button')).not.toBeInTheDocument();
});
it('binds boot and cleanup actions to the recorded operation',async()=>{
  const op={operationId:'11111111-1111-4111-8111-111111111111',status:'staged',diskNumber:0,temporaryBytes:8*1024**3,message:'Installer ready'};
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{operations:[op]}}});
  render(StagedInstaller,{recoveryOnly:true});
  await fireEvent.click(screen.getByRole('button',{name:'Start installer on next restart'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('arm',null,op.operationId);
  await fireEvent.click(screen.getByRole('button',{name:'Remove temporary installer'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('cleanup',null,op.operationId);
});
it('offers only reviewed cleanup for an incomplete copy',()=>{
  state.set({pending:false,snapshot:{status:'complete',stagedIso:{operations:[{operationId:'id',status:'copying',diskNumber:0,temporaryBytes:8*1024**3,message:'Interrupted'}]}}});
  render(StagedInstaller,{recoveryOnly:true});
  expect(screen.queryByRole('button',{name:'Start installer on next restart'})).not.toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Remove temporary installer'})).toBeEnabled();
});
it('blocks overlapping operations',()=>{
  state.set({pending:false,snapshot:{status:'running'}});
  render(StagedInstaller);
  for(const button of screen.getAllByRole('button')) expect(button).toBeDisabled();
});
it('shows unreadable records while preserving valid recovery actions',async()=>{
  const op={operationId:'valid-record',status:'copying',diskNumber:0,temporaryBytes:8*1024**3,message:'Interrupted'};
  state.set({pending:false,snapshot:{status:'complete',stagedIso:{operations:[op],recordErrors:[{operationId:'incomplete-record',message:'Missing state.json'}]}}});
  render(StagedInstaller,{recoveryOnly:true});
  expect(screen.getByRole('alert')).toHaveTextContent('could not be identified safely');
  expect(screen.getByText('incomplete-record: Missing state.json')).toBeInTheDocument();
  expect(screen.getAllByRole('button',{name:'Remove temporary installer'})).toHaveLength(1);
  await fireEvent.click(screen.getByRole('button',{name:'Remove temporary installer'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('cleanup',null,op.operationId);
});
it('allows real inspection only in a testing build with a verified ISO',async()=>{
  state.set({pending:false,snapshot:{status:'idle',stagedTesting:true}});
  const view=render(StagedInstaller,{sourceReady:false});
  expect(screen.getByRole('button',{name:'Check available space'})).toBeDisabled();
  await view.rerender({sourceReady:true});
  expect(screen.getByText('Test build · real disk changes')).toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'Check available space'}));
  expect(calls.stagedIso).toHaveBeenCalledWith('inspect');
});
it('prepares again only after reviewed cleanup completes',async()=>{
  const op={operationId:'id',status:'copying',diskNumber:0,temporaryBytes:8*1024**3};
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedRecovery:{operations:[op],recordErrors:[]}}});
  const prepareAgain=vi.fn();
  render(StagedInstaller,{recoveryOnly:true,sourceReady:true,prepareAgain});
  await fireEvent.click(screen.getByRole('button',{name:'Prepare again'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('cleanup',null,'id');
  expect(prepareAgain).not.toHaveBeenCalled();
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedRecovery:{operations:[],recordErrors:[]},stagedIso:{operationId:'id',status:'cleaned'}}});
  await import('svelte').then(({tick})=>tick());
  expect(prepareAgain).toHaveBeenCalledOnce();
  expect(calls.stagedIso).toHaveBeenLastCalledWith('inspect');
});
const encrypted={diskNumber:0,diskUniqueId:'windows-disk',label:'Shrink partition 3',maximumLinuxBytes:100*1024**3,target:{target_kind:'shrink',partition_number:3,partition_guid:'guid'},encryption:[{driveLetter:'C:',isOsVolume:true,conversionStatus:1,protectionStatus:1,lockStatus:0}]};
const free={diskNumber:1,diskUniqueId:'second-disk',label:'Unallocated space',maximumLinuxBytes:80*1024**3,target:{target_kind:'free',start_offset_bytes:1024**2}};
const inspected=()=>({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[free,encrypted],blocked:[],temporaryBytes:8*1024**3,minimumLinuxBytes:40*1024**3}}});
it('allows encrypted space selection and defers suspension to confirmed staging',async()=>{
  state.set(inspected());
  render(StagedInstaller,{sourceReady:true});
  expect(screen.queryByRole('complementary',{name:'BitLocker preparation'})).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:/Disk 0/}));
  expect(screen.getByRole('complementary',{name:'BitLocker preparation'})).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Review changes'})).toBeEnabled();
  expect(calls.prepareBitLocker).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:encrypted.target,linuxBytes:100*1024**3});
  expect(calls.prepareBitLocker).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button',{name:/Disk 1/}));
  expect(screen.queryByRole('complementary',{name:'BitLocker preparation'})).not.toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Review changes'})).toBeEnabled();
});
it('preserves the chosen allocation across status polling and submits the exact disk and extent',async()=>{
  state.set(inspected());
  render(StagedInstaller,{sourceReady:true});
  await fireEvent.click(screen.getByRole('button',{name:/Disk 1/}));
  await fireEvent.input(screen.getByRole('spinbutton'),{target:{value:'60'}});
  state.set(structuredClone(inspected()));
  await import('svelte').then(({tick})=>tick());
  expect(screen.getByRole('spinbutton')).toHaveValue(60);
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:1,diskUniqueId:'second-disk',target:free.target,linuxBytes:60*1024**3});
});
it('keeps the selected disk after refresh',async()=>{
  state.set(inspected());
  render(StagedInstaller,{sourceReady:true});
  await fireEvent.click(screen.getByRole('button',{name:/Disk 0/}));
  await fireEvent.click(screen.getByRole('button',{name:'Refresh'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('inspect');
  const cleared={...free,diskNumber:0,diskUniqueId:'windows-disk'};
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[cleared],blocked:[]}}});
  await import('svelte').then(({tick})=>tick());
  expect(screen.queryByRole('complementary',{name:'BitLocker preparation'})).not.toBeInTheDocument();
  expect(screen.getByRole('button',{name:/Disk 0/})).toHaveAttribute('aria-pressed','true');
  expect(screen.getByRole('button',{name:'Review changes'})).toBeEnabled();
});
it('invalidates destinations when refreshing fails',async()=>{
  state.set(inspected());
  render(StagedInstaller,{sourceReady:true});
  await fireEvent.click(screen.getByRole('button',{name:/Disk 1/}));
  await fireEvent.click(screen.getByRole('button',{name:'Refresh'}));
  state.set({...inspected(),error:'Inspection failed'});
  await import('svelte').then(({tick})=>tick());
  expect(screen.queryByRole('button',{name:'Review changes'})).not.toBeInTheDocument();
});
