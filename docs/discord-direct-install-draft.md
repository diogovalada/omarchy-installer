# Draft for omarchy-discussions

Status: prepared for review; not posted. The channel recommendation is based on
the visible channel names, not verified moderator guidance. Add a link to the
earlier project announcement if available.

## Suggested title

USB-free Omarchy installation from Windows: deploy a prepared system or boot a staged installer?

## Suggested post

I'm working on an unofficial community installer for Omarchy:
https://github.com/diogovalada/omarchy-installer

I previously shared the project in #omarchy. I'd appreciate feedback on the
Windows x86 direct-install architecture, especially from people familiar with
the installer, dual boot and BitLocker. The aim is to install alongside Windows
on the same disk without needing a USB drive.

I'm comparing two routes:

1. Prepare an installed Omarchy system, then have a Windows helper write it into
   the allocated Linux partitions. Reboot into Omarchy for final setup.
2. Stage a bootable Linux installer and its required files on internal storage,
   then reboot into it to install onto the allocated partitions.

Whether we build the system locally or download a published prepared image is a
separate choice. A published image could support either route.

There are several concerns to get right: preserving Windows and the installer
source, BitLocker recovery prompts and restoration of protection, Secure Boot,
fresh per-installation Linux encryption, and recovery after an interrupted install.
The stock installer's source-disk exclusion and its BitLocker refusal are separate
checks; simply removing them would not establish a safe installation path.

Which approach would you consider easier to support and maintain? Have you tried
something similar, or encountered failure cases that should shape the design?
Are there upstream plans or existing projects we should coordinate with?

This is still design and development work, not a request for people to try an
unverified same-disk installation on their machines.
