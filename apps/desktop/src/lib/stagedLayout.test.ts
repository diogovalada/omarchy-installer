import { describe, expect, it } from 'vitest';
import type { StagedChoice, StagedDisk, StagedRegion, StagedReviewPlan } from './setup';
import { GiB, MiB, afterLayout, barWidths, cantKeepLine, cantReleaseLine, choiceFor, defaultRegion, editorSegments, formatDiskSize, formatSize, freeTargetLine, limits, overviewSegments, regionKey, regionStatus, reviewBullets, reviewWarning, windowsAfterBytes } from './stagedLayout';
import laptop from '../../test-fixtures/staged-layouts/laptop.json';

const temporaryBytes=7.5*GiB, minimumBytes=32*GiB, reserveBytes=Math.ceil(95.3*GiB/MiB)*MiB;
const efi:StagedRegion={kind:'partition',partitionNumber:1,partitionGuid:'efi',offsetBytes:MiB,sizeBytes:260*MiB,label:'EFI system',resizeState:'unavailable',resizeReason:'System or unsupported partition; kept unchanged.'};
const msr:StagedRegion={kind:'partition',partitionNumber:2,partitionGuid:'msr',offsetBytes:261*MiB,sizeBytes:16*MiB,label:'Microsoft reserved',resizeState:'unavailable',resizeReason:'System or unsupported partition; kept unchanged.'};
const windows:StagedRegion={kind:'partition',partitionNumber:3,partitionGuid:'windows',offsetBytes:300*MiB,sizeBytes:953*GiB,label:'C:',fileSystem:'NTFS',freeBytes:200*GiB,resizeState:'checked',resizeReason:'',maximumReleaseBytes:100*GiB,reserveBytes,isWindows:true};
const recovery:StagedRegion={kind:'partition',partitionNumber:4,partitionGuid:'recovery',offsetBytes:300*MiB+953*GiB,sizeBytes:GiB,label:'Windows recovery',resizeState:'unavailable',resizeReason:'System or unsupported partition; kept unchanged.'};
const diskOf=(regions:StagedRegion[],diskSizeBytes=300*MiB+954*GiB+MiB):StagedDisk => ({diskNumber:0,diskUniqueId:'disk',diskSizeBytes,unallocatedBytes:0,largestFreeBytes:0,regions,layoutSha256:'layout'});
const shrinkChoice=(region:StagedRegion):StagedChoice => ({diskNumber:0,diskUniqueId:'disk',label:`Resize ${region.label}`,largestFreeAfterStagingBytes:0,target:{target_kind:'shrink',partition_number:region.partitionNumber,partition_guid:region.partitionGuid}});
const freeChoice=(region:StagedRegion):StagedChoice => ({diskNumber:0,diskUniqueId:'disk',label:'Unallocated space',largestFreeAfterStagingBytes:region.sizeBytes-temporaryBytes,target:{target_kind:'free',start_offset_bytes:region.offsetBytes}});
const context={temporaryBytes,minimumBytes,usable:false};
const keepReview:StagedReviewPlan={diskNumber:0,diskSizeBytes:300*MiB+954*GiB+MiB,layoutSha256:'layout',targetKind:'shrink',
  shrink:{label:'C:',partitionGuid:'windows',offsetBytes:300*MiB,beforeSizeBytes:953*GiB,afterSizeBytes:881.5*GiB},
  temporary:{offsetBytes:300*MiB+881.5*GiB,sizeBytes:temporaryBytes},omarchy:{offsetBytes:300*MiB+889*GiB,sizeBytes:64*GiB},
  largestFreeAfterBytes:64*GiB,minimumOmarchyBytes:minimumBytes,bitLocker:['C:'],warning:'experimental'};

