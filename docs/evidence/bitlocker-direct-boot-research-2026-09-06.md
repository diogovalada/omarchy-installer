# BitLocker and the direct-install OS menu: specification findings

**Implementation follow-up:** the [Windows completion pass](windows-direct-readiness-2026-09-06.md)
adds protector/PCR inspection, current firmware/TCG boot-route validation and a
separate Secure Boot preparation action. The research below records the
requirements that motivated those changes. Machine qualification remains pending.

Date: 2026-09-06. Research and source inspection only; no host TPM/BitLocker
queries, installation, firmware changes, reboot or execution tests.

## Conclusion

An extra Windows boot solely to accommodate adding our menu is **not an inherent
requirement** for the defined compatible case: native UEFI, Secure Boot already
disabled and unchanged, an established direct Windows firmware boot, a supported
default TPM validation profile, and unchanged Windows boot files/BCD. Our
restart-to-Windows design preserves the relevant boot route in that case.
Suspension, deployment/menu registration and restoration in the existing Windows
session is therefore a specification-supported design, not an unresolved question
about whether any such design is possible. Hardware qualification remains needed.

This conclusion does not extend automatically to changing Secure Boot, custom
PCR profiles, existing third-party Windows boot chains, pending boot/firmware
updates, or firmware that does not implement the required reset/boot behavior.

## Source facts

- TCG PFP 1.06 r52 assigns BootOrder and Boot#### measurements to PCR 1
  (§3.3.4.2); GPT/configuration to PCR 5 (§3.3.4.6); executed boot applications
  and attempts to PCR 4 (§3.3.4.5). Warm boots reset the TPM and start a fresh
  measurement sequence (§2.2.7). This reset does not mean clearing ownership or
  persistent TPM keys. [TCG specification](https://trustedcomputinggroup.org/wp-content/uploads/TCG-PC-Client-Platform-Firmware-Profile-Version-1.06-Revision-52_pub-1.pdf)
- Microsoft's native-UEFI defaults are PCR 0, 2, 4, 11, or PCR 7, 11 when using
  supported Secure Boot validation. Neither includes PCR 1 or 5. The actual
  protector can have a custom profile. [Microsoft profiles](https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/configure#configure-tpm-platform-validation-profile-for-native-uefi-firmware-configurations)
- UEFI processes BootNext before the ordinary BootOrder and removes BootNext
  before launching its selected option. [UEFI boot manager](https://uefi.org/specs/UEFI/2.11/03_Boot_Manager.html)
- Limine v12.6.0 `efi_boot_entry` resolves the configured firmware entry, writes
  BootNext, and calls ResetSystem with EfiResetWarm. It does not chainload
  Windows within the current menu boot. [Versioned implementation](https://github.com/Limine-Bootloader/Limine/blob/v12.6.0/common/protos/efi_boot_entry.c)
- EnableKeyProtectors refreshes TPM protectors against the current startup state.
  This supports immediate restoration when relevant future measurements remain
  equivalent; it is not evidence that arbitrary future firmware changes are
  preapproved. [Microsoft API](https://learn.microsoft.com/en-us/windows/win32/secprov/enablekeyprotectors-win32-encryptablevolume#remarks)

## Application to our code

`NativeDisk.RegisterBoot` creates a separate Omarchy boot option and prepends it
to BootOrder, preserving the original Windows entry. `boot_menu.py` selects that
Windows entry through `efi_boot_entry`. Omarchy has a separate ESP; deployment
does not replace Windows Boot Manager or edit its BCD.

The inferred compatible sequence is: menu boots, Windows is selected, firmware
restarts, then the original Windows option boots directly. Linux menu execution
belongs to the preceding boot. Keeping the existing Windows Boot#### identifier
also preserves the boot-option identity in firmware measurements.

Therefore our own boot-order/partition changes need not change a default
protector's validated values. It is unnecessary to claim that every user must
return to Windows once before protection can be restored. Conversely, custom
profiles covering changed state require a separate transition policy; resuming
in the original session alone does not establish that policy.

## Concrete exceptions and missing preflight

1. **Secure Boot transition.** The current provider requires it already disabled
   and rejects enabled/unknown states. Changing enabled to disabled affects
   Windows' security environment and requires firmware interaction/reboot. The
   app currently does not coordinate that transition before its prerequisite
   message. It must not advertise a universal single-session installation to
   Secure-Boot-enabled Windows users. A supported signed Linux boot chain would
   be a different design, not current functionality.
2. **Actual protector profile.** Current BitLocker inventory records status and
   protector IDs, but does not read protector types or PCR profiles. Add typed
   preflight using GetKeyProtectorType and GetKeyProtectorPlatformValidationProfile;
   evaluate all relevant protectors without weakening their policy. Unknown or
   custom profiles cannot inherit the default-profile conclusion.
   [Profile inspection API](https://learn.microsoft.com/en-us/windows/win32/secprov/getkeyprotectorplatformvalidationprofile-win32-encryptablevolume)
3. **Existing boot route.** The provider verifies a Windows firmware entry but
   does not establish that the current Windows session used it directly. An
   existing third-party chain can yield a different baseline. BootCurrent and
   measured-boot evidence should inform eligibility; entry existence is not
   sufficient. Windows exposes its boot log via
   [Tbsi_Get_TCG_Log_Ex](https://learn.microsoft.com/en-us/windows/win32/api/tbs/nf-tbs-tbsi_get_tcg_log_ex).
4. **Concurrent changes and firmware behavior.** Preserve Windows files, BCD,
   firmware policy and the observed route through installation. Qualify cold
   startup and the menu's Windows restart on supported firmware. Testing now has
   a defined prediction to verify; it is not a substitute for researching the
   mechanism. Other future changes can still independently trigger recovery.

The separate [deferred-setup hook defect](deferred-setup-compatibility-2026-09-06.md)
remains unfixed. This research changes neither provider behavior nor the user's
deferred-testing boundary.
