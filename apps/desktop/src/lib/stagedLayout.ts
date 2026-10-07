// Pure layout model and copy for the "Install without USB" disk map. The
// provider decides; this only previews its layout (see Get-StagingPlan).
import type { StagedChoice, StagedDisk, StagedRegion, StagedReviewPlan } from './setup';

export const MiB=1024**2, GiB=1024**3;
const HIDDEN_GAP_BYTES=5*MiB;
// The reason Get-StagingChoices (providers/staged-iso/Staging.ps1) gives
// partitions it never resizes; other reasons explain a problem.
const KEPT_REASON='System or unsupported partition; kept unchanged.';

export type Tone='normal'|'dimmed'|'hatched'|'checking'|'outlined'|'outlined-dimmed';
export type RegionAction='refresh'|'recheck'|null;
export interface RegionStatus { tone:Tone;text:string;action:RegionAction }
export interface StatusContext { temporaryBytes:number;minimumBytes:number;usable:boolean;checking?:boolean }
export interface OverviewSegment { key:string;regions:StagedRegion[];offsetBytes:number;sizeBytes:number;widthPercent:number;label:string }
export type EditorMode='keep'|'replace';
export interface EditorSegment { kind:'used'|'cant-release'|'free'|'temporary'|'omarchy';offsetBytes:number|null;sizeBytes:number;widthPercent:number;label:string }
export interface ProjectedRegion { kind:'free'|'partition'|'temporary'|'omarchy';offsetBytes:number;sizeBytes:number;label:string;partitionGuid?:string }

const alignDown=(bytes:number) => Math.floor(bytes/MiB)*MiB;

export type Rounding='nearest'|'down'|'up';
const round=(value:number,rounding:Rounding) => rounding==='down' ? Math.floor(value+1e-9) : rounding==='up' ? Math.ceil(value-1e-9) : Math.round(value);

// "260 MiB", "7.5 GiB", "953 GiB": MiB below 1 GiB, one decimal without ".0".
// Round down for what is available and up for what is needed, so a
// shortfall never reads as "only 7.5 GiB, 41 MiB more needed" or "0 MiB".
export function formatSize(bytes:number,rounding:Rounding='nearest') {
  const size=Math.max(0,bytes), mebibytes=round(size/MiB,rounding);
  if(mebibytes<1024) return `${mebibytes} MiB`;
  return `${(round(size/GiB*10,rounding)/10).toFixed(1).replace(/\.0$/,'')} GiB`;
}

// Disk headings: whole GiB, or TiB with one decimal from 1 TiB.
export function formatDiskSize(bytes:number) {
  if(bytes<1024*GiB) return `${Math.round(bytes/GiB)} GiB`;
  return `${(bytes/(1024*GiB)).toFixed(1).replace(/\.0$/,'')} TiB`;
}

export function regionKey(region:StagedRegion) {
  if(region.kind==='free') return `free:${region.offsetBytes}`;
  return region.partitionGuid || `partition:${region.offsetBytes}`;
}

// Choices must already be limited to the region's disk: free offsets repeat across disks.
export function choiceFor(region:StagedRegion,choices:StagedChoice[]) {
  return choices.find(c=>region.kind==='free'
    ? c.target.target_kind==='free' && c.target.start_offset_bytes===region.offsetBytes
    : c.target.target_kind==='shrink' && !!region.partitionGuid && c.target.partition_guid===region.partitionGuid);
}

const keptAsIs=(region:StagedRegion) => region.kind==='partition' && region.resizeState==='unavailable' && (!region.resizeReason || region.resizeReason===KEPT_REASON);

