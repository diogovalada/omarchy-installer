import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeEach, expect, it, vi } from 'vitest';
import App from './App.svelte';
import { setup } from './lib/setup';

vi.mock('./lib/downloads', async importOriginal => {
  const actual = await importOriginal<typeof import('./lib/downloads')>();
  const { writable } = await import('svelte/store');
  return { ...actual, downloads: { ...actual.downloads, ...writable({ pending:false, error:null,
    snapshot:{ status:'idle', host_os:'windows', host_architecture:'x86_64', release:null, destination_directory:'C:/Downloads', image_path:null }
  }), native:true, refresh:vi.fn().mockResolvedValue(undefined), resolve:vi.fn().mockResolvedValue(undefined) } };
});
vi.mock('./lib/setup', async importOriginal => {
  const actual = await importOriginal<typeof import('./lib/setup')>();
  const { writable } = await import('svelte/store');
  return { ...actual, setup:{ ...writable({ pending:false, snapshot:null }), refresh:vi.fn(), stagedIso:vi.fn() } };
});
const state = setup as typeof setup & { set:(value:unknown)=>void };
beforeEach(()=>{ vi.clearAllMocks(); state.set({pending:false,snapshot:{status:'idle',stagedTesting:false}}); });
it('keeps normal native builds closed with no empty recovery section',()=>{
  render(App);
  expect(screen.getByRole('button',{name:'Install without USB'})).toBeDisabled();
  expect(screen.queryByRole('region',{name:'Temporary installer'})).not.toBeInTheDocument();
  expect(setup.stagedIso).not.toHaveBeenCalled();
});
it('opens the real testing flow before downloading without prompting for elevation',async()=>{
  state.set({pending:false,snapshot:{status:'idle',stagedTesting:true}});
  render(App);
  await fireEvent.click(screen.getByRole('button',{name:'Install without USB'}));
  expect(screen.getByRole('heading',{name:'Install without USB'})).toBeInTheDocument();
  expect(screen.getByText('Test build · real disk changes')).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Check available space'})).toBeDisabled();
  expect(setup.stagedIso).not.toHaveBeenCalled();
});
it('surfaces an interrupted installer without invoking the privileged checker',()=>{
  state.set({pending:false,snapshot:{status:'idle',stagedTesting:false,stagedRecovery:{operations:[{operationId:'id',status:'copying',diskNumber:0,temporaryBytes:8*1024**3}],recordErrors:[]}}});
  render(App);
  expect(screen.getByRole('heading',{name:'Installer preparation needs attention'})).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'Remove temporary installer'})).toBeEnabled();
  expect(setup.stagedIso).not.toHaveBeenCalled();
});
