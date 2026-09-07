import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SetupPanel from './SetupPanel.svelte';
import { setup } from './setup';
import { downloads } from './downloads';

const calls = vi.hoisted(() => ({ inspectUsb: vi.fn(), inspect: vi.fn(), start: vi.fn(), reviewUsb:vi.fn(), dismissUsbReview:vi.fn() }));
vi.mock('./downloads', async importOriginal => {
  const actual = await importOriginal<typeof import('./downloads')>();
  const { writable } = await import('svelte/store');
  return { ...actual, downloads: { ...writable({ snapshot: { status: 'complete', image_path: 'C:\\fixture.iso' } }), native: true } };
});
vi.mock('./setup', async importOriginal => {
  const actual = await importOriginal<typeof import('./setup')>();
  const { writable } = await import('svelte/store');
  return { ...actual, setup: { ...writable({pending:false,error:null,snapshot:null}), ...calls } };
});
const setSetup=(value:unknown)=>(setup as unknown as {set(v:unknown):void}).set(value);
const setDownloads=(value:unknown)=>(downloads as unknown as {set(v:unknown):void}).set(value);
function state(overrides={}) {
  return { pending:false,error:null,snapshot:{kind:'usb',status:'ready',requirements:[],choices:[{
    id:'chosen-usb',label:'USB fixture',detail:'PhysicalDrive9',sizeBytes:16e9,eligible:true,reasons:[],allocation:null,deletion:null,
    usb:{eligible:true,reasons:[],freeBytes:8e9,requiredBytes:6.3e9,replacesBootloader:true,needsAdministrator:false,...overrides}
  }]}};
}
function showReview(mode:'erase'|'preserve') {
  const value=state();
  setSetup({...value,snapshot:{...value.snapshot,usbReview:{token:'review-token',choiceId:'chosen-usb',mode,label:'USB fixture',detail:'PhysicalDrive9',sizeBytes:16e9,fileName:'omarchy-4.0.2.iso',replacesBootloader:true}}});
}
describe('USB review before administrator access',()=>{
  beforeEach(()=>{
    vi.clearAllMocks();setSetup(state());setDownloads({snapshot:{status:'complete',image_path:'C:\\fixture.iso'}});
    calls.reviewUsb.mockImplementation((_id,mode)=>showReview(mode));
    calls.dismissUsbReview.mockImplementation(()=>setSetup(state()));
  });
  it('reviews erase in the app and starts only after confirming the bound review',async()=>{
    render(SetupPanel,{kind:'usb',close:vi.fn()});
    await fireEvent.click(screen.getByRole('radio'));
    expect(calls.inspectUsb).toHaveBeenCalledExactlyOnceWith('chosen-usb');
    await fireEvent.click(screen.getByRole('button',{name:'Erase USB and create installer'}));
    expect(calls.reviewUsb).toHaveBeenCalledExactlyOnceWith('chosen-usb','erase');
    expect(calls.start).not.toHaveBeenCalled();
    expect(screen.getByRole('heading',{name:'Erase this USB?'})).toHaveFocus();
    expect(screen.getByText('All files and partitions on this USB will be deleted.')).toBeInTheDocument();
    expect(screen.getByText('Installer: omarchy-4.0.2.iso')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button',{name:'Erase USB and create installer'}));
    expect(calls.start).toHaveBeenCalledExactlyOnceWith('chosen-usb',undefined,undefined,undefined,'erase','review-token');
  });
  it('cancels the review without starting an elevated operation',async()=>{
    render(SetupPanel,{kind:'usb',close:vi.fn()});await fireEvent.click(screen.getByRole('radio'));
    await fireEvent.click(screen.getByRole('button',{name:'Erase USB and create installer'}));
    await fireEvent.click(screen.getByRole('button',{name:'Cancel'}));
    expect(calls.dismissUsbReview).toHaveBeenCalledOnce();
    expect(calls.start).not.toHaveBeenCalled();
    expect(screen.queryByRole('heading',{name:'Erase this USB?'})).not.toBeInTheDocument();
  });
  it('reviews preserving separately and explains bootloader replacement before elevation',async()=>{
    render(SetupPanel,{kind:'usb',close:vi.fn()});await fireEvent.click(screen.getByRole('radio'));
    await fireEvent.click(screen.getByRole('button',{name:'Keep files and add installer'}));
    expect(calls.start).not.toHaveBeenCalled();
    expect(screen.getByText('Your files and partitions will be kept.')).toBeInTheDocument();
    expect(screen.getByText(/current bootloader will be backed up and replaced/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button',{name:'Keep files and add installer'}));
    expect(calls.start).toHaveBeenCalledExactlyOnceWith('chosen-usb',undefined,undefined,undefined,'preserve','review-token');
  });
  it('shows a short blocking message with technical explanations collapsed',async()=>{
    setSetup(state({eligible:false,freeBytes:1e9,reasons:['An existing NTFS data partition is required.','There is not enough free space for the ISO and a 64 MiB safety reserve.']}));
    render(SetupPanel,{kind:'usb',close:vi.fn()}); await fireEvent.click(screen.getByRole('radio'));
    expect(screen.getByRole('button',{name:'Keep files and add installer'})).toBeDisabled();
    expect(screen.getByText('Can’t keep files on this USB.')).toHaveClass('error');
    expect(screen.getByRole('status')).toHaveTextContent('Unsupported drive layout and not enough free space.');
    expect(screen.getByText('An existing NTFS data partition is required.').closest('details')).not.toHaveAttribute('open');
    expect(screen.queryByText(/Retain existing files and partitions/)).not.toBeInTheDocument();
    expect(screen.getByRole('button',{name:'Erase USB and create installer'})).toBeEnabled();
  });
  it('requires a verified image throughout the review',async()=>{
    showReview('erase');setDownloads({snapshot:{status:'idle'}});
    render(SetupPanel,{kind:'usb',close:vi.fn()});
    expect(screen.queryByRole('button',{name:'Erase USB and create installer'})).not.toBeInTheDocument();
    expect(calls.start).not.toHaveBeenCalled();
  });
  it('does not expose the deferred restore feature',async()=>{
    setSetup(state({eligible:false,restoreAvailable:true}));
    render(SetupPanel,{kind:'usb',close:vi.fn()}); await fireEvent.click(screen.getByRole('radio'));
    expect(screen.queryByRole('button',{name:'Restore previous boot setup'})).not.toBeInTheDocument();
  });
});