describe('staged layout sizes', () => {
  it('formats MiB below 1 GiB and drops a trailing .0', () => {
    expect(formatSize(260*MiB)).toBe('260 MiB');
    expect(formatSize(0)).toBe('0 MiB');
    expect(formatSize(953*GiB)).toBe('953 GiB');
    expect(formatSize(881.5*GiB)).toBe('881.5 GiB');
    expect(formatSize(7.5*GiB)).toBe('7.5 GiB');
    expect(formatSize(1023.6*MiB)).toBe('1 GiB');
  });

  it('rounds available space down and needed space up', () => {
    expect(formatSize(7.46*GiB,'down')).toBe('7.4 GiB');
    expect(formatSize(5.44*GiB,'up')).toBe('5.5 GiB');
    expect(formatSize(5.4*GiB,'up')).toBe('5.4 GiB');
    expect(formatSize(100,'up')).toBe('1 MiB');
    expect(formatSize(1023.2*MiB,'down')).toBe('1023 MiB');
  });

  it('formats disk headings in whole GiB or TiB', () => {
    expect(formatDiskSize(954.26*GiB)).toBe('954 GiB');
    expect(formatDiskSize(2000*1000**3)).toBe('1.8 TiB');
    expect(formatDiskSize(1024*GiB)).toBe('1 TiB');
    expect(formatDiskSize(1023.6*GiB)).toBe('1 TiB');
  });

  it('keys partitions by GUID and free space by offset', () => {
    expect(regionKey(windows)).toBe('windows');
    expect(regionKey({kind:'free',offsetBytes:5*GiB,sizeBytes:GiB,label:'Unallocated space'})).toBe(`free:${5*GiB}`);
  });

  it('matches regions to their choices', () => {
    const free:StagedRegion={kind:'free',offsetBytes:10*GiB,sizeBytes:20*GiB,label:'Unallocated space'};
    const choices=[freeChoice(free),shrinkChoice(windows)];
    expect(choiceFor(windows,choices)).toBe(choices[1]);
    expect(choiceFor(free,choices)).toBe(choices[0]);
    expect(choiceFor(efi,choices)).toBeUndefined();
  });
});

describe('staged overview', () => {
  it('groups small kept partitions, hides tiny gaps and keeps minimum widths', () => {
    const gap:StagedRegion={kind:'free',offsetBytes:277*MiB,sizeBytes:2*MiB,label:'Unallocated space'};
    const segments=overviewSegments(diskOf([efi,msr,gap,windows,recovery]));
    expect(segments.map(s=>s.label)).toEqual(['2 small partitions','C:','Windows recovery']);
    expect(segments[0].regions).toEqual([efi,msr]);
    expect(segments[0].widthPercent).toBeCloseTo(10/760*100);
    expect(segments[2].widthPercent).toBeCloseTo(10/760*100);
    expect(segments.reduce((total,s)=>total+s.widthPercent,0)).toBeCloseTo(100);
  });

  it('draws free space and keeps a single small partition on its own', () => {
    const free:StagedRegion={kind:'free',offsetBytes:300*MiB+953*GiB,sizeBytes:GiB,label:'Unallocated space'};
    const segments=overviewSegments(diskOf([efi,windows,free]),{barPx:390});
    expect(segments.map(s=>s.key)).toEqual(['efi','windows',`free:${free.offsetBytes}`]);
    expect(segments[2].label).toBe('Free space');
    expect(segments[0].widthPercent).toBeCloseTo(10/390*100);
  });

  it('shares the bar equally when every segment needs its minimum', () => {
    const segments=overviewSegments(diskOf([efi,windows,recovery]),{barPx:20});
    expect(segments.map(s=>s.widthPercent)).toEqual([100/3,100/3,100/3]);
  });

  it('scales any bar with the same rule', () => {
    const widths=barWidths([260*MiB,953*GiB,GiB]);
    expect(widths[0]).toBeCloseTo(10/760*100);
    expect(widths.reduce((a,b)=>a+b,0)).toBeCloseTo(100);
  });
});

