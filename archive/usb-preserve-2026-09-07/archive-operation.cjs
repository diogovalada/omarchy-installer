// One-time archival of the deferred USB experiment; run from the repository.
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const archive = path.join(root, 'archive/usb-preserve-2026-09-07');
if (fs.existsSync(archive)) throw new Error('Archive already exists');
fs.mkdirSync(archive, { recursive: true });
const snapshots = [
  'apps/desktop/src/lib/SetupPanel.svelte', 'apps/desktop/src/lib/setup.ts',
  'apps/desktop/src-tauri/src/setup.rs', 'apps/desktop/src-tauri/src/setup_helper.rs',
  'apps/desktop/src-tauri/src/setup_protocol.rs', 'apps/desktop/src-tauri/src/lib.rs',
  'scripts/stage-desktop-providers.mjs', 'THIRD_PARTY_NOTICES.md', 'ROADMAP.md',
  'docs/usb-preserve-implementation-2026-09-07.md', 'docs/usb-product-direction-2026-09-07.md',
  'docs/usb-preserve-files-2026-09-06.md',
];
for (const relative of snapshots) {
  const target = path.join(archive, 'integration-snapshot', relative);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.copyFileSync(path.join(root, relative), target, fs.constants.COPYFILE_EXCL);
}
for (const relative of ['providers/usb-preserve', 'apps/desktop/src-tauri/src/usb_preserve.rs', 'apps/desktop/src/lib/UsbSetupPanel.test.ts']) {
  const source = path.resolve(root, relative);
  const target = path.resolve(archive, relative);
  if (!source.startsWith(root + path.sep) || !target.startsWith(archive + path.sep)) throw new Error('Invalid archive path');
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.renameSync(source, target);
}
function edit(relative, transform) {
  const file = path.join(root, relative);
  const original = fs.readFileSync(file, 'utf8').replaceAll('\r\n', '\n');
  const changed = transform(original);
  if (changed === original) throw new Error('No change: ' + relative);
  fs.writeFileSync(file, changed);
}
function removeBetween(s, start, end) {
  const a = s.indexOf(start), b = s.indexOf(end, a + start.length);
  if (a < 0 || b < 0) throw new Error('Missing boundaries: ' + start);
  return s.slice(0, a) + s.slice(b);
}
edit('apps/desktop/src-tauri/src/lib.rs', s => s.replace('mod usb_preserve;\n', '').replace('            setup::inspect_usb_choice,\n', ''));
edit('apps/desktop/src-tauri/src/setup_protocol.rs', s => removeBetween(s, '    InspectUsb {', '    DirectX86 {'));
edit('apps/desktop/src-tauri/src/setup_helper.rs', s => removeBetween(s, '        if let Destination::InspectUsb {', '        if matches!(request.destination, Destination::PrepareFirmware)').replace('            Destination::InspectUsb { .. }\n            | Destination::UsbPreserve { .. }\n            | Destination::InspectDirect', '            Destination::InspectDirect'));
edit('apps/desktop/src-tauri/src/setup.rs', s => {
  s = s.replace('    usb: Option<crate::usb_preserve::Inspection>,\n', '').replace(/^\s*usb: None,\n/gm, '');
  s = s.replace('            if inner.snapshot.kind.as_deref() == Some("usb") {\n                for choice in &mut inner.snapshot.choices {\n                    choice.usb = None;\n                }\n            }\n', '');
  s = removeBetween(s, '// Inventory and boot restoration', '#[tauri::command]');
  s = removeBetween(s, '#[tauri::command]\npub fn inspect_usb_choice(', '#[tauri::command]\npub fn start_setup(');
  s = s.replace('    usb_mode: Option<String>,\n', '');
  s = s.replace(/    let source = if (?:kind == "usb"|usb_mode.as_deref\(\) == Some\("restore"\)) \{\n        usb_inspection_source\(&downloads\)\?\n    } else \{\n        source\(&downloads\)\?\n    };/g, '    let source = source(&downloads)?;');
  s = removeBetween(s, '    if let Some(mode) = usb_mode.as_deref() {', '    let kind = match &mut destination {');
  s = s.replace('Destination::Usb { .. } | Destination::UsbPreserve { .. }', 'Destination::Usb { .. }');
  s = s.replace('                    "usb_preserve" => (MessageDialogKind::Info, "Keep files and add installer"),\n', '').replace('                    "usb_restore" => (MessageDialogKind::Info, "Restore previous boot setup"),\n', '');
  return s;
});
edit('apps/desktop/src/lib/setup.ts', s => s.replace(/^export interface UsbInspection .*\n/m, '').replace('; usb?:UsbInspection|null', '').replace("mode?:'preserve'|'restore'; message?:string; backupPath?:string; ", '').replace(/^  inspectUsb:.*\n/m, '').replace(", usbMode?:'erase'|'preserve'|'restore'", '').replace(',usbMode:usbMode ?? null', ''));
edit('apps/desktop/src/lib/SetupPanel.svelte', s => {
  s = s.replace("!readyImage && kind !== 'usb'", '!readyImage');
  s = s.replace("(snapshot?.receipt?.receipt?.mode === 'restore' ? 'Previous USB boot setup restored.' : 'Bootable USB created and verified.')", "'Bootable USB created and verified.'");
  s = s.replace('snapshot?.receipt?.receipt?.message ?? ', '').replace(/^      \{#if snapshot\?\.receipt\?\.receipt\?\.backupPath}.*\n/m, '');
  s = s.replace('Choose a USB drive. We’ll check its free space and boot setup so you can keep existing files where supported, or erase the drive and create a fresh installer.', 'Choose a USB drive to erase and turn into an Omarchy installer. Every file and partition on the selected USB will be deleted.');
  s = s.replace(' onchange={()=>{void setup.inspectUsb(choice.id);}}', '');
  const start = s.indexOf('        {#if selectedChoice}');
  const end = s.indexOf('      {/if}\n      {#if snapshot?.status', start);
  if (start < 0 || end < 0) throw new Error('Missing USB UI');
  s = s.slice(0, start) + `        {#if selectedChoice}
          <div class="actions"><button class="erase" onclick={()=>{void setup.start(selected);}}>Erase USB and create installer</button></div>
          <p>Deletes every file and partition on this USB. You’ll confirm the selected drive before erasing it.</p>
        {/if}
` + s.slice(end);
  s = s.replace('.primary{background:var(--accent);border-color:var(--accent);color:var(--surface-deep)}', '');
  s = s.replace(/^  \.usb-options.*$/m, '  .erase{color:var(--error)}');
  return s;
});
edit('scripts/stage-desktop-providers.mjs', s => s.replace(", 'usb-preserve'", '').replace(/^    if \(providerName === 'usb-preserve'.*\n/m, '').replace('|EFI|xz|txt|dsc|template|cfg', ''));
edit('THIRD_PARTY_NOTICES.md', s => s.replace(/^\| GNU GRUB USB UEFI loader.*\n/m, ''));
edit('ROADMAP.md', s => {
  const a = s.indexOf('USB creation now has a requested second mode:');
  const b = s.indexOf('possible migration workflow, never an implicit fallback from adding alongside.', a);
  if (a < 0 || b < 0) throw new Error('Missing roadmap text');
  return s.slice(0, a) + 'USB preservation and bootloader backup/restoration are deferred at the user’s\nrequest. The experiment, integration snapshots, source notices and virtual boot\nevidence are retained in [the archive](archive/usb-preserve-2026-09-07/README.md),\nexcluded from the active application and provider bundle. The active USB flow\nis **Erase USB and create installer**, with explicit destructive confirmation.\n' + s.slice(b + 'possible migration workflow, never an implicit fallback from adding alongside.'.length);
});
for (const doc of ['docs/usb-preserve-implementation-2026-09-07.md', 'docs/usb-product-direction-2026-09-07.md', 'docs/usb-preserve-files-2026-09-06.md']) {
  edit(doc, s => '> **Archived / deferred, 2026-09-07:** The user paused this feature to avoid its testing burden. The implementation below is historical; it is excluded from the active app. See [archive and reactivation notes](../archive/usb-preserve-2026-09-07/README.md).\n\n' + s);
}
edit('.gitignore', s => s.replaceAll('/providers/usb-preserve/qualification/', '/archive/usb-preserve-2026-09-07/providers/usb-preserve/qualification/'));
fs.writeFileSync(path.join(archive, 'README.md'), `# Deferred USB preservation and boot recovery

Archived on 2026-09-07 at the user's request: postpone bootloader backup/restore
and its testing burden. The preserving mode replaces boot files and depends on
that recovery path, so both features are excluded from the active app.

The active USB workflow remains erase-and-create, with a destructive confirmation.

## Contents

- \`apps/\`: dormant Rust implementation and frontend tests.
- \`providers/usb-preserve/\`: loader, corresponding GNU GRUB source and licenses,
  reproducible build recipe, virtual boot qualification scripts and evidence.
- \`integration-snapshot/\`: exact pre-archive copies of files that wired the
  experiment into the UI, elevated helper, protocol and packaging, plus design docs.

These files are outside active Rust modules, frontend test discovery, and provider
staging. Integration snapshots are historical references, not files to copy over
newer app code wholesale. Existing preview executables are retained as historical
artifacts; bebf264d and 3099c1ff contain the experiment and are superseded.

## Status when paused

18 native and 17 frontend tests passed; the official Omarchy 4.0.2 ISO reached its
installer start screen from a GPT virtual USB with NTFS data and FAT32 EFI under
QEMU/OVMF. No physical USB qualification was performed. Saved evidence does not
establish compatibility with ordinary single-partition USBs or other layouts.

Resume only when requested. Reconcile the snapshots with the current app and
reassess identity binding, interrupted writes, bootloader restoration, physical
USB compatibility, and source/license packaging before re-enabling the feature.
`);
console.log('Archived USB preservation and removed active integrations.');