// Scaled segments for the overview bar. Gaps under 5 MiB are not drawn, and
// neighboring kept partitions under 1% of the disk become one group. Widths
// are percentages of the drawn total, raised to minPx of a barPx-wide bar.
export function overviewSegments(disk:StagedDisk,{barPx=760,minPx=10}:{barPx?:number;minPx?:number}={}):OverviewSegment[] {
  const small=(r:StagedRegion) => keptAsIs(r) && r.sizeBytes<disk.diskSizeBytes/100;
  const groups:StagedRegion[][]=[];
  for(const region of disk.regions) {
    if(region.kind==='free' && region.sizeBytes<HIDDEN_GAP_BYTES) continue;
    const last=groups.at(-1);
    if(last && small(region) && last.every(small)) last.push(region);
    else groups.push([region]);
  }
  const segments=groups.map(regions=>({
    key:regions.length>1 ? `group:${regions[0].offsetBytes}` : regionKey(regions[0]),
    regions,offsetBytes:regions[0].offsetBytes,
    sizeBytes:regions.reduce((total,r)=>total+r.sizeBytes,0),
    widthPercent:0,
    label:regions.length>1 ? `${regions.length} small partitions` : regions[0].kind==='free' ? 'Free space' : regions[0].label
  }));
  return scaleWidths(segments,Math.min(100,minPx/barPx*100));
}

// The overview's width rule for any bar, such as Review's Now and After.
export function barWidths(sizes:number[],{barPx=760,minPx=10}:{barPx?:number;minPx?:number}={}) {
  return scaleWidths(sizes.map(sizeBytes=>({sizeBytes,widthPercent:0})),Math.min(100,minPx/barPx*100)).map(item=>item.widthPercent);
}

// Proportional widths summing to 100, with every segment at least `minimum`.
function scaleWidths<T extends {sizeBytes:number;widthPercent:number}>(items:T[],minimum:number,total=100):T[] {
  if(items.length*minimum>=total) {
    for(const item of items) item.widthPercent=total/items.length;
    return items;
  }
  const fixed=new Set<T>();
  for(;;) {
    const flexible=items.filter(item=>!fixed.has(item));
    const space=total-fixed.size*minimum;
    const bytes=flexible.reduce((sum,item)=>sum+item.sizeBytes,0);
    for(const item of flexible) item.widthPercent=bytes>0 ? item.sizeBytes/bytes*space : space/flexible.length;
    const raised=flexible.filter(item=>item.widthPercent<minimum);
    if(!raised.length) {
      for(const item of fixed) item.widthPercent=minimum;
      return items;
    }
    for(const item of raised) fixed.add(item);
  }
}

// GiB limits for Omarchy's space when shrinking. Keeping Windows needs room
// for the temporary installer plus the minimum Omarchy size.
export function limits(region:StagedRegion|undefined,temporaryBytes:number,minimumBytes:number) {
  const minGiB=Math.ceil(minimumBytes/GiB);
  const release=region?.kind==='partition' && region.resizeState==='checked' ? region.maximumReleaseBytes ?? 0 : 0;
  const maxGiB=Math.max(0,Math.floor((release-temporaryBytes)/GiB));
  return {minGiB,maxGiB,canKeep:maxGiB>=minGiB};
}

// The selected region as it will be laid out, in provider order: the shrunk
// partition (used space, space Windows can't release, space it keeps free),
// the temporary installer, then Omarchy. A free region starts with the
// installer and leaves the rest to Omarchy, so `mode` applies only to
// shrinking. Used space is a fixed stub.
export function editorSegments(region:StagedRegion,{temporaryBytes,omarchyBytes=0,mode}:{temporaryBytes:number;omarchyBytes?:number;mode:EditorMode}):EditorSegment[] {
  const segment=(kind:EditorSegment['kind'],offsetBytes:number|null,sizeBytes:number,label:string):EditorSegment => ({kind,offsetBytes,sizeBytes:Math.max(0,sizeBytes),widthPercent:0,label});
  if(region.kind==='free') {
    const segments=[segment('temporary',region.offsetBytes,temporaryBytes,`installer ${formatSize(temporaryBytes)} · temporary`)];
    const rest=region.sizeBytes-temporaryBytes;
    if(rest>0) segments.push(segment('omarchy',region.offsetBytes+temporaryBytes,rest,`Omarchy ${formatSize(rest)}`));
    return scaleWidths(segments,0);
  }
  const linux=mode==='keep' ? omarchyBytes : 0;
  const start=alignDown(region.offsetBytes+region.sizeBytes)-temporaryBytes-linux;
  const free=region.freeBytes ?? 0;
  const used=region.sizeBytes-free;
  const releasable=Math.min(free,region.maximumReleaseBytes ?? free);
  const segments=[segment('used',region.offsetBytes,used,`${formatSize(used)} used`)];
  if(free-releasable>0) segments.push(segment('cant-release',null,free-releasable,`can't release ${formatSize(free-releasable)}`));
  if(releasable-temporaryBytes-linux>0) segments.push(segment('free',null,releasable-temporaryBytes-linux,`free ${formatSize(releasable-temporaryBytes-linux)}`));
  segments.push(segment('temporary',start,temporaryBytes,`installer ${formatSize(temporaryBytes)} · temporary`));
  if(linux>0) segments.push(segment('omarchy',start+temporaryBytes,linux,`Omarchy ${formatSize(linux)}`));
  const [stub,...rest]=segments;
  stub.widthPercent=15;
  scaleWidths(rest,0,85);
  return segments;
}