describe('staged space editor', () => {
  it('rounds Omarchy limits to whole GiB', () => {
    expect(limits(windows,temporaryBytes,minimumBytes)).toEqual({minGiB:32,maxGiB:92,canKeep:true});
    expect(limits({...windows,maximumReleaseBytes:39*GiB},temporaryBytes,minimumBytes)).toEqual({minGiB:32,maxGiB:31,canKeep:false});
    expect(limits({...windows,resizeState:'unchecked',maximumReleaseBytes:null},temporaryBytes,33.5*GiB)).toEqual({minGiB:34,maxGiB:0,canKeep:false});
    expect(limits({...windows,maximumReleaseBytes:5*GiB},temporaryBytes,0)).toEqual({minGiB:0,maxGiB:0,canKeep:false});
  });

  it('lays out a kept Windows in provider order', () => {
    const segments=editorSegments(windows,{temporaryBytes,omarchyBytes:64*GiB,mode:'keep'});
    expect(segments.map(s=>[s.kind,s.label])).toEqual([
      ['used','in use'],['cant-release','reserved'],['free','unused'],
      ['temporary','Temporary installer 7.5 GiB'],['omarchy','Omarchy 64 GiB']
    ]);
    expect(segments[3].offsetBytes).toBe(300*MiB+881.5*GiB);
    expect(segments[4].offsetBytes).toBe(300*MiB+889*GiB);
    expect(segments[0].widthPercent).toBe(15);
    expect(segments.reduce((total,s)=>total+s.widthPercent,0)).toBeCloseTo(100);
    expect(segments[4].widthPercent/segments[3].widthPercent).toBeCloseTo(64/7.5);
    expect(windowsAfterBytes(windows,temporaryBytes,64*GiB)).toBe(881.5*GiB);
  });

  it('keeps a small installer visible without bending the handle scale', () => {
    const large={...windows,sizeBytes:4000*GiB,freeBytes:3000*GiB,maximumReleaseBytes:2500*GiB,reserveBytes:400*GiB};
    const start=(omarchy:number) => {
      const segments=editorSegments(large,{temporaryBytes,omarchyBytes:omarchy*GiB,mode:'keep',barPx:700});
      const index=segments.findIndex(s=>s.kind==='temporary');
      expect(segments[index].widthPercent).toBeCloseTo(24/700*100);
      expect(segments.reduce((total,s)=>total+s.widthPercent,0)).toBeCloseTo(100);
      return segments.slice(0,index).reduce((total,s)=>total+s.widthPercent,0);
    };
    expect(start(32)-start(64)).toBeCloseTo(start(64)-start(96));
  });

  it('aligns the installer to the MiB below the partition end', () => {
    const unaligned={...windows,sizeBytes:953*GiB+12345};
    const [temporary]=editorSegments(unaligned,{temporaryBytes,mode:'replace'}).filter(s=>s.kind==='temporary');
    expect(temporary.offsetBytes).toBe(300*MiB+945.5*GiB);
    expect(windowsAfterBytes(unaligned,temporaryBytes)).toBe(945.5*GiB);
  });

  it('shrinks only for the installer when replacing Windows', () => {
    const segments=editorSegments(windows,{temporaryBytes,omarchyBytes:64*GiB,mode:'replace'});
    expect(segments.map(s=>[s.kind,s.sizeBytes])).toEqual([['used',753*GiB],['cant-release',100*GiB],['free',92.5*GiB],['temporary',temporaryBytes]]);
  });

  it("explains the space Windows can't release", () => {
    expect(cantReleaseLine(windows)).toBe("95.3 GiB stays free for Windows, and some files near the end of C: can't be moved.");
    expect(cantReleaseLine({...windows,maximumReleaseBytes:200*GiB-reserveBytes-512*MiB})).toBe('95.3 GiB stays free for Windows.');
    expect(cantReleaseLine({...windows,reserveBytes:null})).toBe("Windows can't release this space from C:.");
    expect(cantReleaseLine({...windows,reserveBytes:0})).toBe("Windows can't release this space from C:.");
    expect(cantReleaseLine({...windows,resizeState:'insufficient'})).toBe("Windows can't release this space from C:.");
  });

  it("says why Windows can't be kept, and when freeing space helps", () => {
    // The reserve is the limit: what is freed can be released.
    expect(cantKeepLine({...windows,freeBytes:reserveBytes+30*GiB,maximumReleaseBytes:30*GiB,limitedByFiles:false,filesReleaseBytes:200*GiB},temporaryBytes,minimumBytes,'Windows'))
      .toBe('Windows can give only 22.5 GiB; Omarchy needs 32 GiB. Free up 9.5 GiB more in Windows, then refresh.');
    // The reserve limits it now, but files would cap it below what keeping needs: no advice.
    expect(cantKeepLine({...windows,freeBytes:reserveBytes+30*GiB,maximumReleaseBytes:30*GiB,limitedByFiles:false,filesReleaseBytes:35*GiB},temporaryBytes,minimumBytes,'Windows'))
      .toBe('Windows can give only 22.5 GiB; Omarchy needs 32 GiB.');
    // Files Windows can't move are the limit: freeing space doesn't help.
    expect(cantKeepLine({...windows,maximumReleaseBytes:30*GiB},temporaryBytes,minimumBytes,'Data (D:)'))
      .toBe('Data (D:) can give only 22.5 GiB; Omarchy needs 32 GiB.');
    // Even emptied, a 50 GiB partition couldn't release enough: no advice.
    expect(cantKeepLine({...windows,sizeBytes:50*GiB,freeBytes:40*GiB,reserveBytes:20*GiB,maximumReleaseBytes:20*GiB},8*GiB,minimumBytes,'Data (D:)'))
      .toBe('Data (D:) can give only 12 GiB; Omarchy needs 32 GiB.');
  });

  it('describes the space left in a free region', () => {
    expect(freeTargetLine(112*GiB,minimumBytes)).toBe('Omarchy can use 112 GiB.');
    expect(freeTargetLine(31.99*GiB,minimumBytes)).toBe('Only 31.9 GiB remains; Omarchy needs 32 GiB. Delete a partition in the installer to make room.');
  });

  it('puts the installer at the start of free space and leaves the rest to Omarchy', () => {
    const free:StagedRegion={kind:'free',offsetBytes:500*GiB,sizeBytes:120*GiB,label:'Unallocated space'};
    const segments=editorSegments(free,{temporaryBytes,omarchyBytes:64*GiB,mode:'keep'});
    expect(segments.map(s=>[s.kind,s.offsetBytes,s.label])).toEqual([
      ['temporary',500*GiB,'Temporary installer 7.5 GiB'],['omarchy',507.5*GiB,'Omarchy']
    ]);
  });

  it('rounds available space down and the installer up, as the lines do', () => {
    const free:StagedRegion={kind:'free',offsetBytes:500*GiB,sizeBytes:119.96*GiB,label:'Unallocated space'};
    const temporary=7.46*GiB;
    expect(editorSegments(free,{temporaryBytes:temporary,mode:'keep'}).map(s=>s.label)).toEqual(['Temporary installer 7.5 GiB','Omarchy']);
    expect(freeTargetLine(free.sizeBytes-temporary,minimumBytes)).toBe('Omarchy can use 112.5 GiB.');
    expect(editorSegments({...free,sizeBytes:120*GiB},{temporaryBytes:7.54*GiB,mode:'keep'})[0].label).toBe('Temporary installer 7.6 GiB');
    expect(freeTargetLine(120*GiB-7.54*GiB,minimumBytes)).toBe('Omarchy can use 112.4 GiB.');
  });
});

