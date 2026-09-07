"""File-backed proof configuration; adapted from upstream (see NOTICE)."""

import uuid

MIB = 1024**2
GIB = 1024**3
DISK_BYTES = 40 * GIB


def configuration(*, deferred=False, hostname=None, encrypted=False):
    """No arbitrary target, encryption material, or custom installer commands."""
    hostname = hostname or "omarchy-proof-" + uuid.uuid4().hex[:8]
    def size(value):
        return {"sector_size": {"unit": "B", "value": 512}, "unit": "B", "value": value}

    def partition(start, length, fs, mount, flags, subvolumes):
        return {
            "btrfs": subvolumes, "dev_path": None, "flags": flags,
            "fs_type": fs, "mount_options": ["compress=zstd"] if fs == "btrfs" else [],
            "mountpoint": mount, "obj_id": str(uuid.uuid4()), "size": size(length),
            "start": size(start), "status": "create", "type": "primary",
        }

    result = {
        "app_config": None, "archinstall-language": "English", "auth_config": {},
        "audio_config": {"audio": "pipewire"},
        "bootloader_config": {"bootloader": "Limine", "uki": False, "removable": False},
        "custom_commands": [],
        "omarchy_install": {
            "mode": "full_disk", "defer_provisioning": deferred, "target_mount": "/mnt",
            "boot": {"esp_mount": "/boot", "esp_path": "/EFI/limine",
                     "efi_binary": "limine_x64.efi", "enable_fallback": True},
            "storage": {"kernel": "linux"},
        },
        "disk_config": {"config_type": "default_layout", "device_modifications": [{
            "device": "/dev/vda", "wipe": True, "partitions": [
                partition(MIB, 2 * GIB, "fat32", "/boot", ["boot", "esp"], []),
                partition(2 * GIB + MIB, DISK_BYTES - 2 * GIB - 2 * MIB,
                          "btrfs", None, [], [
                              {"mountpoint": "/", "name": "@"},
                              {"mountpoint": "/home", "name": "@home"},
                              {"mountpoint": "/var/log", "name": "@log"},
                              {"mountpoint": "/var/cache/pacman/pkg", "name": "@pkg"},
                          ]),
            ],
        }]},
        "hostname": hostname, "kernels": ["linux"], "network_config": {"type": "iso"},
        "ntp": True, "parallel_downloads": 4, "script": None, "services": [],
        "swap": True, "timezone": "UTC",
        "locale_config": {"kb_layout": "us", "sys_enc": "UTF-8", "sys_lang": "en_US.UTF-8"},
        "mirror_config": {
            "custom_repositories": [], "custom_servers": [
                {"url": "https://mirror.omarchy.org/$repo/os/$arch"},
                {"url": "https://mirror.rackspace.com/archlinux/$repo/os/$arch"},
                {"url": "https://geo.mirror.pkgbuild.com/$repo/os/$arch"},
            ], "mirror_regions": {}, "optional_repositories": [],
        },
        "packages": ["base-devel", "git", "omarchy-keyring", "omarchy-settings", "omarchy"],
        "profile_config": {"gfx_driver": None, "greeter": None, "profile": {}},
        "version": "3.0.9",
    }
    if encrypted:
        if not deferred:
            raise ValueError("The product encryption profile requires upstream deferred owner setup")
        root = result["disk_config"]["device_modifications"][0]["partitions"][1]
        # The pinned ISO's configurator emits this block. Its InstallContext
        # generates a fresh passphrase; no password crosses the Windows UI.
        result["disk_config"]["disk_encryption"] = {
            "encryption_type": "luks", "lvm_volumes": [], "iter_time": 2000,
            "partitions": [root["obj_id"]],
        }
    return result