// The selected can't-release zone: the reserve Windows keeps free, plus files
// it can't move when that leaves 1 GiB or more.
export function cantReleaseLine(region:StagedRegion) {
  if(region.reserveBytes==null) return `Windows can't release this space from ${region.label}.`;
  const free=region.freeBytes ?? 0, zone=free-Math.min(free,region.maximumReleaseBytes ?? 0);
  const keeps=`Windows keeps ${formatSize(region.reserveBytes)} free`;
  return zone-region.reserveBytes<GiB ? `${keeps}.` : `${keeps} and can't move some files near the end of ${region.label}.`;
}

// Space left for Omarchy after the installer in a free region; Review says
// it without "the remaining".
export function freeTargetLine(bytes:number,minimumBytes:number,{review=false}:{review?:boolean}={}) {
  if(bytes<minimumBytes) return `Only ${formatSize(bytes,'down')} remains; Omarchy needs ${formatSize(minimumBytes,'up')}. Delete a partition in the installer to make room.`;
  return `Omarchy can use ${review ? '' : 'the remaining '}${formatSize(bytes,'down')}.`;
}

// Windows' size after the shrink, as Get-StagingPlan computes it.
export function windowsAfterBytes(region:StagedRegion,temporaryBytes:number,omarchyBytes=0) {
  return alignDown(region.offsetBytes+region.sizeBytes)-temporaryBytes-omarchyBytes-region.offsetBytes;
}