describe('staged default selection', () => {
  const free:StagedRegion={kind:'free',offsetBytes:300*MiB+953*GiB,sizeBytes:120*GiB,label:'Unallocated space'};
  const data:StagedRegion={...windows,partitionNumber:5,partitionGuid:'data',label:'D:',offsetBytes:2000*GiB,sizeBytes:1000*GiB,maximumReleaseBytes:200*GiB,isWindows:false};

  it('prefers the largest free space that fits Omarchy', () => {
    const disk=diskOf([efi,windows,free],2000*GiB);
    expect(defaultRegion(disk,[shrinkChoice(windows),freeChoice(free)],temporaryBytes,minimumBytes)).toBe(free);
    const larger:StagedRegion={...free,offsetBytes:1500*GiB,sizeBytes:300*GiB};
    expect(defaultRegion(diskOf([efi,windows,free,larger],2000*GiB),[freeChoice(free),freeChoice(larger)],temporaryBytes,minimumBytes)).toBe(larger);
  });

  it("ignores another disk's choices", () => {
    const disk=diskOf([efi,windows,free],2000*GiB);
    expect(defaultRegion(disk,[{...freeChoice(free),diskUniqueId:'other'}],temporaryBytes,minimumBytes)).toBe(windows);
  });

  it('then the shrink that keeps Windows with the most room', () => {
    const small={...free,sizeBytes:30*GiB};
    const disk=diskOf([efi,windows,small,data],4000*GiB);
    expect(defaultRegion(disk,[freeChoice(small),shrinkChoice(windows),shrinkChoice(data)],temporaryBytes,minimumBytes)).toBe(data);
  });

  it('then the first usable target, then the partition that explains why not', () => {
    const tight={...windows,maximumReleaseBytes:10*GiB};
    const disk=diskOf([efi,tight,data],4000*GiB);
    expect(defaultRegion(disk,[shrinkChoice(tight)],temporaryBytes,minimumBytes)).toBe(tight);
    expect(defaultRegion(disk,[],temporaryBytes,minimumBytes)).toBe(tight);
    const checking={...windows,isWindows:undefined,resizeState:'unchecked' as const};
    expect(defaultRegion(diskOf([efi,checking,data],4000*GiB),[],temporaryBytes,minimumBytes)).toBe(checking);
    expect(defaultRegion(diskOf([efi,{...windows,isWindows:undefined},data],4000*GiB),[],temporaryBytes,minimumBytes)).toBe(data);
    expect(defaultRegion(diskOf([efi]),[],temporaryBytes,minimumBytes)).toBeUndefined();
  });
});

