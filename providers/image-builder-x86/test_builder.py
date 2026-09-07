import json
from pathlib import Path
import tempfile
import subprocess
import unittest
from unittest.mock import Mock, patch

import builder
from configuration import configuration, DISK_BYTES, MIB, GIB


class SafetyAndProofTests(unittest.TestCase):
    def test_only_virtual_target_and_bounded_partition_layout(self):
        config = configuration()
        disks = config['disk_config']['device_modifications']
        self.assertEqual([d['device'] for d in disks], ['/dev/vda'])
        self.assertEqual(config['custom_commands'], [])
        self.assertNotIn('disk_encryption', config['disk_config'])
        boot, root = disks[0]['partitions']
        self.assertEqual(boot['start']['value'], MIB)
        self.assertEqual(boot['size']['value'], 2 * GIB)
        self.assertEqual(root['start']['value'], 2 * GIB + MIB)
        self.assertEqual(root['start']['value'] + root['size']['value'], DISK_BYTES - MIB)
        self.assertEqual(root['fs_type'], 'btrfs')

    def test_each_install_has_unique_partition_ids_and_hostname(self):
        a, b = configuration(), configuration()
        self.assertNotEqual(a['hostname'], b['hostname'])
        self.assertNotEqual(a['disk_config'], b['disk_config'])

    def test_deferred_profile_has_no_credentials_or_encryption(self):
        config = configuration(deferred=True)
        self.assertTrue(config['omarchy_install']['defer_provisioning'])
        self.assertNotIn('password', json.dumps(config))
        self.assertNotIn('encryption', json.dumps(config))

    def test_independent_boot_has_neither_iso_nor_cidata(self):
        run = Path('/runs/fresh-operation')
        args = builder.qemu_arguments(run, run / 'image.building.qcow2')
        joined = '\n'.join(args)
        self.assertNotIn('cdrom', joined)
        self.assertNotIn('cidata', joined)
        self.assertNotIn('/input', joined)
        self.assertNotIn('/dev/', joined)
        self.assertIn('q35,accel=tcg', args)
        self.assertNotIn('-enable-kvm', args)
        self.assertNotIn('-virtfs', args)

    def test_install_media_are_read_only_and_only_fixed_files(self):
        run = Path('/runs/fresh-operation')
        args = builder.qemu_arguments(run, run / 'disk.qcow2', iso=Path('/input/omarchy-4.0.2.iso'))
        drives = [args[i + 1] for i, arg in enumerate(args[:-1]) if arg == '-drive']
        self.assertTrue(all('readonly=on' in arg for arg in drives if 'cdrom' in arg or 'cidata' in arg))
        self.assertIn('restrict=on,hostfwd=tcp:127.0.0.1:2322-:22', '\n'.join(args))

    def test_input_rejects_links_directories_and_escape(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            valid = root / 'image.iso'
            valid.write_bytes(b'ISO')
            self.assertEqual(builder.regular_file(valid, root), valid)
            with self.assertRaises(ValueError):
                builder.regular_file(root, root)
            nested = root / 'other'
            nested.mkdir()
            escape = nested / 'escape.iso'
            escape.write_bytes(b'ISO')
            with self.assertRaises(ValueError):
                builder.regular_file(escape, root)
            link = root / 'link.iso'
            link.symlink_to(valid)
            with self.assertRaises(ValueError):
                builder.regular_file(link, root)
            hard = root / 'hard.iso'
            hard.hardlink_to(valid)
            with self.assertRaises(ValueError):
                builder.regular_file(hard, root)

    def test_checksum_mismatch_rejected_before_signature_execution(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'omarchy-test.iso').write_bytes(b'bad')
            (root / 'omarchy-test.iso.sig').write_bytes(b'sig')
            with patch.object(builder, 'INPUT', root), patch.object(builder, 'command') as execute:
                with self.assertRaisesRegex(ValueError, 'checksum'):
                    builder.verify_source({'version': 'test', 'sizeBytes': 3, 'sha256': '0' * 64}, root)
                execute.assert_not_called()

    def test_guest_proof_requires_installed_root_identity_and_package(self):
        builder.assert_guest_proof('btrfs\n/dev/vda2[/@]\n' + 'a' * 32 + '\nomarchy 4.0.2\n')
        builder.assert_guest_proof('btrfs\n/dev/vda2\n' + 'a' * 32 + '\nomarchy 4.0.2\n')
        for invalid in ['overlay\noverlay\n' + 'a' * 32 + '\nomarchy 4.0.2',
                        'btrfs\n/dev/sda2\n' + 'a' * 32 + '\nomarchy 4.0.2',
                        'btrfs\n/dev/vda20\n' + 'a' * 32 + '\nomarchy 4.0.2',
                        'btrfs\n/dev/vda2-other\n' + 'a' * 32 + '\nomarchy 4.0.2',
                        'btrfs\n/dev/vda2[/@]trailing\n' + 'a' * 32 + '\nomarchy 4.0.2',
                        'btrfs\n/dev/vda2[/@]\n\nomarchy 4.0.2',
                        'btrfs\n/dev/vda2[/@]\n' + 'a' * 32 + '\nno package']:
            with self.assertRaises(ValueError):
                builder.assert_guest_proof(invalid)

    def test_live_key_is_discarded_installed_key_pinned_and_later_mismatch_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Path(temp)
            candidate, known = run / 'discovery_known_hosts', run / 'known_hosts'

            def live(args, **kwargs):
                self.assertIn('StrictHostKeyChecking=accept-new', args)
                self.assertIn(f'UserKnownHostsFile={candidate}', args)
                candidate.write_text('live-key\n')
                return subprocess.CompletedProcess(args, 255, '', 'authentication denied')

            with patch.object(builder, 'command', side_effect=live):
                self.assertFalse(builder.probe_installed(run, discover=True))
            self.assertFalse(known.exists())
            self.assertFalse(candidate.exists())

            def installed(args, **kwargs):
                self.assertIn('test ! -d /run/archiso', args[-1])
                candidate.write_text('installed-key\n')
                return subprocess.CompletedProcess(args, 0, '', '')

            with patch.object(builder, 'command', side_effect=installed):
                self.assertTrue(builder.probe_installed(run, discover=True))
            self.assertEqual(known.read_text(), 'installed-key\n')

            def mismatch(args, **kwargs):
                self.assertIn('StrictHostKeyChecking=yes', args)
                self.assertIn(f'UserKnownHostsFile={known}', args)
                self.assertNotIn('StrictHostKeyChecking=accept-new', args)
                return subprocess.CompletedProcess(args, 255, '', 'host key changed')

            with patch.object(builder, 'command', side_effect=mismatch):
                self.assertFalse(builder.probe_installed(run))
            self.assertEqual(known.read_text(), 'installed-key\n')

    def test_authenticated_guest_without_installed_marker_cannot_pin_key(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Path(temp)

            def missing_marker(args, **kwargs):
                (run / 'discovery_known_hosts').write_text('unqualified-key\n')
                return subprocess.CompletedProcess(args, 1, '', '')

            with patch.object(builder, 'command', side_effect=missing_marker):
                self.assertFalse(builder.probe_installed(run, discover=True))
            self.assertFalse((run / 'known_hosts').exists())
            self.assertFalse((run / 'discovery_known_hosts').exists())

    def test_install_wait_retries_transient_ssh_timeout(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Path(temp)
            vm = Mock()
            vm.process.poll.return_value = None
            vm.screenshot.return_value = run / 'latest-screen.png'
            probes = 0

            def execute(args, **kwargs):
                nonlocal probes
                if args[0] == 'ssh':
                    probes += 1
                    (run / 'discovery_known_hosts').write_text(f'candidate-{probes}\n')
                    if probes == 1:
                        raise subprocess.TimeoutExpired(args, 15)
                    return subprocess.CompletedProcess(args, 0, '', '')
                return subprocess.CompletedProcess(args, 0, 'Installing Omarchy', '')

            with patch.object(builder, 'command', side_effect=execute), patch.object(builder.time, 'sleep'):
                builder.wait_installed(vm, run, 30, discover=True)
            self.assertEqual(probes, 2)
            self.assertEqual((run / 'known_hosts').read_text(), 'candidate-2\n')
            self.assertFalse((run / 'discovery_known_hosts').exists())

    def test_iso_offset_accepts_contiguous_multi_extent_file(self):
        report = "File data lba: 0 , 132242 , 2097151 , 5915328512 , '/arch/x86_64/airootfs.sfs'\n"
        report += "File data lba: 1 , 2229393 , 791193 , 5915328512 , '/arch/x86_64/airootfs.sfs'\n"
        self.assertEqual(builder.squashfs_offset(report, 6227752960), 270831616)
        for invalid in [report.replace('2229393', '2229400'),
                        report.replace('5915328512', '9999999999'),
                        report.replace('File data lba: 1', 'File data lba: 3'), '']:
            with self.assertRaises(ValueError):
                builder.squashfs_offset(invalid, 6227752960)
        with self.assertRaises(ValueError):
            builder.squashfs_offset(report, 512)


if __name__ == '__main__':
    unittest.main(verbosity=2)