// The reviewed plan drawn over the inspected disk, or null when the disk
// changed since the user saw it. Free space under 5 MiB is left out, as in
// the overview (a shrink always leaves an alignment sliver under 1 MiB).
export function afterLayout(disk:StagedDisk,review:StagedReviewPlan):ProjectedRegion[]|null {
  if(disk.diskNumber!==review.diskNumber || disk.diskSizeBytes!==review.diskSizeBytes) return null;
  const project=(r:StagedRegion):ProjectedRegion => ({kind:r.kind,offsetBytes:r.offsetBytes,sizeBytes:r.sizeBytes,label:r.kind==='free' ? 'Free space' : r.label,...(r.partitionGuid ? {partitionGuid:r.partitionGuid} : {})});
  const temporary=review.temporary;
  const omarchy=review.omarchy && review.omarchy.sizeBytes>0 ? review.omarchy : null;
  const added:ProjectedRegion[]=[{kind:'temporary',offsetBytes:temporary.offsetBytes,sizeBytes:temporary.sizeBytes,label:'Temporary installer'}];
  if(omarchy) added.push({kind:'omarchy',offsetBytes:omarchy.offsetBytes,sizeBytes:omarchy.sizeBytes,label:'Omarchy'});
  const addedEnd=(omarchy ?? temporary).offsetBytes+(omarchy ?? temporary).sizeBytes;
  if(review.targetKind==='shrink') {
    const shrink=review.shrink;
    if(!shrink) return null;
    const index=disk.regions.findIndex(r=>r.kind==='partition' && r.partitionGuid===shrink.partitionGuid && r.offsetBytes===shrink.offsetBytes && r.sizeBytes===shrink.beforeSizeBytes);
    if(index<0) return null;
    const region=disk.regions[index];
    const end=region.offsetBytes+region.sizeBytes;
    const tail:ProjectedRegion[]=end>addedEnd ? [{kind:'free',offsetBytes:addedEnd,sizeBytes:end-addedEnd,label:'Free space'}] : [];
    return drawn([...disk.regions.slice(0,index).map(project),{...project(region),sizeBytes:shrink.afterSizeBytes},...added,...tail,...disk.regions.slice(index+1).map(project)]);
  }
  const index=disk.regions.findIndex(r=>r.kind==='free' && temporary.offsetBytes>=r.offsetBytes && temporary.offsetBytes<r.offsetBytes+r.sizeBytes);
  if(index<0) return null;
  const region=disk.regions[index];
  const end=region.offsetBytes+region.sizeBytes;
  const before:ProjectedRegion[]=temporary.offsetBytes>region.offsetBytes ? [{kind:'free',offsetBytes:region.offsetBytes,sizeBytes:temporary.offsetBytes-region.offsetBytes,label:'Free space'}] : [];
  const after:ProjectedRegion[]=end>addedEnd ? [{kind:'free',offsetBytes:addedEnd,sizeBytes:end-addedEnd,label:'Free space'}] : [];
  return drawn([...disk.regions.slice(0,index).map(project),...before,...added,...after,...disk.regions.slice(index+1).map(project)]);
}
const drawn=(regions:ProjectedRegion[]) => regions.filter(r=>r.kind!=='free' || r.sizeBytes>=HIDDEN_GAP_BYTES);

// Keep the user's selection elsewhere; this picks the first one: free space
// that fits Omarchy, then the shrink that can keep Windows with the most room,
// then any usable target, then the largest NTFS partition to explain why not.
export function defaultRegion(disk:StagedDisk,choices:StagedChoice[],temporaryBytes:number,minimumBytes:number) {
  const own=choices.filter(c=>c.diskUniqueId===disk.diskUniqueId);
  const usable=disk.regions.filter(r=>choiceFor(r,own));
  const largest=(regions:StagedRegion[],size:(r:StagedRegion)=>number) => regions.reduce<StagedRegion|undefined>((best,r)=>!best || size(r)>size(best) ? r : best,undefined);
  return largest(usable.filter(r=>r.kind==='free' && r.sizeBytes-temporaryBytes>=minimumBytes),r=>r.sizeBytes)
    ?? largest(usable.filter(r=>r.kind==='partition' && limits(r,temporaryBytes,minimumBytes).canKeep),r=>r.maximumReleaseBytes ?? 0)
    ?? usable[0]
    ?? largest(disk.regions.filter(r=>r.kind==='partition' && r.fileSystem==='NTFS'),r=>r.sizeBytes);
}