describe('staged region status', () => {
  it('describes kept and unsupported partitions', () => {
    expect(regionStatus(efi,context)).toEqual({tone:'dimmed',text:'EFI system · 260 MiB · kept as is',action:null});
    const exfat:StagedRegion={kind:'partition',offsetBytes:0,sizeBytes:50*GiB,label:'Data (E:)',fileSystem:'exFAT',resizeState:'unavailable',resizeReason:'Only NTFS partitions can be resized here.'};
    expect(regionStatus(exfat,context)).toEqual({tone:'hatched',text:'Data (E:) · 50 GiB · Only NTFS partitions can be resized here.',action:null});
    expect(regionStatus([efi,msr],context)).toEqual({tone:'dimmed',text:'EFI system 260 MiB · Microsoft reserved 16 MiB · kept as is',action:null});
    expect(regionStatus({kind:'partition',offsetBytes:0,sizeBytes:GiB,label:'Partition 9'},context).text).toBe('Partition 9 · 1 GiB · kept as is');
    expect(regionStatus([],context).text).toBe('');
  });

  it('asks for more free space when the precheck fails', () => {
    const full:StagedRegion={...windows,sizeBytes:100*GiB,freeBytes:22.1*GiB,resizeState:'insufficient',maximumReleaseBytes:null,reserveBytes:20*GiB};
    expect(regionStatus(full,context)).toEqual({tone:'hatched',text:'C: needs 5.4 GiB more free space (37.4 GiB to keep Windows). Free up space in Windows, then refresh.',action:'refresh',cause:'space'});
    expect(regionStatus({...full,freeBytes:22.06*GiB},context).text).toBe('C: needs 5.5 GiB more free space (37.5 GiB to keep Windows). Free up space in Windows, then refresh.');
    expect(regionStatus({...full,freeBytes:27.5*GiB-100},context).text).toBe('C: needs 1 MiB more free space (32.1 GiB to keep Windows). Free up space in Windows, then refresh.');
    // Even empty, a 40 GiB partition can't keep Windows beside the installer, Omarchy and the reserve.
    expect(regionStatus({...full,sizeBytes:40*GiB,freeBytes:22.1*GiB},context).text).toBe('C: needs 5.4 GiB more free space. Free up space in Windows, then refresh.');
    // A data partition is named by its label.
    expect(regionStatus({...full,label:'D:',isWindows:false},context).text).toBe('D: needs 5.4 GiB more free space (37.4 GiB to keep D:). Free up space in Windows, then refresh.');
  });

  it("says when a partition can't hold the installer even if empty", () => {
    const small:StagedRegion={...windows,label:'RESTORE (Partition 5)',sizeBytes:22*GiB,freeBytes:8.6*GiB,resizeState:'insufficient',maximumReleaseBytes:null,reserveBytes:20*1000**3};
    expect(regionStatus(small,context)).toEqual({tone:'hatched',text:'RESTORE (Partition 5) is too small for the installer.',action:null});
  });

  it('shows checking and unchecked partitions', () => {
    const unchecked:StagedRegion={...windows,resizeState:'unchecked',maximumReleaseBytes:null};
    expect(regionStatus(unchecked,{...context,checking:true})).toEqual({tone:'checking',text:'Checking how much C: can shrink. This can take a few minutes.',action:null});
    expect(regionStatus(unchecked,context)).toEqual({tone:'dimmed',text:"C: wasn't checked. Refresh to try again.",action:'refresh'});
  });

  it('shows measured limits, shortfalls and failures', () => {
    expect(regionStatus(windows,{...context,usable:true})).toEqual({tone:'normal',text:'C: can give Omarchy up to 92 GiB',action:null});
    expect(regionStatus({...windows,maximumReleaseBytes:20*GiB},{...context,usable:true}).text).toBe('C: has room for the installer only');
    expect(regionStatus({...windows,maximumReleaseBytes:3*GiB},context)).toEqual({tone:'hatched',text:"Windows can't move some files near the end of C:, so it can release only 3 GiB.",action:'recheck',cause:'files'});
    expect(regionStatus({...windows,maximumReleaseBytes:0},context).text).toBe("Windows can't move some files near the end of C:, so it can't shrink it.");
    // Files also limit the release (4 GiB of 5 GiB room), so freeing space may not be enough to keep Windows.
    expect(regionStatus({...windows,freeBytes:25*GiB,reserveBytes:20*GiB,maximumReleaseBytes:4*GiB},context)).toEqual({tone:'hatched',text:'C: needs 2.5 GiB more free space. Free up space in Windows, then recheck.',action:'recheck',cause:'space'});
    // Within alignment of the room, but Windows' own minimum (files) sets it: freeing space won't help.
    expect(regionStatus({...windows,sizeBytes:100*GiB,freeBytes:27.5*GiB+MiB,reserveBytes:20*GiB,maximumReleaseBytes:7.5*GiB-MiB,limitedByFiles:true},context)).toEqual({tone:'hatched',text:"Windows can't move some files near the end of C:, so it can release only 7.4 GiB.",action:'recheck',cause:'files'});
    // The release tracks the room, short by alignment: both figures count from the release.
    expect(regionStatus({...windows,freeBytes:25*GiB,reserveBytes:20*GiB,maximumReleaseBytes:5*GiB-MiB,limitedByFiles:false,filesReleaseBytes:200*GiB},context).text).toBe('C: needs 2.6 GiB more free space (34.6 GiB to keep Windows). Free up space in Windows, then recheck.');
    // The reserve limits it now, but files would cap it at 20 GiB once space is freed: no keeping amount.
    expect(regionStatus({...windows,sizeBytes:100*GiB,freeBytes:27.5*GiB,reserveBytes:20*GiB,maximumReleaseBytes:7.5*GiB-MiB,limitedByFiles:false,filesReleaseBytes:20*GiB},context).text).toBe('C: needs 1 MiB more free space. Free up space in Windows, then recheck.');
    expect(regionStatus({...windows,freeBytes:reserveBytes+temporaryBytes+MiB,maximumReleaseBytes:temporaryBytes-MiB,limitedByFiles:false,filesReleaseBytes:200*GiB},context).text).toBe('C: needs 1 MiB more free space (32.1 GiB to keep Windows). Free up space in Windows, then recheck.');
    expect(regionStatus({...windows,reserveBytes:0,maximumReleaseBytes:0,resizeReason:'Windows returned unsupported NTFS shrink limits.'},context).text).toBe('Windows returned unsupported NTFS shrink limits.');
    expect(regionStatus({...windows,reserveBytes:null,maximumReleaseBytes:7.46*GiB},context).text).toBe('Windows can release only 7.4 GiB from C:. 41 MiB more is needed.');
    expect(regionStatus({...windows,reserveBytes:null,maximumReleaseBytes:0},context).text).toBe("Windows can't release space from C:. 7.5 GiB is needed.");
    expect(regionStatus(windows,context)).toEqual({tone:'hatched',text:"Windows can't shrink C:.",action:'recheck'});
    expect(regionStatus({...windows,resizeReason:'NTFS volume must be writable, healthy and operational.'},context).text).toBe('NTFS volume must be writable, healthy and operational.');
    expect(regionStatus({...windows,resizeState:'failed',maximumReleaseBytes:null},context)).toEqual({tone:'hatched',text:"Windows couldn't measure C:.",action:'recheck',cause:'measure'});
    // The provider's own reason goes to the All partitions table.
    expect(regionStatus({...windows,resizeState:'failed',resizeReason:'The volume is dirty.'},context)).toEqual({tone:'hatched',text:"Windows couldn't measure C:.",action:'recheck',cause:'measure',detail:'The volume is dirty.'});
  });

  it('shows usable and small free space', () => {
    const free:StagedRegion={kind:'free',offsetBytes:0,sizeBytes:120*GiB,label:'Unallocated space'};
    expect(regionStatus(free,{...context,usable:true})).toEqual({tone:'outlined',text:'Free space · 120 GiB',action:null});
    expect(regionStatus({...free,sizeBytes:2*GiB},context)).toEqual({tone:'outlined-dimmed',text:'Free space · 2 GiB · too small',action:null});
    expect(regionStatus({...free,sizeBytes:7.46*GiB},context).text).toBe('Free space · 7.4 GiB · too small');
  });
});

