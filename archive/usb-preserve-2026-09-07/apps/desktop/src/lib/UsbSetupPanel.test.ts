import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SetupPanel from './SetupPanel.svelte';
import { setup } from './setup';
import { downloads } from './downloads';

const calls = vi.hoisted(() => ({ inspectUsb: vi.fn(), inspect: vi.fn(), start: vi.fn() }));
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
    usb:{eligible:true,reasons:[],freeBytes:8e9,requiredBytes:6.3e9,replacesBootloader:true,restoreAvailable:false,needsAdministrator:false,...overrides}
  }]}};
}
describe('USB preserving and erase choices',()=>{
  beforeEach(()=>{vi.clearAllMocks();setSetup(state());setDownloads({snapshot:{status:'complete',image_path:'C:\\fixture.iso'}});});
  it('inspects the selected USB and sends distinct explicit operation modes',async()=>{
    render(SetupPanel,{kind:'usb',close:vi.fn()});
    await fireEvent.click(screen.getByRole('radio'));
    expect(calls.inspectUsb).toHaveBeenCalledExactlyOnceWith('chosen-usb');
    expect(screen.getByText(/saved on this USB before replacement/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button',{name:'Keep files and add installer'}));
    expect(calls.start).toHaveBeenLastCalledWith('chosen-usb',undefined,undefined,undefined,'preserve');
    await fireEvent.click(screen.getByRole('button',{name:'Erase USB and create installer'}));
    expect(calls.start).toHaveBeenLastCalledWith('chosen-usb',undefined,undefined,undefined,'erase');
    expect(screen.getByText(/Deletes every file and partition/)).toBeInTheDocument();
  });
  it('disables keeping files with the precise reason while retaining the erase choice',async()=>{
    setSetup(state({eligible:false,freeBytes:1e9,reasons:['There is not enough free space.']}));
    render(SetupPanel,{kind:'usb',close:vi.fn()}); await fireEvent.click(screen.getByRole('radio'));
    expect(screen.getByRole('button',{name:'Keep files and add installer'})).toBeDisabled();
    expect(screen.getByText('There is not enough free space.')).toBeInTheDocument();
    expect(screen.getByRole('button',{name:'Erase USB and create installer'})).toBeEnabled();
  });
  it('allows restoring a USB boot backup without a downloaded ISO',async()=>{
    setDownloads({snapshot:{status:'idle'}}); setSetup(state({eligible:false,restoreAvailable:true}));
    render(SetupPanel,{kind:'usb',close:vi.fn()}); await fireEvent.click(screen.getByRole('radio'));
    expect(screen.getByRole('button',{name:'Keep files and add installer'})).toBeDisabled();
    expect(screen.getByRole('button',{name:'Erase USB and create installer'})).toBeDisabled();
    await fireEvent.click(screen.getByRole('button',{name:'Restore previous boot setup'}));
    expect(calls.start).toHaveBeenLastCalledWith('chosen-usb',undefined,undefined,undefined,'restore');
  });
});
