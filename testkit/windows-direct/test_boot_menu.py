"""Pure first-boot/update/reset regression tests; no installed paths are touched."""
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'providers/image-builder-x86'))
from boot_menu import after_update, rebind_machine, render

OLD = '1' * 32
NEW = '2' * 32
SETTINGS = {'defaultOs': 'windows', 'timeoutSeconds': 10}
TEMPLATE = (Path(__file__).parent / 'fixtures/limine-reset.conf').read_text()
KERNELS = f'''{TEMPLATE}
/Omarchy
  comment: machine-id={OLD}
//linux
  protocol: efi
  path: boot():/EFI/Linux/linux.efi#123456
//linux-fallback
  protocol: efi
  path: boot():/EFI/Linux/fallback.efi#abcdef
'''


class MenuLifecycle(unittest.TestCase):
    def test_owner_rekey_reset_allows_upstream_generation_then_restores_preferences(self):
        managed = render(KERNELS, SETTINGS)
        rebound = rebind_machine(managed, NEW, TEMPLATE)
        self.assertIn(f'machine-id={NEW}', rebound)
        # Upstream owner rekey replaces the config, then calls its boot tools.
        self.assertEqual(rebind_machine(TEMPLATE, NEW, TEMPLATE), TEMPLATE)
        self.assertEqual(after_update(TEMPLATE, SETTINGS, TEMPLATE, 'limine-install'), TEMPLATE)
        rebuilt = KERNELS.replace(OLD, NEW).replace('linux.efi#123456', 'linux.efi#fresh')
        final = after_update(rebuilt, SETTINGS, TEMPLATE, 'limine-mkinitcpio-install')
        self.assertIn('default_entry: Windows', final)
        self.assertIn('timeout: 10', final)
        self.assertIn('linux.efi#fresh', final)
        self.assertNotIn('linux.efi#123456', final)
        self.assertEqual(render(final, SETTINGS), final)

    def test_factory_reset_retry_accepts_the_same_template(self):
        for _ in range(3):
            self.assertEqual(rebind_machine(TEMPLATE, NEW, TEMPLATE), TEMPLATE)
        self.assertIn('/Advanced Omarchy options', render(KERNELS, SETTINGS))

    def test_arbitrary_empty_or_damaged_configs_are_rejected(self):
        for original, template in [('', ''), ('# damaged\n', TEMPLATE),
                                   (TEMPLATE, None), (KERNELS, KERNELS),
                                   (TEMPLATE + '/foreign\n', TEMPLATE + '/foreign\n')]:
            with self.subTest(original=original), self.assertRaises(ValueError):
                rebind_machine(original, NEW, template)

    def test_ambiguous_group_or_identity_is_rejected(self):
        managed = render(KERNELS, SETTINGS)
        for original in [managed + '\n/Advanced Omarchy options\n',
                         managed.replace(f'machine-id={OLD}', 'missing-identity')]:
            with self.assertRaises(ValueError):
                rebind_machine(original, NEW, TEMPLATE)

    def test_kernel_generation_and_final_validation_cannot_accept_a_missing_menu(self):
        for caller in ('limine-entry-tool', 'limine-mkinitcpio-install', None):
            with self.assertRaises(ValueError):
                after_update(TEMPLATE, SETTINGS, TEMPLATE, caller)
        with self.assertRaises(ValueError):
            render(TEMPLATE, SETTINGS)


if __name__ == '__main__':
    unittest.main()