describe('staged review', () => {
  it('projects a kept Windows over the inspected disk', () => {
    const after=afterLayout(diskOf([efi,windows,recovery]),keepReview);
    expect(after?.map(r=>[r.kind,r.label,r.offsetBytes,r.sizeBytes])).toEqual([
      ['partition','EFI system',MiB,260*MiB],['partition','C:',300*MiB,881.5*GiB],
      ['temporary','Temporary installer',300*MiB+881.5*GiB,temporaryBytes],['omarchy','Omarchy',300*MiB+889*GiB,64*GiB],
      ['partition','Windows recovery',300*MiB+953*GiB,GiB]
    ]);
  });

  it('leaves out the alignment sliver after an unaligned partition end', () => {
    const unaligned={...windows,sizeBytes:953*GiB+12345};
    const replace={...keepReview,omarchy:null,largestFreeAfterBytes:0,shrink:{...keepReview.shrink!,beforeSizeBytes:unaligned.sizeBytes,afterSizeBytes:945.5*GiB},temporary:{offsetBytes:300*MiB+945.5*GiB,sizeBytes:temporaryBytes}};
    const after=afterLayout(diskOf([efi,unaligned,{...recovery,offsetBytes:recovery.offsetBytes+MiB}]),replace);
    expect(after?.map(r=>r.kind)).toEqual(['partition','partition','temporary','partition']);
  });

  it('projects free space around the installer', () => {
    const free:StagedRegion={kind:'free',offsetBytes:300*MiB+953*GiB,sizeBytes:120*GiB,label:'Unallocated space'};
    const review:StagedReviewPlan={...keepReview,diskSizeBytes:2000*GiB,targetKind:'free',shrink:null,omarchy:null,temporary:{offsetBytes:free.offsetBytes,sizeBytes:temporaryBytes},largestFreeAfterBytes:112.5*GiB};
    const after=afterLayout(diskOf([efi,windows,free],2000*GiB),review);
    expect(after?.map(r=>[r.kind,r.offsetBytes,r.sizeBytes])).toEqual([
      ['partition',MiB,260*MiB],['partition',300*MiB,953*GiB],['temporary',free.offsetBytes,temporaryBytes],['free',free.offsetBytes+temporaryBytes,112.5*GiB]
    ]);
  });

  it('draws nothing when the disk changed since the check', () => {
    expect(afterLayout(diskOf([efi,{...windows,sizeBytes:952*GiB},recovery]),keepReview)).toBeNull();
    expect(afterLayout(diskOf([efi,windows,recovery],2000*GiB),keepReview)).toBeNull();
    expect(afterLayout(diskOf([efi,windows,recovery]),{...keepReview,targetKind:'free',shrink:null})).toBeNull();
  });

  it('draws nothing when the layout fingerprint differs or is missing', () => {
    // For example, a new partition now ends the free region the installer
    // still starts in: the target looks the same, the fingerprint doesn't.
    const free:StagedRegion={kind:'free',offsetBytes:300*MiB+953*GiB,sizeBytes:120*GiB,label:'Unallocated space'};
    const review:StagedReviewPlan={...keepReview,diskSizeBytes:2000*GiB,targetKind:'free',shrink:null,omarchy:null,temporary:{offsetBytes:free.offsetBytes,sizeBytes:temporaryBytes},largestFreeAfterBytes:82.5*GiB};
    expect(afterLayout(diskOf([efi,windows,free],2000*GiB),review)).not.toBeNull();
    expect(afterLayout(diskOf([efi,windows,free],2000*GiB),{...review,layoutSha256:'changed'})).toBeNull();
    expect(afterLayout({...diskOf([efi,windows,free],2000*GiB),layoutSha256:undefined},review)).toBeNull();
  });

  it('builds bullets for keeping Windows', () => {
    expect(reviewBullets(keepReview,true,true)).toEqual([
      'C: shrinks from 953 to 881.5 GiB. Your files stay.',
      'Omarchy can use 64 GiB. The 7.5 GiB installer is removed later.',
      'BitLocker on C: pauses until you resume it. Files stay encrypted.',
      'Next: turn off Secure Boot in firmware settings.'
    ]);
  });

  it('counts free space that joins or exceeds the chosen size', () => {
    // 120 GiB of free space after C: joins Omarchy's 64 GiB.
    expect(reviewBullets({...keepReview,largestFreeAfterBytes:184*GiB},false,true)[1]).toBe('Omarchy can use 184 GiB. The 7.5 GiB installer is removed later.');
  });

  it('builds bullets for replacing a partition and for free space', () => {
    const replace={...keepReview,omarchy:null,bitLocker:[]};
    expect(reviewBullets(replace,false,true)).toEqual([
      'C: shrinks from 953 to 881.5 GiB. Your files stay.',
      'Adds a 7.5 GiB temporary installer. In the installer, delete Windows to make room.',
      'Next: set your next restart to open the installer.'
    ]);
    expect(reviewBullets({...replace,shrink:{...replace.shrink!,label:'D:'}},null,false)[1]).toBe('Adds a 7.5 GiB temporary installer. In the installer, delete D: to make room.');
    const free={...replace,targetKind:'free' as const,shrink:null,largestFreeAfterBytes:112*GiB};
    expect(reviewBullets(free,undefined,true).slice(0,2)).toEqual(['Uses free space. Existing partitions stay.','Omarchy can use 112 GiB.']);
    expect(reviewBullets({...free,largestFreeAfterBytes:20*GiB},false,true)[1]).toBe('Only 20 GiB remains; Omarchy needs 32 GiB. Delete a partition in the installer to make room.');
    expect(reviewBullets({...keepReview,bitLocker:['C:','D:','E:']},false,true)[2]).toBe('BitLocker on C:, D: and E: pauses until you resume it. Files stay encrypted.');
  });

  it('names the test-build warning', () => {
    expect(reviewWarning(keepReview)).toBe('Test build: real disk changes. This image may not start or install.');
    expect(reviewWarning({...keepReview,warning:'unsigned'})).toBe('Unsigned test image: real disk changes. It may not start or install.');
    expect(reviewWarning({...keepReview,warning:null})).toBeNull();
  });
});

