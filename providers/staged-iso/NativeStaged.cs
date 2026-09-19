// Staged-ISO primitives share the existing, narrow Windows disk/UEFI boundary.
using System;
using System.IO;
using System.Linq;
using System.Collections.Generic;
using System.Text;

namespace Omarchy.DirectX86 {
    public static partial class NativeDisk {
        [System.Runtime.InteropServices.DllImport("kernel32.dll", CharSet=System.Runtime.InteropServices.CharSet.Unicode, SetLastError=true)]
        static extern bool SetVolumeMountPoint(string path, string volume);
        [System.Runtime.InteropServices.DllImport("kernel32.dll", CharSet=System.Runtime.InteropServices.CharSet.Unicode, SetLastError=true)]
        static extern bool DeleteVolumeMountPoint(string path);
        [System.Runtime.InteropServices.DllImport("kernel32.dll", CharSet=System.Runtime.InteropServices.CharSet.Unicode, SetLastError=true)]
        static extern bool GetVolumeNameForVolumeMountPoint(string path, StringBuilder volume, int length);
        public static void MountStagingVolume(string path, string volume) {
            if (!System.Text.RegularExpressions.Regex.IsMatch(volume, @"^\\\\\?\\Volume\{[0-9a-fA-F-]{36}\}\\$") || !Directory.Exists(path)) throw new InvalidDataException("Invalid staging volume mount");
            Check(SetVolumeMountPoint(path.TrimEnd('\\') + "\\", volume), "Cannot mount owned staging volume");
            var actual = new StringBuilder(1024);
            Check(GetVolumeNameForVolumeMountPoint(path.TrimEnd('\\') + "\\", actual, actual.Capacity), "Cannot verify staging mount");
            if (!string.Equals(actual.ToString(), volume, StringComparison.OrdinalIgnoreCase)) throw new InvalidDataException("Staging mount identity changed");
        }
        public static void UnmountStagingVolume(string path, string volume) {
            var actual = new StringBuilder(1024);
            Check(GetVolumeNameForVolumeMountPoint(path.TrimEnd('\\') + "\\", actual, actual.Capacity), "Cannot identify staging mount");
            if (!string.Equals(actual.ToString(), volume, StringComparison.OrdinalIgnoreCase)) throw new InvalidDataException("Refusing to detach another volume");
            Check(DeleteVolumeMountPoint(path.TrimEnd('\\') + "\\"), "Cannot remove owned staging mount");
        }
        static readonly Guid StageDataType = new Guid("ebd0a0a2-b9e5-4433-87c0-68b6b72699c7");
        public static byte[] PlanStagingLayout(byte[] before, long start, long dataSize, Guid esp, Guid data) {
            const long espSize = 536870912;
            if (before.Length < Header || BitConverter.ToInt32(before, 0) != 1) throw new InvalidDataException("GPT required");
            int count = BitConverter.ToInt32(before, 4), maximum = BitConverter.ToInt32(before, 40);
            if (count < 0 || count > 4096 || before.Length != Header + count * Entry || maximum < count || maximum > 4096)
                throw new InvalidDataException("Invalid GPT inventory");
            if (esp == Guid.Empty || data == Guid.Empty || esp == data || dataSize < 1073741824 || dataSize > 68719476736L)
                throw new InvalidDataException("Invalid staging identity or size");
            ValidateRange(start, checked(espSize + dataSize));
            long end = checked(start + espSize + dataSize), first = BitConverter.ToInt64(before, 24);
            if (start < first || end > checked(first + BitConverter.ToInt64(before, 32))) throw new InvalidDataException("Staging exceeds GPT bounds");
            var slots = new List<int>(); var numbers = new HashSet<int>();
            for (int i = 0; i < count; i++) {
                int p = Header + i * Entry; long a = BitConverter.ToInt64(before, p + 8), size = BitConverter.ToInt64(before, p + 16);
                if (size == 0) { slots.Add(i); continue; }
                if (size < 0 || (start < checked(a + size) && a < end)) throw new InvalidDataException("Staging overlaps existing data");
                Guid id = GuidAt(before, p + 48);
                if (id == esp || id == data) throw new InvalidDataException("Staging GUID already exists");
                numbers.Add(BitConverter.ToInt32(before, p + 24));
            }
            while (slots.Count < 2) { if (count >= maximum) throw new InvalidDataException("Two free GPT entries required"); slots.Add(count++); }
            var result = new byte[Header + count * Entry]; Buffer.BlockCopy(before, 0, result, 0, before.Length);
            Put(result, 4, BitConverter.GetBytes(count));
            for (int i = 0; i < count; i++) result[Header + i * Entry + 28] = 0;
            for (int i = 0; i < 2; i++) {
                int p = Header + slots[i] * Entry, number = 1; while (numbers.Contains(number)) number++; numbers.Add(number);
                Array.Clear(result, p, Entry); Put(result, p, BitConverter.GetBytes(1));
                Put(result, p + 8, BitConverter.GetBytes(i == 0 ? start : start + espSize));
                Put(result, p + 16, BitConverter.GetBytes(i == 0 ? espSize : dataSize));
                Put(result, p + 24, BitConverter.GetBytes(number)); result[p + 28] = 1;
                Put(result, p + 32, (i == 0 ? EspType : StageDataType).ToByteArray());
                Put(result, p + 48, (i == 0 ? esp : data).ToByteArray());
                Put(result, p + 64, BitConverter.GetBytes(0x8000000000000000UL));
                Put(result, p + 72, Encoding.Unicode.GetBytes(i == 0 ? "Omarchy installer EFI" : "Omarchy installer source"));
            }
            return result;
        }
        public static int[] AllocateStaging(int disk, string expectedHash, long start, long dataSize, Guid esp, Guid data) {
            using (var h = OpenDisk(disk, true)) {
                byte[] before = Layout(h);
                if (Hash(before) != expectedHash) throw new InvalidDataException("GPT changed before staging allocation");
                byte[] next = PlanStagingLayout(before, start, dataSize, esp, data); int returned;
                Check(DeviceIoControl(h, SET_LAYOUT, next, next.Length, null, 0, out returned, IntPtr.Zero), "Cannot allocate staging partitions");
                Check(FlushFileBuffers(h), "Cannot flush staging GPT");
                Check(DeviceIoControl(h, UPDATE_PROPERTIES, null, 0, null, 0, out returned, IntPtr.Zero), "Cannot refresh staging partitions");
                byte[] after = Layout(h);
                if (!before.Skip(8).Take(Header - 8).SequenceEqual(after.Skip(8).Take(Header - 8))) throw new InvalidDataException("Staging changed GPT disk identity or geometry");
                for (int i = 0; i < BitConverter.ToInt32(before, 4); i++) {
                    int p = Header + i * Entry; if (BitConverter.ToInt64(before, p + 16) == 0) continue;
                    int q = FindEntry(after, GuidAt(before, p + 48));
                    if (!before.Skip(p).Take(28).SequenceEqual(after.Skip(q).Take(28)) || !before.Skip(p + 32).Take(112).SequenceEqual(after.Skip(q + 32).Take(112)))
                        throw new InvalidDataException("Existing partition metadata changed");
                }
                return new[] { FindPartition(after, esp, start, 536870912, EspType), FindPartition(after, data, start + 536870912, dataSize, StageDataType) };
            }
        }
        static int FindEntry(byte[] layout, Guid id) {
            for (int i = 0; i < BitConverter.ToInt32(layout, 4); i++) { int p = Header + i * Entry; if (GuidAt(layout, p + 48) == id) return p; }
            throw new InvalidDataException("Recorded partition missing");
        }
        public static byte[] PlanStagingDeletion(byte[] before, int number, Guid id, long offset, long size, Guid type) {
            if (type != EspType && type != StageDataType) throw new InvalidDataException("Not a staging partition type");
            if (FindPartition(before, id, offset, size, type) != number) throw new InvalidDataException("Owned deletion target changed");
            byte[] next = (byte[])before.Clone();
            for (int i = 0; i < BitConverter.ToInt32(next, 4); i++) next[Header + i * Entry + 28] = 0;
            int p = FindEntry(next, id); Array.Clear(next, p, Entry); Put(next, p, BitConverter.GetBytes(1)); next[p + 28] = 1;
            VerifyOnlyDelete(before, next, id, offset, size);
            return next;
        }
        public static void DeleteStagingPartition(int disk, int number, Guid id, long offset, long size, Guid type, string volume, string expectedLayout, bool raw) {
            if (!System.Text.RegularExpressions.Regex.IsMatch(volume, @"^\\\\\?\\Volume\{[0-9a-fA-F-]{36}\}\\$")) throw new InvalidDataException("Invalid owned volume path");
            using (var vh = CreateFile(volume.TrimEnd('\\'), READ | WRITE, 3, IntPtr.Zero, 3, 0, IntPtr.Zero)) {
                if (vh.IsInvalid) throw new IOException("Cannot open owned staging volume");
                int returned; var extents = new byte[1024];
                Check(DeviceIoControl(vh, 0x00560000, null, 0, extents, extents.Length, out returned, IntPtr.Zero), "Cannot verify staging volume extents");
                if (returned < 32 || BitConverter.ToInt32(extents, 0) != 1 || BitConverter.ToInt32(extents, 8) != disk || BitConverter.ToInt64(extents, 16) != offset || BitConverter.ToInt64(extents, 24) != size) throw new InvalidDataException("Owned volume extent changed");
                bool locked = DeviceIoControl(vh, 0x00090018, null, 0, null, 0, out returned, IntPtr.Zero);
                int error = System.Runtime.InteropServices.Marshal.GetLastWin32Error();
                if (!locked && !(raw && (error == 1 || error == 50 || error == 1005))) throw new IOException("Staging volume is in use; close its files before cleanup");
                if (locked) Check(DeviceIoControl(vh, 0x00090020, null, 0, null, 0, out returned, IntPtr.Zero), "Cannot dismount staging volume");
                using (var h = OpenDisk(disk, true)) {
                    byte[] before = Layout(h);
                    if (Hash(before) != expectedLayout) throw new InvalidDataException("Disk changed immediately before owned cleanup");
                    byte[] next = PlanStagingDeletion(before, number, id, offset, size, type);
                    Check(DeviceIoControl(h, SET_LAYOUT, next, next.Length, null, 0, out returned, IntPtr.Zero), "Cannot remove owned staging partition");
                    Check(FlushFileBuffers(h), "Cannot flush staging removal");
                    Check(DeviceIoControl(h, UPDATE_PROPERTIES, null, 0, null, 0, out returned, IntPtr.Zero), "Cannot refresh owned cleanup");
                    VerifyOnlyDelete(before, Layout(h), id, offset, size);
                }
            }
        }
        public static byte[] StagingBootOption(int partition, long start, Guid esp) {
            if (partition < 1 || esp == Guid.Empty) throw new InvalidDataException("Invalid staging EFI target");
            ValidateRange(start, 536870912);
            byte[] path = Encoding.Unicode.GetBytes("\\EFI\\BOOT\\BOOTX64.EFI\0");
            var dp = new byte[42 + 4 + path.Length + 4]; dp[0] = 4; dp[1] = 1;
            Put(dp, 2, BitConverter.GetBytes((ushort)42)); Put(dp, 4, BitConverter.GetBytes(partition));
            Put(dp, 8, BitConverter.GetBytes(start / 512)); Put(dp, 16, BitConverter.GetBytes(536870912L / 512));
            Put(dp, 24, esp.ToByteArray()); dp[40] = 2; dp[41] = 2; dp[42] = 4; dp[43] = 4;
            Put(dp, 44, BitConverter.GetBytes((ushort)(4 + path.Length))); Put(dp, 46, path);
            int end = dp.Length - 4; dp[end] = 127; dp[end + 1] = 255; dp[end + 2] = 4;
            byte[] desc = Encoding.Unicode.GetBytes("Omarchy temporary installer\0"), result = new byte[6 + desc.Length + dp.Length];
            Put(result, 0, BitConverter.GetBytes(1)); Put(result, 4, BitConverter.GetBytes((ushort)dp.Length));
            Put(result, 6, desc); Put(result, 6 + desc.Length, dp); return result;
        }
        public static string ReserveStagingBootName() {
            FirmwarePrivilege();
            for (int i = 0; i <= 65535; i++) { string name = "Boot" + i.ToString("X4"); if (GetVariable(name) == null) return name; }
            throw new InvalidOperationException("No free firmware entry");
        }
        static ushort StagingIndex(string name) {
            if (name == null || !System.Text.RegularExpressions.Regex.IsMatch(name, "^Boot[0-9A-F]{4}$")) throw new InvalidDataException("Invalid firmware entry name");
            return Convert.ToUInt16(name.Substring(4), 16);
        }
        public static void RegisterStagingBoot(string name, byte[] option, string expectedOrder) {
            StagingIndex(name); FirmwarePrivilege();
            if (GetVariable(name) != null || FirmwarePreflight() != expectedOrder) throw new InvalidDataException("Firmware changed before staging registration");
            Check(SetFirmwareEnvironmentVariableEx(name, EfiGlobal, option, (uint)option.Length, 7), "Cannot register temporary installer");
            if (!option.SequenceEqual(GetVariable(name) ?? new byte[0]) || FirmwarePreflight() != expectedOrder) throw new InvalidDataException("Temporary entry readback failed; boot order was not changed by the installer");
        }
        public static void ArmStagingBoot(string name, byte[] option) {
            ushort index = StagingIndex(name); FirmwarePrivilege();
            if (!option.SequenceEqual(GetVariable(name) ?? new byte[0]) || GetVariable("BootNext") != null) throw new InvalidDataException("Firmware entry changed or another one-time boot is pending");
            byte[] next = BitConverter.GetBytes(index);
            Check(SetFirmwareEnvironmentVariableEx("BootNext", EfiGlobal, next, 2, 7), "Cannot select next installer boot");
            if (!next.SequenceEqual(GetVariable("BootNext") ?? new byte[0])) throw new InvalidDataException("One-time boot readback failed");
        }
        public static void RemoveStagingBoot(string name, byte[] option) {
            ushort index = StagingIndex(name); FirmwarePrivilege();
            byte[] current = GetVariable("BootCurrent"), existing = GetVariable(name);
            if (current == null || current.Length != 2 || BitConverter.ToUInt16(current, 0) == index) throw new InvalidDataException("Cannot remove the current boot source");
            if (existing != null && !option.SequenceEqual(existing)) throw new InvalidDataException("Temporary firmware slot has been reused");
            byte[] next = GetVariable("BootNext");
            if (next != null && next.Length == 2 && BitConverter.ToUInt16(next, 0) == index)
                Check(SetFirmwareEnvironmentVariableEx("BootNext", EfiGlobal, new byte[0], 0, 7), "Cannot clear owned one-time boot");
            byte[] order = GetVariable("BootOrder");
            if (order == null || order.Length % 2 != 0) throw new InvalidDataException("Unknown firmware order");
            var kept = new List<byte>();
            for (int i = 0; i < order.Length; i += 2) if (BitConverter.ToUInt16(order, i) != index) { kept.Add(order[i]); kept.Add(order[i + 1]); }
            if (kept.Count != order.Length) {
                if (kept.Count == 0) throw new InvalidDataException("Cannot remove the only boot option");
                Check(SetFirmwareEnvironmentVariableEx("BootOrder", EfiGlobal, kept.ToArray(), (uint)kept.Count, 7), "Cannot remove owned boot order reference");
                if (!kept.SequenceEqual(GetVariable("BootOrder") ?? new byte[0])) throw new InvalidDataException("Boot cleanup readback failed");
            }
            if (existing != null) Check(SetFirmwareEnvironmentVariableEx(name, EfiGlobal, new byte[0], 0, 7), "Cannot remove temporary firmware entry");
            if (GetVariable(name) != null) throw new InvalidDataException("Temporary entry remains");
        }
    }
}