// One line about a region or a group of small partitions, with its visual
// tone and the one action it offers. All region-state copy lives here.
export function regionStatus(target:StagedRegion|StagedRegion[],context:StatusContext):RegionStatus {
  const status=(tone:Tone,text:string,action:RegionAction=null):RegionStatus => ({tone,text,action});
  if(Array.isArray(target)) {
    if(!target.length) return status('dimmed','');
    if(target.length>1) return status('dimmed',`${target.map(r=>`${r.label} ${formatSize(r.sizeBytes)}`).join(' · ')} · kept as is`);
    target=target[0];
  }
  const region=target, name=region.label, size=formatSize(region.sizeBytes);
  if(region.kind==='free') return context.usable ? status('outlined',`Free space · ${size}`) : status('outlined-dimmed',`Free space · ${size} · too small`);
  switch(region.resizeState) {
    case 'insufficient': {
      const missing=region.reserveBytes!=null && region.freeBytes!=null ? context.temporaryBytes+region.reserveBytes-region.freeBytes : 0;
      return status('hatched',missing>0 ? `${name} needs ${formatSize(missing,'up')} more free space. Free up space in Windows, then refresh.` : `${name} doesn't have enough free space. Free up space in Windows, then refresh.`,'refresh');
    }
    case 'unchecked':
      return context.checking ? status('checking',`Checking how much ${name} can shrink. This can take a few minutes.`) : status('dimmed',`${name} wasn't checked. Refresh to try again.`,'refresh');
    case 'checked': {
      const release=region.maximumReleaseBytes ?? 0;
      if(context.usable) {
        const {maxGiB,canKeep}=limits(region,context.temporaryBytes,context.minimumBytes);
        return status('normal',`${[name,region.fileSystem].filter(Boolean).join(' · ')} · ${canKeep ? `can give Omarchy up to ${maxGiB} GiB` : 'room for the installer only'}`);
      }
      if(release<context.temporaryBytes) {
        // Freeing space helps only when the reserve is the limit. Otherwise
        // free space minus the reserve fits the installer, and the limit is
        // files Windows can't move near the end of the partition.
        const room=region.freeBytes!=null && region.reserveBytes!=null ? region.freeBytes-region.reserveBytes : null;
        if(room!=null && room<context.temporaryBytes) return status('hatched',`${name} needs ${formatSize(context.temporaryBytes-room,'up')} more free space. Free up space in Windows, then recheck.`,'recheck');
        if(room!=null) return status('hatched',`Windows can't move some files near the end of ${name}, so it ${release<MiB ? 'cannot shrink it' : `can release only ${formatSize(release,'down')}`}.`,'recheck');
        const needed=formatSize(context.temporaryBytes-release,'up');
        return status('hatched',release<MiB ? `Windows can't release space from ${name}. ${needed} is needed.` : `Windows can release only ${formatSize(release,'down')} from ${name}. ${needed} more is needed.`,'recheck');
      }
      return status('hatched',region.resizeReason || `Windows can't shrink ${name}.`,'recheck');
    }
    case 'failed':
      return status('hatched',region.resizeReason || `Windows couldn't measure ${name}.`,'recheck');
    default:
      return !region.resizeReason || region.resizeReason===KEPT_REASON ? status('dimmed',`${name} · ${size} · kept as is`) : status('hatched',`${name} · ${size} · ${region.resizeReason}`);
  }
}

// Review bullets from the reviewed plan. `windows` is false when the shrunk
// partition isn't the Windows partition, so replacing names it instead.
export function reviewBullets(review:StagedReviewPlan,secureBoot:boolean|null|undefined,windows:boolean) {
  const bullets:string[]=[];
  const temporary=formatSize(review.temporary.sizeBytes);
  if(review.shrink) {
    const {label,beforeSizeBytes:before,afterSizeBytes:after}=review.shrink;
    const from=before>=GiB && after>=GiB ? formatSize(before).replace(/ GiB$/,'') : formatSize(before);
    bullets.push(`${label} shrinks from ${from} to ${formatSize(after)}. Your files stay.`);
    if(review.omarchy?.sizeBytes) bullets.push(`Omarchy gets ${formatSize(review.omarchy.sizeBytes)}. The ${temporary} installer is removed later.`);
    else bullets.push(`Adds a ${temporary} temporary installer. In the installer, delete ${windows ? 'Windows' : label} to make room.`);
  } else {
    bullets.push('Uses free space. Existing partitions stay.');
    bullets.push(freeTargetLine(review.largestFreeAfterBytes,review.minimumOmarchyBytes,{review:true}));
  }
  if(review.bitLocker.length) bullets.push(`BitLocker on ${review.bitLocker.join(' and ')} pauses until you resume it. Files stay encrypted.`);
  bullets.push(secureBoot ? 'Next: restart into firmware settings to turn off Secure Boot.' : 'Next: choose the installer for your next restart.');
  return bullets;
}

export function reviewWarning(review:StagedReviewPlan) {
  if(review.warning==='experimental') return 'Test build: real disk changes. This ISO may not start, or may refuse this disk.';
  if(review.warning==='unsigned') return 'Unsigned test image: real disk changes. It may not start or install.';
  return null;
}