describe('captured laptop layout', () => {
  // A real disk check (test-fixtures/staged-layouts/laptop.json): Windows
  // can't shrink C: at all, and the OEM RESTORE partition is too small.
  const captured=laptop as unknown as {disks:StagedDisk[];choices:StagedChoice[];temporaryBytes:number;minimumLinuxBytes:number};
  const disk=captured.disks[0];
  const c=disk.regions.find(r=>r.isWindows)!;
  const restore=disk.regions.find(r=>r.label.startsWith('RESTORE'))!;
  const statusContext={temporaryBytes:captured.temporaryBytes,minimumBytes:captured.minimumLinuxBytes,usable:false};

  it('groups the small system partitions and hides the alignment gap', () => {
    expect(overviewSegments(disk).map(s=>s.label)).toEqual(['2 small partitions','OS (C:)','Windows recovery','RESTORE (Partition 5)','Windows recovery']);
  });

  it('selects C: to explain why no space can be used', () => {
    expect(captured.choices).toEqual([]);
    expect(defaultRegion(disk,captured.choices,captured.temporaryBytes,captured.minimumLinuxBytes)).toBe(c);
  });

  it('explains both partitions', () => {
    expect(regionStatus(c,statusContext)).toEqual({tone:'hatched',text:"Windows can't move some files near the end of OS (C:), so it can't shrink it.",action:'recheck',cause:'files'});
    expect(cantReleaseLine(c)).toBe("93 GiB stays free for Windows, and some files near the end of OS (C:) can't be moved.");
    expect(regionStatus(restore,statusContext)).toEqual({tone:'hatched',text:'RESTORE (Partition 5) is too small for the installer.',action:null});
  });
});
