import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, expect, it, vi } from 'vitest';
import StagedInstaller from './StagedInstaller.svelte';
import { setup } from './setup';
const calls=vi.hoisted(()=>({stagedIso:vi.fn(),prepareBitLocker:vi.fn(),respondStagedReview:vi.fn(),cancel:vi.fn()}));
vi.mock('./setup',async importOriginal=>{
  const actual=await importOriginal<typeof import('./setup')>();
  const {writable}=await import('svelte/store');
  return {...actual,setup:{...writable({pending:false,snapshot:null}),...calls}};
});
const state=setup as typeof setup & {set:(value:unknown)=>void};
beforeEach(()=>{vi.clearAllMocks();state.set({pending:false,snapshot:null});});
const tick=()=>import('svelte').then(({tick})=>tick());
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
  expect(screen.getByText('Download the image above first.')).toBeInTheDocument();
  await view.rerender({sourceReady:true});
  expect(screen.getByText('Test build')).toBeInTheDocument();
  expect(screen.getByText('Windows will ask for administrator access. Nothing is changed.')).toBeInTheDocument();
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
  await tick();
  expect(prepareAgain).toHaveBeenCalledOnce();
  expect(calls.stagedIso).toHaveBeenLastCalledWith('inspect');
});
it('shows a placeholder map while the first disk read runs',async()=>{
  state.set({pending:false,snapshot:{status:'idle',stagedTesting:true}});
  render(StagedInstaller,{sourceReady:true});
  await fireEvent.click(screen.getByRole('button',{name:'Check available space'}));
  state.set({pending:false,snapshot:{status:'running',stage:'loading',message:'Loading Windows disk tools...',stagedTesting:true}});
  await tick();
  expect(screen.getByText('Reading disks…')).toBeInTheDocument();
  // The provider's progress stays out of sight but reaches screen readers.
  expect(screen.getByRole('status')).toHaveTextContent('Loading Windows disk tools...');
  // Stopping a check isn't stopping a preparation.
  state.set({pending:false,snapshot:{status:'running',stage:'loading',message:'Loading Windows disk tools...',stagedTesting:true,cancelAvailable:true,cancelRequested:false}});
  await tick();
  expect(screen.getByRole('button',{name:'Stop checking'})).toBeEnabled();
  expect(screen.queryByRole('button',{name:'Stop preparing'})).not.toBeInTheDocument();
});
const GiB=1024**3, MiB=1024**2;
const encrypted={diskNumber:0,diskUniqueId:'windows-disk',label:'Shrink partition 3',largestFreeAfterStagingBytes:0,target:{target_kind:'shrink',partition_number:3,partition_guid:'guid'},encryption:[{driveLetter:'C:',isOsVolume:true,conversionStatus:1,protectionStatus:1,lockStatus:0}]};
const free={diskNumber:1,diskUniqueId:'second-disk',label:'Unallocated space',largestFreeAfterStagingBytes:80*GiB,target:{target_kind:'free',start_offset_bytes:MiB}};
const inspected=()=>({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[free,encrypted],blocked:[],temporaryBytes:8*GiB,minimumLinuxBytes:32*GiB}}});
const fullDisk={diskNumber:0,diskUniqueId:'windows-disk',diskSizeBytes:500*GiB,unallocatedBytes:0,largestFreeBytes:0,regions:[{kind:'partition',partitionNumber:3,partitionGuid:'guid',offsetBytes:MiB,sizeBytes:500*GiB,label:'C:',fileSystem:'NTFS',freeBytes:100*GiB,resizeState:'unchecked',isWindows:true}]};
const checkedC={...fullDisk.regions[0],resizeState:'checked',maximumReleaseBytes:108*GiB,reserveBytes:50*GiB};
it('leaves tiny gaps off the map and counts them in the table total',async()=>{
  const gap={kind:'free',offsetBytes:2*MiB,sizeBytes:MiB,label:'Unallocated space'};
  const withGap={...fullDisk,unallocatedBytes:MiB,regions:[fullDisk.regions[0],gap]};
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[],blocked:[],disks:[withGap],minimumLinuxBytes:40*GiB,temporaryBytes:8*GiB}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getAllByRole('radio')).toHaveLength(1);
  await fireEvent.click(screen.getByRole('button',{name:'All partitions (1)'}));
  expect(screen.getByRole('table')).toHaveTextContent('Free space outside partitions: 1 MiB');
  expect(screen.queryByRole('rowheader',{name:'Free space'})).not.toBeInTheDocument();
});
it('explains a partition that cannot provide enough space without waiting for a shrink limit',async()=>{
  const scarce={...fullDisk,regions:[{...fullDisk.regions[0],freeBytes:1.9*GiB,reserveBytes:50*GiB,resizeState:'insufficient'}]};
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[],blocked:[],disks:[scarce],minimumLinuxBytes:40*GiB,temporaryBytes:7.3*GiB}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  const line='C: needs 55.4 GiB more free space. Free up space in Windows, then refresh.';
  expect(screen.getByRole('radio',{name:line})).toBeChecked();
  expect(screen.getByText(line,{selector:'p'})).toBeInTheDocument();
  expect(screen.getByText('No space can be used yet.')).toBeInTheDocument();
  // The line already says to free up space; without the USB route there is no other help.
  expect(screen.queryByText('What can help?')).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'Refresh disks'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('inspect');
});
it('shows a full disk while Windows measures its resize limit',async()=>{
  state.set({pending:false,snapshot:{status:'running',stage:'inspecting',message:'Disks are ready. Checking Windows resize limits...',stagedTesting:true,stagedIso:{choices:[],blocked:[],disks:[fullDisk],minimumLinuxBytes:40*GiB,temporaryBytes:8*GiB}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByRole('heading',{name:'Disk 0 · 500 GiB'})).toBeInTheDocument();
  expect(screen.getByRole('radio',{name:'Checking how much C: can shrink. This can take a few minutes.'})).toBeChecked();
  expect(screen.queryByText('No space can be used yet.')).not.toBeInTheDocument();
  expect(screen.getByRole('status')).toHaveTextContent('Disks are ready. Checking Windows resize limits...');
  expect(screen.queryByRole('button',{name:'Review changes'})).not.toBeInTheDocument();
  expect(calls.stagedIso).not.toHaveBeenCalled();
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[encrypted],blocked:[],disks:[{...fullDisk,regions:[checkedC]}],minimumLinuxBytes:40*GiB,temporaryBytes:8*GiB}}});
  await tick();
  expect(screen.getByRole('radio',{name:'C: can give Omarchy up to 100 GiB'})).toBeChecked();
  expect(screen.getByRole('status')).toHaveTextContent('Disk check finished.');
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:encrypted.target,linuxBytes:64*GiB});
});
it('keeps browsing available while measured limits arrive in one inspection',async()=>{
  const other={...fullDisk,diskNumber:1,diskUniqueId:'other-disk',regions:[{...fullDisk.regions[0],partitionGuid:'other-guid',label:'D:',isWindows:false}]};
  const base={choices:[],blocked:[],disks:[fullDisk,other],minimumLinuxBytes:40*GiB,temporaryBytes:8*GiB};
  state.set({pending:false,snapshot:{status:'running',stage:'inspecting',stagedTesting:true,stagedIso:base}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(calls.stagedIso).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button',{name:'Disk 1 · 500 GiB'}));
  expect(screen.getByRole('radio',{name:/Checking how much D: can shrink/})).toBeChecked();
  state.set({pending:false,snapshot:{status:'running',stage:'analyzing-resize',stagedTesting:true,stagedIso:{...base,choices:[encrypted],disks:[{...fullDisk,regions:[checkedC]},other]}}});
  await tick();
  expect(calls.stagedIso).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button',{name:'Disk 0 · 500 GiB'}));
  expect(screen.getByRole('radio',{name:'C: can give Omarchy up to 100 GiB'})).toBeChecked();
  expect(screen.getByRole('button',{name:'Review changes'})).toBeDisabled();
  state.set({pending:false,snapshot:{status:'complete',stage:'complete',stagedTesting:true,stagedIso:{...base,choices:[encrypted],disks:[{...fullDisk,regions:[checkedC]},{...other,regions:[{...other.regions[0],resizeState:'checked',maximumReleaseBytes:10*GiB,reserveBytes:50*GiB}]}]}}});
  await tick();
  expect(screen.getByRole('radio',{name:'C: can give Omarchy up to 100 GiB'})).toBeChecked();
  expect(screen.getByRole('button',{name:'Review changes'})).toBeEnabled();
  expect(calls.stagedIso).not.toHaveBeenCalled();
});
it('shows a measured shortfall with a recheck and the partition table',async()=>{
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[],blocked:[],disks:[{...fullDisk,regions:[{...checkedC,maximumReleaseBytes:6*GiB}]}],minimumLinuxBytes:32*GiB,temporaryBytes:8*GiB}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByText("Windows can't move some files near the end of C:, so it can release only 6 GiB.",{selector:'p'})).toBeInTheDocument();
  expect(screen.getByText('No space can be used yet.')).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'All partitions (1)'})).toHaveAttribute('aria-expanded','false');
  expect(screen.queryByRole('button',{name:'Review changes'})).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'Recheck C:'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('inspect',null,null,{diskNumber:0,diskUniqueId:'windows-disk',partitionNumber:3,partitionGuid:'guid'});
});
it('allows encrypted space selection and defers suspension to confirmed staging',async()=>{
  state.set(inspected());
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByRole('button',{name:'Disk 0'})).toHaveAttribute('aria-pressed','true');
  expect(screen.queryByText(/BitLocker/)).not.toBeInTheDocument();
  expect(screen.getByText('In the installer, delete Windows to make room for Omarchy.')).toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:encrypted.target});
  expect(calls.prepareBitLocker).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button',{name:'Disk 1'}));
  expect(screen.getByText('Omarchy can use 80 GiB.')).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Review changes'})).toBeEnabled();
});
it('preserves the chosen disk across status polling and submits the exact target',async()=>{
  state.set(inspected());
  render(StagedInstaller,{sourceReady:true});
  await fireEvent.click(screen.getByRole('button',{name:/Disk 1/}));
  state.set(structuredClone(inspected()));
  await tick();
  expect(screen.getByRole('button',{name:/Disk 1/})).toHaveAttribute('aria-pressed','true');
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:1,diskUniqueId:'second-disk',target:free.target});
});
it('keeps the selected disk after refresh',async()=>{
  state.set(inspected());
  render(StagedInstaller,{sourceReady:true});
  await fireEvent.click(screen.getByRole('button',{name:/Disk 0/}));
  await fireEvent.click(screen.getByRole('button',{name:'Refresh'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('inspect');
  const cleared={...free,diskNumber:0,diskUniqueId:'windows-disk'};
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[cleared],blocked:[]}}});
  await tick();
  expect(screen.getByRole('heading',{name:'Disk 0'})).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Review changes'})).toBeEnabled();
});
it('invalidates destinations when refreshing fails',async()=>{
  state.set(inspected());
  render(StagedInstaller,{sourceReady:true});
  await fireEvent.click(screen.getByRole('button',{name:/Disk 1/}));
  await fireEvent.click(screen.getByRole('button',{name:'Refresh'}));
  state.set({...inspected(),error:'Inspection failed'});
  await tick();
  expect(screen.queryByRole('button',{name:'Review changes'})).not.toBeInTheDocument();
});
// EFI, C: and free space after it, as on a disk with room left at the end.
const efi={kind:'partition',partitionNumber:1,partitionGuid:'efi-guid',offsetBytes:MiB,sizeBytes:100*MiB,label:'EFI system',fileSystem:'FAT32',resizeState:'unavailable',resizeReason:'System or unsupported partition; kept unchanged.',isWindows:false};
const roomy={...fullDisk,unallocatedBytes:100*GiB,largestFreeBytes:100*GiB,regions:[efi,{...checkedC,offsetBytes:101*MiB,sizeBytes:400*GiB},{kind:'free',offsetBytes:101*MiB+400*GiB,sizeBytes:100*GiB-101*MiB,label:'Unallocated space'}]};
const freeAtEnd={diskNumber:0,diskUniqueId:'windows-disk',label:'Unallocated space',largestFreeAfterStagingBytes:100*GiB-101*MiB-8*GiB,target:{target_kind:'free',start_offset_bytes:101*MiB+400*GiB}};
it('chooses free space first and selects partitions with the keyboard',async()=>{
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[freeAtEnd,encrypted],blocked:[],disks:[roomy],minimumLinuxBytes:32*GiB,temporaryBytes:8*GiB}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  const map=screen.getByRole('radiogroup',{name:'Partitions on Disk 0'});
  const freeSegment=screen.getByRole('radio',{name:'Free space · 99.9 GiB'});
  expect(freeSegment).toBeChecked();
  expect(freeSegment).toHaveAttribute('tabindex','0');
  expect(map).toContainElement(freeSegment);
  expect(screen.getByText('Omarchy can use 91.9 GiB.')).toBeInTheDocument();
  expect(screen.getByText('Temporary installer 8 GiB')).toBeInTheDocument();
  expect(screen.queryByRole('slider')).not.toBeInTheDocument();
  await fireEvent.keyDown(freeSegment,{key:'ArrowLeft'});
  const windows=screen.getByRole('radio',{name:'C: can give Omarchy up to 100 GiB'});
  expect(windows).toBeChecked();
  expect(windows).toHaveFocus();
  await fireEvent.keyDown(windows,{key:'Home'});
  expect(screen.getByRole('radio',{name:'EFI system · 100 MiB · kept as is'})).toBeChecked();
  expect(screen.getByText('Space comes from C:.')).toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:encrypted.target,linuxBytes:64*GiB});
  await fireEvent.keyDown(screen.getByRole('radio',{name:/EFI system/}),{key:'End'});
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:freeAtEnd.target});
});
it('keeps the space source when a later measurement would move the default',async()=>{
  const data={...fullDisk.regions[0],partitionNumber:4,partitionGuid:'data-guid',offsetBytes:101*MiB+400*GiB,sizeBytes:1000*GiB,label:'D:',freeBytes:900*GiB,isWindows:false};
  const dataChoice={...encrypted,label:'Resize D:',target:{target_kind:'shrink',partition_number:4,partition_guid:'data-guid'}};
  const disk=(d:object)=>({...fullDisk,diskSizeBytes:1500*GiB,regions:[efi,{...checkedC,offsetBytes:101*MiB,sizeBytes:400*GiB},d]});
  state.set({pending:false,snapshot:{status:'running',stage:'analyzing-resize',stagedTesting:true,stagedIso:{choices:[encrypted],blocked:[],disks:[disk(data)],minimumLinuxBytes:32*GiB,temporaryBytes:8*GiB}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  await fireEvent.click(screen.getByRole('radio',{name:/EFI system/}));
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[encrypted,dataChoice],blocked:[],disks:[disk({...data,resizeState:'checked',maximumReleaseBytes:700*GiB,reserveBytes:100*GiB})],minimumLinuxBytes:32*GiB,temporaryBytes:8*GiB}}});
  await tick();
  expect(screen.getByRole('radio',{name:/EFI system/})).toBeChecked();
  expect(screen.getByText('Space comes from C:.')).toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:encrypted.target,linuxBytes:64*GiB});
});
it('replaces the choose screen while a reviewed plan waits',async()=>{
  state.set({pending:false,snapshot:{...measured().snapshot,status:'running',stage:'staged-installer',stagedReview:{summary:'Prepare the installer on Disk 0?'}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByRole('region',{name:'Review changes'})).toBeInTheDocument();
  expect(screen.queryByRole('radiogroup')).not.toBeInTheDocument();
  expect(screen.queryByRole('button',{name:'Refresh'})).not.toBeInTheDocument();
});
it('names a target that is not the Windows partition',async()=>{
  const data={...fullDisk,regions:[{...checkedC,label:'D:',isWindows:false}]};
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[encrypted],blocked:[],disks:[data],minimumLinuxBytes:32*GiB,temporaryBytes:8*GiB}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByRole('radio',{name:'Keep D:'})).toBeChecked();
  await fireEvent.click(screen.getByRole('radio',{name:'Replace D:'}));
  expect(screen.getByText('In the installer, delete D: to make room for Omarchy.')).toBeInTheDocument();
});
it('shows why a disk cannot be used in place of the map',async()=>{
  const blocked={diskNumber:1,diskUniqueId:'mbr-disk',diskSizeBytes:2048*GiB,reason:'Disk 1 uses MBR partitioning. Only GPT disks are supported.'};
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[encrypted],blocked:[blocked],disks:[{...fullDisk,regions:[checkedC]}],minimumLinuxBytes:32*GiB,temporaryBytes:8*GiB}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByRole('button',{name:'Disk 0 · 500 GiB'})).toHaveAttribute('aria-pressed','true');
  await fireEvent.click(screen.getByRole('button',{name:"Disk 1 · 2 TiB · can't use"}));
  expect(screen.getByText(blocked.reason)).toBeInTheDocument();
  expect(screen.queryByRole('radiogroup')).not.toBeInTheDocument();
  expect(screen.queryByRole('button',{name:'Review changes'})).not.toBeInTheDocument();
});
const measured=()=>({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[encrypted],blocked:[],disks:[{...fullDisk,regions:[checkedC]}],minimumLinuxBytes:32*GiB,temporaryBytes:8*GiB,secureBoot:true}}});
it('keeps Windows with a chosen Omarchy size or replaces it later',async()=>{
  state.set(measured());
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByRole('radio',{name:'Keep Windows'})).toBeChecked();
  expect(screen.getByText('GiB · 32–100')).toBeInTheDocument();
  await fireEvent.input(screen.getByRole('spinbutton',{name:'Space for Omarchy'}),{target:{value:'40'}});
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:encrypted.target,linuxBytes:40*GiB});
  await fireEvent.click(screen.getByRole('radio',{name:'Replace Windows'}));
  expect(screen.getByText('In the installer, delete Windows to make room for Omarchy.')).toBeInTheDocument();
  expect(screen.queryByRole('spinbutton')).not.toBeInTheDocument();
  expect(screen.queryByRole('slider')).not.toBeInTheDocument();
  expect(screen.getByText('Windows (C:) 492 GiB')).toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:encrypted.target});
});
it('moves the handle in step with the size field',async()=>{
  state.set(measured());
  render(StagedInstaller,{sourceReady:true});
  await tick();
  const slider=screen.getByRole('slider',{name:'Space for Omarchy'}) as HTMLInputElement;
  const field=screen.getByRole('spinbutton',{name:'Space for Omarchy'});
  expect(slider).toHaveAttribute('min','32');
  expect(slider).toHaveAttribute('max','100');
  expect(slider).toHaveAttribute('dir','rtl');
  expect(slider).toHaveAttribute('aria-valuetext','Omarchy 64 GiB, Windows (C:) keeps 428 GiB');
  // jsdom has no layout, so every bar label is in the legend.
  expect(screen.getByText('Omarchy 64 GiB')).toBeInTheDocument();
  await fireEvent.input(slider,{target:{value:'90'}});
  expect(field).toHaveValue(90);
  expect(screen.getByText('Omarchy 90 GiB')).toBeInTheDocument();
  expect(slider).toHaveAttribute('aria-valuetext','Omarchy 90 GiB, Windows (C:) keeps 402 GiB');
  expect(screen.getByText('Windows (C:) keeps 402 GiB')).toBeInTheDocument();
  await fireEvent.keyDown(slider,{key:'PageUp'});
  expect(field).toHaveValue(100);
  expect(slider.value).toBe('100');
  await fireEvent.keyDown(slider,{key:'PageDown'});
  expect(field).toHaveValue(90);
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:encrypted.target,linuxBytes:90*GiB});
});
it('clamps a typed size when leaving the field, so the submitted size is the one shown',async()=>{
  state.set(measured());
  render(StagedInstaller,{sourceReady:true});
  await tick();
  const slider=screen.getByRole('slider',{name:'Space for Omarchy'}) as HTMLInputElement;
  const field=screen.getByRole('spinbutton',{name:'Space for Omarchy'});
  // Typing 40 passes through 4, which must not jump to the minimum.
  await fireEvent.input(field,{target:{value:'4'}});
  expect(field).toHaveValue(4);
  expect(slider.value).toBe('64');
  await fireEvent.change(field);
  expect(field).toHaveValue(32);
  expect(slider.value).toBe('32');
  await fireEvent.input(field,{target:{value:'500'}});
  await fireEvent.change(field);
  expect(field).toHaveValue(100);
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:encrypted.target,linuxBytes:100*GiB});
});
it("explains the space Windows can't release when it is selected",async()=>{
  const roomy={...checkedC,freeBytes:200*GiB};
  state.set({pending:false,snapshot:{...measured().snapshot,stagedIso:{...measured().snapshot.stagedIso,disks:[{...fullDisk,regions:[roomy]}]}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  const zone=screen.getByRole('button',{name:'Reserved 92 GiB'});
  const line=screen.getByText("50 GiB stays free for Windows, and some files near the end of C: can't be moved.");
  expect(zone).toHaveAttribute('aria-expanded','false');
  expect(line).not.toBeVisible();
  await fireEvent.click(zone);
  expect(zone).toHaveAttribute('aria-expanded','true');
  expect(line).toBeVisible();
});
it('keeps the space being edited when a later measurement would move the default',async()=>{
  const data={...fullDisk.regions[0],partitionNumber:4,partitionGuid:'data-guid',offsetBytes:101*MiB+400*GiB,sizeBytes:1000*GiB,label:'D:',freeBytes:900*GiB,isWindows:false};
  const dataChoice={...encrypted,label:'Resize D:',target:{target_kind:'shrink',partition_number:4,partition_guid:'data-guid'}};
  const disk=(d:object)=>({...fullDisk,diskSizeBytes:1500*GiB,regions:[efi,{...checkedC,offsetBytes:101*MiB,sizeBytes:400*GiB},d]});
  state.set({pending:false,snapshot:{status:'running',stage:'analyzing-resize',stagedTesting:true,stagedIso:{choices:[encrypted],blocked:[],disks:[disk(data)],minimumLinuxBytes:32*GiB,temporaryBytes:8*GiB}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  await fireEvent.input(screen.getByRole('spinbutton',{name:'Space for Omarchy'}),{target:{value:'40'}});
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[encrypted,dataChoice],blocked:[],disks:[disk({...data,resizeState:'checked',maximumReleaseBytes:700*GiB,reserveBytes:100*GiB})],minimumLinuxBytes:32*GiB,temporaryBytes:8*GiB}}});
  await tick();
  expect(screen.getByRole('radio',{name:'C: can give Omarchy up to 100 GiB'})).toBeChecked();
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:encrypted.target,linuxBytes:40*GiB});
});
// C: measured and D: still being checked, or both measured, on one disk.
const dataRegion={...fullDisk.regions[0],partitionNumber:4,partitionGuid:'data-guid',offsetBytes:101*MiB+400*GiB,sizeBytes:1000*GiB,label:'D:',freeBytes:900*GiB,isWindows:false};
const dataMeasured={...dataRegion,resizeState:'checked',maximumReleaseBytes:700*GiB,reserveBytes:100*GiB};
const dataChoice={...encrypted,label:'Resize D:',target:{target_kind:'shrink',partition_number:4,partition_guid:'data-guid'}};
const twoPartitions=(data:object,status:'running'|'complete')=>({pending:false,snapshot:{status,stage:status==='running' ? 'analyzing-resize' : 'complete',stagedTesting:true,stagedIso:{choices:status==='running' ? [encrypted] : [encrypted,dataChoice],blocked:[],disks:[{...fullDisk,diskSizeBytes:1500*GiB,regions:[efi,{...checkedC,offsetBytes:101*MiB,sizeBytes:400*GiB},data]}],minimumLinuxBytes:32*GiB,temporaryBytes:8*GiB}}});
it('pins the source shown in the editor when it stands in for an earlier pick',async()=>{
  state.set(twoPartitions(dataMeasured,'complete'));
  render(StagedInstaller,{sourceReady:true});
  await tick();
  await fireEvent.click(screen.getByRole('radio',{name:'D: can give Omarchy up to 692 GiB'}));
  await fireEvent.click(screen.getByRole('button',{name:'Refresh'}));
  state.set(twoPartitions(dataRegion,'running'));
  await tick();
  expect(screen.getByText('Space comes from C:.')).toBeInTheDocument();
  await fireEvent.input(screen.getByRole('spinbutton',{name:'Space for Omarchy'}),{target:{value:'40'}});
  state.set(twoPartitions(dataMeasured,'complete'));
  await tick();
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:encrypted.target,linuxBytes:40*GiB});
});
it('keeps a half-typed size when a measurement arrives',async()=>{
  state.set(twoPartitions(dataRegion,'running'));
  render(StagedInstaller,{sourceReady:true});
  await tick();
  const field=screen.getByRole('spinbutton',{name:'Space for Omarchy'});
  await fireEvent.input(field,{target:{value:'4'}});
  state.set(twoPartitions(dataMeasured,'complete'));
  await tick();
  expect(screen.getByRole('spinbutton',{name:'Space for Omarchy'})).toBe(field);
  expect(field).toHaveValue(4);
  await fireEvent.change(field);
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:encrypted.target,linuxBytes:32*GiB});
});
it('offers only replacing when Windows cannot give enough space to keep it',async()=>{
  state.set({pending:false,snapshot:{...measured().snapshot,stagedIso:{...measured().snapshot.stagedIso,disks:[{...fullDisk,regions:[{...checkedC,maximumReleaseBytes:30*GiB}]}]}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByRole('radio',{name:'C: has room for the installer only'})).toBeChecked();
  // Keeping is shown but unavailable, with the numbers; Windows can't move
  // the files in the way, so freeing space isn't suggested.
  expect(screen.getByText('Windows can give only 22 GiB; Omarchy needs 32 GiB.')).toBeInTheDocument();
  expect(screen.getByRole('radio',{name:'Keep Windows'})).toBeDisabled();
  expect(screen.getByRole('radio',{name:'Replace Windows'})).toBeChecked();
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('stage',{diskNumber:0,diskUniqueId:'windows-disk',target:encrypted.target});
});
// The plan the helper reviews for keeping Windows with 64 GiB on `measured()`.
const reviewed=(layoutSha256='layout')=>({summary:'EXPERIMENTAL TESTING BUILD: real disk changes.\n\nPrepare the installer on Disk 0?\n\nShrink C: from 500.0 to 428.0 GiB. Leave 64 GiB unallocated for Omarchy after it.',review:{operationId:'11111111-1111-4111-8111-111111111111',diskNumber:0,diskSizeBytes:500*GiB,layoutSha256,targetKind:'shrink',shrink:{label:'C:',partitionGuid:'guid',offsetBytes:MiB,beforeSizeBytes:500*GiB,afterSizeBytes:428*GiB},temporary:{offsetBytes:MiB+428*GiB,sizeBytes:8*GiB},omarchy:{offsetBytes:MiB+436*GiB,sizeBytes:64*GiB},largestFreeAfterBytes:64*GiB,minimumOmarchyBytes:32*GiB,bitLocker:['C:'],warning:'experimental'}});
const reviewing=(stagedReview:object|null,extra:object={})=>{
  const base=measured().snapshot;
  return {pending:false,snapshot:{...base,status:'running',stage:'staged-installer',stagedIso:{...base.stagedIso,disks:[{...fullDisk,layoutSha256:'layout',regions:[checkedC]}]},stagedReview,...extra}};
};
it('moves the Secure Boot advice from the choose screen to Review',async()=>{
  state.set(measured());
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.queryByText(/Secure Boot/)).not.toBeInTheDocument();
  state.set(reviewing(reviewed()));
  await tick();
  expect(screen.getByRole('region',{name:'Review changes'})).toHaveTextContent('Next: turn off Secure Boot in firmware settings.');
});
it('reviews the plan with Now and After bars, short bullets and one warning',async()=>{
  state.set(reviewing(reviewed()));
  render(StagedInstaller,{sourceReady:true});
  await tick();
  const review=screen.getByRole('region',{name:'Review changes'});
  expect(screen.getByRole('heading',{name:'Prepare Disk 0?'})).toBeInTheDocument();
  expect(review.querySelectorAll('.bar')).toHaveLength(2);
  expect(screen.getAllByRole('listitem').map(item=>item.textContent)).toEqual([
    'C: shrinks from 500 to 428 GiB. Your files stay.',
    'Omarchy can use 64 GiB. The 8 GiB installer is removed later.',
    'BitLocker on C: pauses until you resume it. Files stay encrypted.',
    'Next: turn off Secure Boot in firmware settings.'
  ]);
  expect(review).toHaveTextContent('Test build: real disk changes. This image may not start or install.');
  expect(screen.getByText('Full details')).toBeInTheDocument();
  expect(review).toHaveTextContent('Shrink C: from 500.0 to 428.0 GiB.');
  expect(screen.queryByRole('button',{name:'Review changes'})).not.toBeInTheDocument();
});
it('shows the bullets without bars when the disk changed since the check',async()=>{
  state.set(reviewing(reviewed('changed')));
  render(StagedInstaller,{sourceReady:true});
  await tick();
  const review=screen.getByRole('region',{name:'Review changes'});
  expect(review.querySelector('.bars')).toBeNull();
  expect(review).toHaveTextContent('C: shrinks from 500 to 428 GiB. Your files stay.');
});
it('returns to the choose screen with the selection intact when Review is declined',async()=>{
  state.set({pending:false,snapshot:{...reviewing(null).snapshot,status:'complete',stage:'complete'}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  await fireEvent.input(screen.getByRole('spinbutton',{name:'Space for Omarchy'}),{target:{value:'40'}});
  state.set(reviewing(reviewed()));
  await tick();
  await fireEvent.click(screen.getByRole('button',{name:'Cancel'}));
  expect(calls.respondStagedReview).toHaveBeenLastCalledWith(false);
  // The helper stops; the app reports a cancelled step, not a failure.
  state.set({pending:false,snapshot:{...reviewing(null).snapshot,status:'cancelled',stage:'staged-installer',message:'Nothing was changed.',error:null}});
  await tick();
  expect(screen.queryByRole('region',{name:'Review changes'})).not.toBeInTheDocument();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  expect(screen.getByRole('spinbutton',{name:'Space for Omarchy'})).toHaveValue(40);
  expect(screen.getByRole('button',{name:'Review changes'})).toBeEnabled();
  expect(screen.getByText('Nothing was changed.')).toBeInTheDocument();
});
it('says what Review waits for beside the button',async()=>{
  state.set(measured());
  render(StagedInstaller,{sourceReady:true});
  await tick();
  await fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  state.set({...reviewing(null),snapshot:{...reviewing(null).snapshot,message:'Loading Windows disk tools...'}});
  await tick();
  expect(screen.getByText('Planning the changes…')).toBeInTheDocument();
  expect(screen.queryByText('Loading Windows disk tools...')).not.toBeInTheDocument();
  state.set(reviewing(reviewed()));
  await tick();
  expect(screen.queryByText('Planning the changes…')).not.toBeInTheDocument();
});
it('shows preparation progress where the review was',async()=>{
  state.set(reviewing(reviewed()));
  render(StagedInstaller,{sourceReady:true});
  await tick();
  await fireEvent.click(screen.getByRole('button',{name:'Prepare installer'}));
  expect(calls.respondStagedReview).toHaveBeenLastCalledWith(true);
  state.set(reviewing(null,{stage:'copying-installer',message:'Copying and verifying the installer...',bytes:2*GiB,totalBytes:6*GiB,cancelAvailable:true,cancelRequested:false}));
  await tick();
  const panel=screen.getByRole('region',{name:'Preparing the installer'});
  expect(panel).toHaveTextContent('Copying and verifying the installer...');
  expect(screen.getByRole('progressbar',{name:'Copying the installer'})).toHaveAttribute('value',String(2*GiB));
  expect(screen.getAllByRole('button',{name:'Stop preparing'})).toHaveLength(1);
  expect(screen.queryByRole('radiogroup')).not.toBeInTheDocument();
});
it('keeps progress and Stop on the home screen during preparation',async()=>{
  const op={operationId:'11111111-1111-4111-8111-111111111111',status:'copying',diskNumber:0,temporaryBytes:8*GiB,message:''};
  state.set(reviewing({...reviewed(),approved:true},{kind:null,stage:'copying-installer',message:'Copying and verifying the installer...',bytes:2*GiB,totalBytes:6*GiB,cancelAvailable:true,cancelRequested:false,stagedRecovery:{operations:[op],recordErrors:[]}}));
  render(StagedInstaller,{recoveryOnly:true,sourceReady:true});
  await tick();
  expect(screen.getByText('Copying and verifying the installer...')).toBeInTheDocument();
  expect(screen.getByRole('progressbar',{name:'Copying the installer'})).toHaveAttribute('value',String(2*GiB));
  expect(screen.getByRole('button',{name:'Stop preparing'})).toBeEnabled();
  // The installer being prepared isn't one that didn't finish.
  expect(screen.getByRole('heading',{name:'Preparing the installer · Disk 0'})).toBeInTheDocument();
  expect(screen.getByText('Keep this window open.')).toBeInTheDocument();
  expect(screen.queryByRole('button',{name:'Remove temporary installer'})).not.toBeInTheDocument();
});
it('shows the preparing panel when opened during preparation',async()=>{
  const op={operationId:'11111111-1111-4111-8111-111111111111',status:'copying',diskNumber:0,temporaryBytes:8*GiB,message:''};
  state.set(reviewing({...reviewed(),approved:true},{stage:'copying-installer',message:'Copying and verifying the installer...',bytes:2*GiB,totalBytes:6*GiB,cancelAvailable:true,cancelRequested:false,stagedRecovery:{operations:[op],recordErrors:[]}}));
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByRole('region',{name:'Preparing the installer'})).toHaveTextContent('Copying and verifying the installer...');
  // The panel stands in for the new installer's card.
  expect(screen.queryByRole('article')).not.toBeInTheDocument();
  expect(screen.queryByRole('region',{name:'Review changes'})).not.toBeInTheDocument();
  expect(screen.queryByRole('radiogroup')).not.toBeInTheDocument();
});
it('reviews the staging plan in the app before any disk change',async()=>{
  state.set({pending:false,snapshot:{...measured().snapshot,status:'running',stagedReview:{summary:'Prepare the installer on Disk 0?\n\nShrink C: from 500.0 to 428.0 GiB.'}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  const review=screen.getByRole('region',{name:'Review changes'});
  expect(review).toHaveTextContent('Shrink C: from 500.0 to 428.0 GiB.');
  expect(screen.queryByRole('button',{name:'Review changes'})).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'Cancel'}));
  expect(calls.respondStagedReview).toHaveBeenLastCalledWith(false);
  await fireEvent.click(screen.getByRole('button',{name:'Prepare installer'}));
  expect(calls.respondStagedReview).toHaveBeenLastCalledWith(true);
});
it('shows copy progress and offers to stop only while copying',async()=>{
  state.set({pending:false,snapshot:{status:'running',stage:'copying-installer',message:'Copying and verifying the installer...',bytes:2*1024**3,totalBytes:6*1024**3,cancelAvailable:true,cancelRequested:false,stagedTesting:true}});
  render(StagedInstaller,{sourceReady:true});
  expect(screen.getByRole('progressbar',{name:'Copying the installer'})).toHaveAttribute('value',String(2*1024**3));
  await fireEvent.click(screen.getByRole('button',{name:'Stop preparing'}));
  expect(calls.cancel).toHaveBeenCalledOnce();
});
it('lets the installer be selected again after a restart and restarts to firmware settings',async()=>{
  const op={operationId:'11111111-1111-4111-8111-111111111111',status:'boot-scheduled',diskNumber:0,temporaryBytes:8*1024**3,message:''};
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{operations:[op]}}});
  render(StagedInstaller,{recoveryOnly:true});
  expect(screen.getByRole('heading',{name:'Temporary installer · Disk 0'})).toBeInTheDocument();
  expect(screen.getByText('Install Omarchy')).toHaveAttribute('aria-current','step');
  expect(screen.getByText('Starting Windows later')).toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'Start installer again on next restart'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('arm',null,op.operationId);
  await fireEvent.click(screen.getByRole('button',{name:'Restart to firmware settings'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('firmware',null,op.operationId);
});
it('emphasizes only the next step for a prepared installer',async()=>{
  const op={operationId:'11111111-1111-4111-8111-111111111111',status:'staged',diskNumber:0,temporaryBytes:8*1024**3,message:''};
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{operations:[op],choices:[],blocked:[],secureBoot:true}}});
  render(StagedInstaller,{recoveryOnly:true});
  expect(screen.getByRole('button',{name:'Restart to firmware settings'})).toHaveClass('primary');
  expect(screen.getByRole('button',{name:'Start installer on next restart'})).not.toHaveClass('primary');
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{operations:[op],choices:[],blocked:[],secureBoot:false}}});
  await tick();
  expect(screen.getByRole('button',{name:'Start installer on next restart'})).toHaveClass('primary');
  expect(screen.queryByRole('button',{name:'Restart to firmware settings'})).not.toBeInTheDocument();
});
it('asks for a restart after selecting the installer, then suggests removal',async()=>{
  const op={operationId:'11111111-1111-4111-8111-111111111111',status:'boot-scheduled',diskNumber:0,temporaryBytes:8*1024**3,message:'',restartedSinceScheduled:false};
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedRecovery:{operations:[op],recordErrors:[]}}});
  render(StagedInstaller,{recoveryOnly:true});
  expect(screen.getByText('Restart Windows to start the installer.')).toBeInTheDocument();
  for(const button of screen.getAllByRole('button')) expect(button).not.toHaveClass('primary');
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedRecovery:{operations:[{...op,restartedSinceScheduled:true}],recordErrors:[]}}});
  await tick();
  expect(screen.getByText('Clean up')).toHaveAttribute('aria-current','step');
  expect(screen.getByText('Does your PC now start Omarchy by itself? Then remove the temporary installer. Resume BitLocker when Windows reminds you.')).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Remove temporary installer'})).toHaveClass('primary');
  expect(screen.getByRole('button',{name:'Start installer again on next restart'})).not.toHaveClass('primary');
});
const card=(status:string,secureBoot:boolean|null,restartedSinceScheduled=false)=>({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[],blocked:[],secureBoot,operations:[{operationId:'11111111-1111-4111-8111-111111111111',status,diskNumber:0,temporaryBytes:8*GiB,message:'',restartedSinceScheduled}]}}});
const installerDisk='In the installer, choose the disk marked “installer disk, free space only”, then “Free space install”.';
it('shows only the current step of a prepared installer, with one primary button',async()=>{
  const cases:[ReturnType<typeof card>,string,string,string|null,boolean][]=[
    [card('staged',true),'Turn off Secure Boot','Restart into firmware settings, turn off Secure Boot, then return to Windows.','Restart to firmware settings',false],
    [card('arming',false),'Set next restart','Make your next restart open the installer.','Start installer on next restart',true],
    [card('boot-scheduled',false),'Install Omarchy','Restart Windows to start the installer.',null,true],
    [card('boot-scheduled',false,true),'Clean up','Does your PC now start Omarchy by itself? Then remove the temporary installer. Resume BitLocker when Windows reminds you.','Remove temporary installer',false]
  ];
  for(const [snapshot,step,text,primary,instruction] of cases) {
    state.set(snapshot);
    const view=render(StagedInstaller,{sourceReady:true});
    await tick();
    expect(screen.getByText(step)).toHaveAttribute('aria-current','step');
    expect(screen.getByText(text)).toBeInTheDocument();
    expect(screen.queryByText(installerDisk)!=null).toBe(instruction);
    const primaries=screen.getAllByRole('button').filter(button=>button.classList.contains('primary'));
    expect(primaries.map(button=>button.textContent)).toEqual(primary ? [primary] : []);
    // Remove and Prepare again are always offered.
    expect(screen.getByRole('button',{name:'Remove temporary installer'})).toBeEnabled();
    expect(screen.getByRole('button',{name:'Prepare again'})).toBeEnabled();
    view.unmount();
  }
});
it('offers a firmware restart beside starting the installer when Secure Boot is unknown',async()=>{
  state.set(card('staged',null));
  render(StagedInstaller,{recoveryOnly:true});
  await tick();
  expect(screen.getByText("Turn off Secure Boot if it's on")).not.toHaveAttribute('aria-current');
  expect(screen.getByText('Set next restart')).toHaveAttribute('aria-current','step');
  expect(screen.getByRole('button',{name:'Start installer on next restart'})).toHaveClass('primary');
  await fireEvent.click(screen.getByRole('button',{name:'Restart to firmware settings'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('firmware',null,'11111111-1111-4111-8111-111111111111');
});
it('replaces the choose screen with the installer steps while one exists',async()=>{
  state.set(card('staged',true));
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByRole('heading',{name:'Temporary installer · Disk 0'})).toBeInTheDocument();
  expect(screen.queryByRole('button',{name:'Check available space'})).not.toBeInTheDocument();
  expect(screen.queryByRole('button',{name:'Review changes'})).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'Prepare again'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('cleanup',null,'11111111-1111-4111-8111-111111111111');
});
it('offers checking disks after a removal failed elsewhere, such as on the home screen',async()=>{
  state.set({pending:false,stagedAction:'cleanup',snapshot:{...card('copying',true).snapshot,status:'failed',error:'The recorded staging disk cannot be identified uniquely.'}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByRole('button',{name:'Check disks again'})).toBeEnabled();
  // After another action fails, retrying it is what helps.
  state.set({pending:false,stagedAction:'arm',snapshot:{...card('staged',false).snapshot,status:'failed',error:'Firmware rejected the boot entry.'}});
  await tick();
  expect(screen.queryByRole('button',{name:'Check disks again'})).not.toBeInTheDocument();
  // A polling error after a removal that succeeded isn't a failed removal.
  state.set({pending:false,stagedAction:'cleanup',error:'Connection unavailable',snapshot:{...card('copying',true).snapshot,status:'complete'}});
  await tick();
  expect(screen.queryByRole('button',{name:'Check disks again'})).not.toBeInTheDocument();
});
it('can check disks beside an installer that cannot be removed',async()=>{
  state.set(card('copying',true));
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.queryByRole('button',{name:'Check disks again'})).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'Remove temporary installer'}));
  // The real store records the action; the mock does not.
  state.set({pending:false,stagedAction:'cleanup',snapshot:{...card('copying',true).snapshot,status:'failed',error:'The recorded staging disk cannot be identified uniquely.'}});
  await tick();
  expect(screen.queryByRole('radiogroup')).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'Check disks again'}));
  expect(calls.stagedIso).toHaveBeenLastCalledWith('inspect');
  const base=measured().snapshot;
  state.set({pending:false,snapshot:{...base,stagedIso:{...base.stagedIso,operations:card('copying',true).snapshot.stagedIso.operations}}});
  await tick();
  expect(screen.getByRole('radiogroup',{name:'Partitions on Disk 0'})).toBeInTheDocument();
  expect(screen.getByRole('heading',{name:"Preparing the installer didn't finish · Disk 0"})).toBeInTheDocument();
  expect(screen.queryByRole('button',{name:'Check disks again'})).not.toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Review changes'})).toBeEnabled();
});
it('says a removal is under way, or that it did not finish',async()=>{
  const removing={...card('cleaning',null).snapshot,status:'running',stage:'staged-installer',message:'Loading Windows disk tools...'};
  const stagedTarget={action:'cleanup',operationId:'11111111-1111-4111-8111-111111111111'};
  // From the click on, before the record says `cleaning`.
  state.set({pending:true,stagedTarget,snapshot:card('boot-scheduled',null,true).snapshot});
  render(StagedInstaller,{recoveryOnly:true,sourceReady:true});
  await tick();
  expect(screen.getByRole('heading',{name:'Removing the temporary installer · Disk 0'})).toBeInTheDocument();
  expect(screen.queryByText('Starting Windows later')).not.toBeInTheDocument();
  state.set({pending:false,stagedAction:'cleanup',stagedTarget,snapshot:removing});
  await tick();
  expect(screen.getByRole('heading',{name:'Removing the temporary installer · Disk 0'})).toBeInTheDocument();
  expect(screen.getByText('Keep this window open.')).toBeInTheDocument();
  expect(screen.queryByRole('button',{name:'Remove temporary installer'})).not.toBeInTheDocument();
  state.set({pending:false,stagedAction:'cleanup',stagedTarget,snapshot:{...removing,status:'failed',error:'Windows could not delete the partition.'}});
  await tick();
  expect(screen.getByRole('heading',{name:"Removing the temporary installer didn't finish · Disk 0"})).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Remove temporary installer'})).toBeEnabled();
});
it('keeps other recorded installers as they are while one is prepared or removed',async()=>{
  const other={operationId:'22222222-2222-4222-8222-222222222222',status:'copying',diskNumber:1,temporaryBytes:8*GiB,message:''};
  const mine={operationId:'11111111-1111-4111-8111-111111111111',status:'copying',diskNumber:0,temporaryBytes:8*GiB,message:''};
  // Preparing another installer beside one that didn't finish.
  state.set(reviewing({...reviewed(),approved:true},{stage:'copying-installer',message:'Copying and verifying the installer...',stagedRecovery:{operations:[other,mine],recordErrors:[]}}));
  const view=render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByRole('region',{name:'Preparing the installer'})).toBeInTheDocument();
  expect(screen.getAllByRole('article')).toHaveLength(1);
  expect(screen.getByRole('heading',{name:"Preparing the installer didn't finish · Disk 1"})).toBeInTheDocument();
  view.unmount();
  // Removing one installer.
  state.set({pending:false,stagedTarget:{action:'cleanup',operationId:mine.operationId},snapshot:{status:'running',stagedTesting:true,stagedRecovery:{operations:[other,{...mine,status:'cleaning'}],recordErrors:[]}}});
  const home=render(StagedInstaller,{recoveryOnly:true,sourceReady:true});
  await tick();
  expect(screen.getByRole('heading',{name:'Removing the temporary installer · Disk 0'})).toBeInTheDocument();
  expect(screen.getByRole('heading',{name:"Preparing the installer didn't finish · Disk 1"})).toBeInTheDocument();
  // Another command, such as the USB route's disk check, changes neither.
  state.set({pending:false,stagedTarget:null,snapshot:{status:'inspecting',kind:'usb',stagedTesting:true,stagedRecovery:{operations:[other],recordErrors:[]}}});
  await tick();
  expect(screen.getByRole('heading',{name:"Preparing the installer didn't finish · Disk 1"})).toBeInTheDocument();
  home.unmount();
});
it('confirms a removal on the home screen, but leaves next steps to the card',async()=>{
  const removed='Temporary installer partitions and its firmware entry were removed.';
  state.set({pending:false,stagedAction:'cleanup',snapshot:{status:'complete',stagedTesting:true,stagedRecovery:{operations:[],recordErrors:[]},stagedIso:{operationId:'11111111-1111-4111-8111-111111111111',status:'cleaned',message:removed}}});
  render(StagedInstaller,{recoveryOnly:true,sourceReady:true});
  await tick();
  expect(screen.getByRole('region',{name:'Temporary installer'})).toHaveTextContent(removed);
  // After preparing (the last command answered Review) or setting the next
  // restart, the provider's own advice would contradict the card.
  for(const stagedAction of [null,'arm']) {
    const prepared=card('staged',true).snapshot;
    state.set({pending:false,stagedAction,snapshot:{...prepared,stagedIso:{...prepared.stagedIso,operationId:'11111111-1111-4111-8111-111111111111',status:'staged',message:'Restart into the installer.'}}});
    await tick();
    expect(screen.getByRole('heading',{name:'Temporary installer · Disk 0'})).toBeInTheDocument();
    expect(screen.queryByText('Restart into the installer.')).not.toBeInTheDocument();
  }
});
it('offers help and the USB route when no space can be used',async()=>{
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[],blocked:[],disks:[{...fullDisk,regions:[{...checkedC,maximumReleaseBytes:0}]}],minimumLinuxBytes:32*GiB,temporaryBytes:8*GiB}}});
  const useUsb=vi.fn();
  render(StagedInstaller,{sourceReady:true,useUsb});
  await tick();
  expect(screen.getByText('No space can be used yet.')).toBeInTheDocument();
  expect(screen.getByText('What can help?')).toBeInTheDocument();
  // Files Windows can't move block C:, so the tip is about those.
  expect(screen.getByText('Turning off hibernation and System Restore, then restarting, often lets Windows shrink C: further.')).toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'create a bootable USB'}));
  expect(useUsb).toHaveBeenCalledOnce();
});
it('keeps a usable default when a later measurement finds more room, and says what Review waits for',async()=>{
  state.set(twoPartitions(dataRegion,'running'));
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByRole('radio',{name:'C: can give Omarchy up to 100 GiB'})).toBeChecked();
  expect(screen.getByText('Still checking D:…')).toBeInTheDocument();
  state.set(twoPartitions(dataMeasured,'complete'));
  await tick();
  // D: could give more, but the selection doesn't jump from C: by itself.
  expect(screen.getByRole('radio',{name:'C: can give Omarchy up to 100 GiB'})).toBeChecked();
  expect(screen.queryByText(/Still checking/)).not.toBeInTheDocument();
  expect(screen.getByText('Select where the space comes from.')).toBeInTheDocument();
});
it('explains a failed measurement plainly and keeps the reason in the table',async()=>{
  const failed={...fullDisk.regions[0],resizeState:'failed',resizeReason:'Windows returned unsupported NTFS shrink limits.',reserveBytes:0};
  state.set({pending:false,snapshot:{status:'complete',stagedTesting:true,stagedIso:{choices:[],blocked:[],disks:[{...fullDisk,regions:[failed]}],minimumLinuxBytes:32*GiB,temporaryBytes:8*GiB}}});
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByText("Windows couldn't measure C:.",{selector:'p'})).toBeInTheDocument();
  expect(screen.getByText('Restart Windows, then recheck.')).toBeInTheDocument();
  expect(screen.queryByText(/hibernation/)).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button',{name:'All partitions (1)'}));
  expect(screen.getByRole('table')).toHaveTextContent("Windows couldn't measure C:. Windows returned unsupported NTFS shrink limits.");
});
it('names the new parts of the After bar and keeps the warning out of Full details',async()=>{
  state.set(reviewing(reviewed()));
  render(StagedInstaller,{sourceReady:true});
  await tick();
  const review=screen.getByRole('region',{name:'Review changes'});
  expect(review.querySelector('.legend')).toHaveTextContent('Temporary installer');
  expect(review.querySelector('.legend')).toHaveTextContent('Omarchy');
  expect(review.querySelector('details')).not.toHaveTextContent('EXPERIMENTAL');
});
it('helps with Secure Boot and leaves starting the installer for later',async()=>{
  state.set(card('staged',true));
  render(StagedInstaller,{sourceReady:true});
  await tick();
  expect(screen.getByText('How?')).toBeInTheDocument();
  const row=screen.getByRole('button',{name:'Remove temporary installer'}).parentElement!;
  expect([...row.querySelectorAll('button')].map(button=>button.textContent)).toEqual(['Remove temporary installer','Prepare again','Start installer on next restart']);
});
