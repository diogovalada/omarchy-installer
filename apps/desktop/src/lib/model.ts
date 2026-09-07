export type HostId = 'windows' | 'intel-mac' | 'apple-silicon' | 'linux';
export type FlowId = 'overview' | 'try' | 'install' | 'usb' | 'download' | 'manage';
export type Availability = 'ready' | 'preview' | 'unavailable';

export interface HostProfile {
  id: HostId;
  label: string;
  shortLabel: string;
  detail: string;
  architecture: string;
  tryStatus: Availability;
  installStatus: Availability;
  usbStatus: Availability;
}

export const hosts: HostProfile[] = [
  {
    id: 'windows', label: 'Windows PC', shortLabel: 'Windows', detail: 'Windows 11 · UEFI', architecture: 'x86-64',
    tryStatus: 'ready', installStatus: 'preview', usbStatus: 'ready'
  },
  {
    id: 'intel-mac', label: 'Intel Mac', shortLabel: 'Intel Mac', detail: 'macOS 15 · Intel', architecture: 'x86-64',
    tryStatus: 'preview', installStatus: 'preview', usbStatus: 'ready'
  },
  {
    id: 'apple-silicon', label: 'Apple silicon Mac', shortLabel: 'Apple Silicon', detail: 'macOS 15 · M1–M5', architecture: 'arm64',
    tryStatus: 'ready', installStatus: 'ready', usbStatus: 'ready'
  },
  {
    id: 'linux', label: 'Linux PC', shortLabel: 'Linux', detail: 'Linux · UEFI', architecture: 'x86-64',
    tryStatus: 'preview', installStatus: 'unavailable', usbStatus: 'preview'
  }
];

export const getHost = (id: HostId): HostProfile => hosts.find((host) => host.id === id) ?? hosts[0];

export const statusCopy = (status: Availability) => ({
  ready: 'Available', preview: 'Preview', unavailable: 'Not available'
})[status];

export function canContinue(status: Availability): boolean {
  return status !== 'unavailable';
}
