// Narrow Windows disk/UEFI primitives. Loaded only from the packaged provider.
// No executable paths, shell commands, downloads, or generic write ranges.
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using Microsoft.Win32.SafeHandles;

namespace Omarchy.DirectX86 {
    public static class NativeDisk {
        const uint READ = 0x80000000, WRITE = 0x40000000;
        const uint GET_LAYOUT = 0x00070050, SET_LAYOUT = 0x0007C054;
        const uint UPDATE_PROPERTIES = 0x00070140;
        const string EfiGlobal = "{8BE4DF61-93CA-11D2-AA0D-00E098032B8C}";
        const int Header = 48, Entry = 144;
        static readonly Guid EspType = new Guid("c12a7328-f81f-11d2-ba4b-00a0c93ec93b");
        static readonly Guid LinuxType = new Guid("0fc63daf-8483-4772-8e79-3d69d8477de4");
        [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
        static extern SafeFileHandle CreateFile(string n, uint a, uint s, IntPtr sa, uint c, uint f, IntPtr t);
        [DllImport("kernel32.dll", SetLastError=true)]
        static extern bool DeviceIoControl(SafeFileHandle h, uint c, byte[] i, int il, byte[] o, int ol, out int b, IntPtr ov);
        [DllImport("kernel32.dll", SetLastError=true)]
        static extern bool FlushFileBuffers(SafeFileHandle h);
        [DllImport("kernel32.dll", SetLastError=true)]
        static extern bool GetFirmwareType(out uint kind);
        [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
        static extern uint GetFinalPathNameByHandle(SafeFileHandle h, StringBuilder path, uint length, uint flags);
        [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
        static extern uint GetFirmwareEnvironmentVariable(string n, string g, byte[] b, uint s);
        [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
        static extern bool SetFirmwareEnvironmentVariableEx(string n, string g, byte[] b, uint s, uint a);
        [DllImport("advapi32.dll", SetLastError=true)]
        static extern bool OpenProcessToken(IntPtr p, uint a, out SafeFileHandle token);
        [DllImport("advapi32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
        static extern bool LookupPrivilegeValue(string s, string n, out long luid);
        [DllImport("advapi32.dll", SetLastError=true)]
        static extern bool AdjustTokenPrivileges(SafeFileHandle t, bool d, ref Privileges p, uint l, IntPtr prev, IntPtr req);
        // TOKEN_PRIVILEGES contains a DWORD followed by LUID_AND_ATTRIBUTES.
        // LUID is two DWORDs (4-byte alignment), even in a 64-bit process.
        [StructLayout(LayoutKind.Sequential, Pack=4)] struct Privileges { public uint Count; public long Luid; public uint Attributes; }

        static void Check(bool ok, string message) {
            if (!ok) throw new Win32Exception(Marshal.GetLastWin32Error(), message);
        }
        static SafeFileHandle OpenDisk(int number, bool writable) {
            if (number < 0 || number > 4095) throw new ArgumentOutOfRangeException("number");
            var h = CreateFile(@"\\.\PhysicalDrive" + number, writable ? READ | WRITE : 0,
                               3, IntPtr.Zero, 3, writable ? 0x80000000u : 0u, IntPtr.Zero);
            if (h.IsInvalid) { h.Dispose(); throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot open selected physical disk"); }
            return h;
        }
        static byte[] Layout(SafeFileHandle h) {
            var bytes = new byte[1024 * 1024]; int returned;
            Check(DeviceIoControl(h, GET_LAYOUT, null, 0, bytes, bytes.Length, out returned, IntPtr.Zero), "Cannot read GPT layout");
            if (returned < Header || BitConverter.ToUInt32(bytes, 0) != 1) throw new InvalidDataException("Only GPT disks are supported");
            uint count = BitConverter.ToUInt32(bytes, 4);
            if (count > 4096 || returned < Header + Entry * count) throw new InvalidDataException("Invalid native partition table");
            Array.Resize(ref bytes, Header + Entry * (int)count);
            return bytes;
        }
        public static string Firmware() { uint f; return GetFirmwareType(out f) ? (f == 2 ? "uefi" : "bios") : "unknown"; }
        public static string Hash(byte[] value) { using (var sha = SHA256.Create()) return BitConverter.ToString(sha.ComputeHash(value)).Replace("-", "").ToLowerInvariant(); }
        public static byte[] ReadLayout(int disk) { using (var h = OpenDisk(disk, false)) return Layout(h); }
        public static string LayoutHash(int disk) { return Hash(ReadLayout(disk)); }
        public static string VolumeForPath(string path) {
            string current = Path.GetFullPath(Environment.ExpandEnvironmentVariables(path));
            while (!File.Exists(current) && !Directory.Exists(current)) {
                current = Path.GetDirectoryName(current);
                if (String.IsNullOrEmpty(current)) throw new InvalidDataException("Cannot identify the volume of a protected path");
            }
            using (var h = CreateFile(current, 0x80, 7, IntPtr.Zero, 3, 0x02000000, IntPtr.Zero)) {
                if (h.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot resolve a protected path");
                var name = new StringBuilder(32768);
                uint length = GetFinalPathNameByHandle(h, name, (uint)name.Capacity, 1);
                if (length == 0 || length >= name.Capacity) throw new InvalidDataException("Cannot resolve a protected volume identity");
                string resolved = name.ToString();
                if (!resolved.StartsWith(@"\\?\Volume{", StringComparison.OrdinalIgnoreCase)) throw new InvalidDataException("Protected path is not on an identified local volume");
                int end = resolved.IndexOf('}'); Guid id;
                if (end < 0 || !Guid.TryParse(resolved.Substring(11, end - 11), out id)) throw new InvalidDataException("Invalid protected volume identity");
                return @"\\?\Volume{" + id.ToString() + @"}\";
            }
        }
        public static string ReadDeletionPartitionName(int disk, int number, Guid id, long offset, long size) {
            byte[] layout = ReadLayout(disk);
            int count = (int)BitConverter.ToUInt32(layout, 4); string name = null;
            int occupied = Enumerable.Range(0, count).Count(i => BitConverter.ToInt64(layout, Header + i * Entry + 16) > 0);
            if ((long)BitConverter.ToUInt32(layout, 40) - occupied + 1 < 2) throw new InvalidDataException("Deleting one partition would not leave two GPT entries for Omarchy");
            for (int i = 0; i < count; i++) {
                int p = Header + i * Entry;
                if (GuidAt(layout, p + 48) != id) continue;
                Guid type = GuidAt(layout, p + 32);
                if (name != null || id == Guid.Empty || BitConverter.ToInt32(layout, p + 24) != number || BitConverter.ToInt64(layout, p + 8) != offset || BitConverter.ToInt64(layout, p + 16) != size) throw new InvalidDataException("Deletion partition identity is ambiguous or changed");
                if (type != LinuxType && type != new Guid("ebd0a0a2-b9e5-4433-87c0-68b6b72699c7")) throw new InvalidDataException("Protected partition type cannot be deleted");
                if ((BitConverter.ToUInt64(layout, p + 64) & ~0x8000000000000000UL) != 0) throw new InvalidDataException("Required, bootable or otherwise protected GPT attributes prevent deletion");
                name = Encoding.Unicode.GetString(layout, p + 72, 72).TrimEnd('\0');
            }
            if (name == null) throw new InvalidDataException("Deletion partition was not found");
            return name;
        }
        public static string InspectLinuxFilesystem(int disk, int number, Guid id, long offset, long size) {
            if (FindPartition(ReadLayout(disk), id, offset, size, LinuxType) != number)
                throw new InvalidDataException("Linux partition identity changed");
            string path = @"\\?\GLOBALROOT\Device\Harddisk" + disk + @"\Partition" + number;
            using (var h = CreateFile(path, READ, 3, IntPtr.Zero, 3, 0, IntPtr.Zero)) {
                if (h.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot inspect the Linux partition");
                using (var stream = new FileStream(h, FileAccess.Read)) {
                    byte[] data = new byte[69632]; int done = 0;
                    while (done < data.Length) { int n = stream.Read(data, done, data.Length - done); if (n == 0) break; done += n; }
                    if (done >= 6 && data[0] == 0x4c && data[1] == 0x55 && data[2] == 0x4b && data[3] == 0x53 && data[4] == 0xba && data[5] == 0xbe) return "LUKS encrypted";
                    if (done >= 65680 && Encoding.ASCII.GetString(data, 65536 + 64, 8) == "_BHRfS_M")
                        return BitConverter.ToUInt64(data, 65536 + 136) == 1 ? "Btrfs" : "Btrfs (multiple devices)";
                    if (done >= 2048 && BitConverter.ToUInt16(data, 1024 + 56) == 0xef53) {
                        uint incompat = BitConverter.ToUInt32(data, 1024 + 96);
                        if ((incompat & 8) != 0 || BitConverter.ToUInt32(data, 1024 + 228) != 0 || data.Skip(1024 + 208).Take(16).Any(b => b != 0)) return "ext (external journal)";
                        return (incompat & 0x40) != 0 ? "ext4" : "ext2/ext3";
                    }
                    return "Unknown";
                }
            }
        }
        public static SafeFileHandle LockDeletionTarget(int disk, int number, Guid id, long offset, long size, string volumeId) {
            byte[] layout = ReadLayout(disk);
            Guid type = String.IsNullOrEmpty(volumeId) ? LinuxType : new Guid("ebd0a0a2-b9e5-4433-87c0-68b6b72699c7");
            if (FindPartition(layout, id, offset, size, type) != number) throw new InvalidDataException("Deletion target changed before locking");
            bool windowsVolume = !String.IsNullOrEmpty(volumeId);
            if (windowsVolume && !System.Text.RegularExpressions.Regex.IsMatch(volumeId, @"^\\\\\?\\Volume\{[0-9a-fA-F-]{36}\}\\$")) throw new InvalidDataException("Invalid deletion volume path");
            string path = windowsVolume ? volumeId.TrimEnd('\\') : @"\\?\GLOBALROOT\Device\Harddisk" + disk + @"\Partition" + number;
            var h = CreateFile(path, READ | WRITE, windowsVolume ? 3u : 0u, IntPtr.Zero, 3, 0, IntPtr.Zero);
            try {
                if (h.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error(), "Partition is in use or cannot be locked for deletion");
                int returned;
                if (windowsVolume) {
                    var extents = new byte[1024];
                    Check(DeviceIoControl(h, 0x00560000, null, 0, extents, extents.Length, out returned, IntPtr.Zero), "Cannot verify deletion volume extents");
                    if (returned < 32 || BitConverter.ToInt32(extents, 0) != 1 || BitConverter.ToInt32(extents, 8) != disk || BitConverter.ToInt64(extents, 16) != offset || BitConverter.ToInt64(extents, 24) != size) throw new InvalidDataException("Deletion volume is not exactly the confirmed partition");
                    Check(DeviceIoControl(h, 0x00090018, null, 0, null, 0, out returned, IntPtr.Zero), "Partition is in use; close its files and applications before retrying");
                } else {
                    bool mounted = DeviceIoControl(h, 0x00090028, null, 0, null, 0, out returned, IntPtr.Zero);
                    int error = mounted ? 0 : Marshal.GetLastWin32Error();
                    if (mounted || (error != 1 && error != 50 && error != 1005 && error != 21)) throw new InvalidDataException("Linux partition mount state is not safely known; unmount it before retrying");
                }
                return h;
            } catch { h.Dispose(); throw; }
        }
        public static string VerifyOnlyDelete(byte[] before, byte[] after, Guid partition, long offset, long size) {
            if (before.Length < Header || after.Length < Header || !before.Take(4).SequenceEqual(after.Take(4)) || !before.Skip(8).Take(Header - 8).SequenceEqual(after.Skip(8).Take(Header - 8))) throw new InvalidDataException("Deletion changed disk geometry or GPT identity");
            var expected = new Dictionary<Guid, byte[]>(); bool found = false;
            foreach (var layout in new[] { before, after }) {
                uint count = BitConverter.ToUInt32(layout, 4);
                if (count > 4096 || layout.Length != Header + count * Entry) throw new InvalidDataException("Invalid deletion layout readback");
                for (int i = 0; i < count; i++) {
                    int p = Header + i * Entry;
                    if (BitConverter.ToInt64(layout, p + 16) == 0) continue;
                    Guid id = GuidAt(layout, p + 48);
                    var entry = layout.Skip(p).Take(Entry).ToArray(); entry[28] = 0;
                    if (Object.ReferenceEquals(layout, before)) {
                        if (id == partition) {
                            if (found || BitConverter.ToInt64(layout, p + 8) != offset || BitConverter.ToInt64(layout, p + 16) != size) throw new InvalidDataException("Deletion source identity or extent changed");
                            found = true;
                        } else { if (id == Guid.Empty || expected.ContainsKey(id)) throw new InvalidDataException("Ambiguous saved partition identity"); expected.Add(id, entry); }
                    } else {
                        byte[] original;
                        if (id == partition || !expected.TryGetValue(id, out original) || !original.SequenceEqual(entry)) throw new InvalidDataException("Deletion changed another partition or added an unexpected one");
                        expected.Remove(id);
                    }
                }
            }
            if (!found || expected.Count != 0) throw new InvalidDataException("Deletion removed more than the confirmed partition");
            return Hash(after);
        }
        public static string VerifyOnlyShrink(byte[] before, byte[] after, Guid partition, long offset, long oldSize, long newSize) {
            if (before.Length != after.Length || before.Length < Header || newSize <= 0 || newSize >= oldSize)
                throw new InvalidDataException("Unexpected layout shape or shrink size");
            byte[] expected = (byte[])before.Clone();
            int count = (int)BitConverter.ToUInt32(before, 4); bool found = false;
            if (count < 0 || before.Length != Header + count * Entry) throw new InvalidDataException("Invalid saved GPT layout");
            for (int i = 0; i < count; i++) {
                int p = Header + i * Entry;
                // RewritePartition is an instruction/status bit, not GPT data.
                expected[p + 28] = after[p + 28];
                if (GuidAt(before, p + 48) != partition) continue;
                if (found || BitConverter.ToInt64(before, p + 8) != offset || BitConverter.ToInt64(before, p + 16) != oldSize)
                    throw new InvalidDataException("Shrink source identity or extent changed");
                found = true; Put(expected, p + 16, BitConverter.GetBytes(newSize));
            }
            if (!found || !expected.SequenceEqual(after)) throw new InvalidDataException("Windows resize changed more than the authorized NTFS partition length");
            return Hash(after);
        }
        static Guid GuidAt(byte[] b, int offset) { var g = new byte[16]; Buffer.BlockCopy(b, offset, g, 0, 16); return new Guid(g); }
        static void Put(byte[] b, int p, byte[] v) { Buffer.BlockCopy(v, 0, b, p, v.Length); }
        static void ValidateRange(long start, long length) {
            if (start < 1048576 || start % 1048576 != 0 || length <= 0 || length % 1048576 != 0)
                throw new InvalidDataException("Partition extent must have exact MiB alignment");
            checked { long end = start + length; if (end <= start) throw new InvalidDataException("Partition overflow"); }
        }
        static int FindPartition(byte[] layout, Guid id, long start, long length, Guid type) {
            int count = (int)BitConverter.ToUInt32(layout, 4);
            for (int i = 0; i < count; i++) {
                int p = Header + i * Entry;
                if (GuidAt(layout, p + 48) == id) {
                    if (BitConverter.ToInt64(layout, p + 8) != start || BitConverter.ToInt64(layout, p + 16) != length || GuidAt(layout, p + 32) != type)
                        throw new InvalidDataException("Partition identity was reused for a different extent");
                    return (int)BitConverter.ToUInt32(layout, p + 24);
                }
            }
            throw new InvalidDataException("Created partition missing from refreshed GPT");
        }
        public static int[] Allocate(int disk, string expectedHash, long start, long rootSize, Guid espId, Guid rootId) {
            const long espSize = 2147483648L;
            if (rootSize < 40800092160L) throw new InvalidDataException("Target root partition is smaller than the constructed base image");
            ValidateRange(start, espSize + rootSize);
            if (espId == Guid.Empty || rootId == Guid.Empty || espId == rootId) throw new InvalidDataException("Unique partition IDs required");
            using (var h = OpenDisk(disk, true)) {
                byte[] before = Layout(h);
                if (Hash(before) != expectedHash) throw new InvalidDataException("Disk layout changed after confirmation");
                long first = BitConverter.ToInt64(before, 24), usable = BitConverter.ToInt64(before, 32);
                if (start < first || checked(start + espSize + rootSize) > checked(first + usable))
                    throw new InvalidDataException("New partitions exceed GPT usable bounds");
                int count = (int)BitConverter.ToUInt32(before, 4);
                int maximum = (int)BitConverter.ToUInt32(before, 40);
                var free = new List<int>();
                var usedNumbers = new HashSet<int>();
                for (int i = 0; i < count; i++) {
                    int p = Header + i * Entry;
                    long a = BitConverter.ToInt64(before, p + 8), n = BitConverter.ToInt64(before, p + 16);
                    if (n == 0) { free.Add(i); continue; }
                    usedNumbers.Add((int)BitConverter.ToUInt32(before, p + 24));
                    if (start < checked(a + n) && a < checked(start + espSize + rootSize)) throw new InvalidDataException("New partitions overlap existing data");
                    if (GuidAt(before, p + 48) == espId || GuidAt(before, p + 48) == rootId) throw new InvalidDataException("Partition GUID already exists");
                }
                while (free.Count < 2) { if (count >= maximum) throw new InvalidDataException("GPT has no two free entries"); free.Add(count++); }
                var updated = new byte[Header + count * Entry]; Buffer.BlockCopy(before, 0, updated, 0, before.Length);
                Put(updated, 4, BitConverter.GetBytes(count));
                for (int i = 0; i < count; i++) updated[Header + i * Entry + 28] = 0;
                Guid[] types = { EspType, LinuxType }, ids = { espId, rootId };
                long[] starts = { start, start + espSize }, sizes = { espSize, rootSize };
                for (int i = 0; i < 2; i++) {
                    int p = Header + free[i] * Entry;
                    Array.Clear(updated, p, Entry);
                    Put(updated, p, BitConverter.GetBytes(1));
                    Put(updated, p + 8, BitConverter.GetBytes(starts[i]));
                    Put(updated, p + 16, BitConverter.GetBytes(sizes[i]));
                    int partitionNumber = 1;
                    while (usedNumbers.Contains(partitionNumber)) partitionNumber++;
                    usedNumbers.Add(partitionNumber);
                    Put(updated, p + 24, BitConverter.GetBytes(partitionNumber)); updated[p + 28] = 1;
                    Put(updated, p + 32, types[i].ToByteArray()); Put(updated, p + 48, ids[i].ToByteArray());
                    Put(updated, p + 64, BitConverter.GetBytes(0x8000000000000000UL)); // no automatic drive letter
                    Put(updated, p + 72, Encoding.Unicode.GetBytes(i == 0 ? "Omarchy EFI" : "Omarchy Linux"));
                }
                // Kernel storage provider serializes the layout request. Existing
                // partition entries are passed intact with RewritePartition=false.
                int returned;
                Check(DeviceIoControl(h, SET_LAYOUT, updated, updated.Length, null, 0, out returned, IntPtr.Zero), "Could not allocate exact alongside GPT partitions");
                Check(FlushFileBuffers(h), "Could not flush GPT allocation");
                Check(DeviceIoControl(h, UPDATE_PROPERTIES, null, 0, null, 0, out returned, IntPtr.Zero), "Could not refresh partition devices");
                byte[] after = Layout(h);
                for (int i = 0; i < (int)BitConverter.ToUInt32(before, 4); i++) {
                    int p = Header + i * Entry;
                    if (BitConverter.ToInt64(before, p + 16) == 0) continue;
                    Guid id = GuidAt(before, p + 48);
                    int found = FindPartition(after, id, BitConverter.ToInt64(before, p + 8), BitConverter.ToInt64(before, p + 16), GuidAt(before, p + 32));
                    if (found != BitConverter.ToInt32(before, p + 24)) throw new InvalidDataException("Existing partition number changed");
                    // Attributes and partition name must also remain unchanged.
                    int q = Enumerable.Range(0, (int)BitConverter.ToUInt32(after, 4)).First(j => GuidAt(after, Header + j * Entry + 48) == id);
                    if (!before.Skip(p + 64).Take(80).SequenceEqual(after.Skip(Header + q * Entry + 64).Take(80)))
                        throw new InvalidDataException("Existing GPT metadata changed");
                }
                return new int[] { FindPartition(after, espId, start, espSize, EspType), FindPartition(after, rootId, start + espSize, rootSize, LinuxType) };
            }
        }
        public static void WritePartition(int disk, int number, Guid id, long start, long size, long imageSize, Stream input, string sha256, string role, Action<string,long,long> progress) {
            Guid type = role == "esp" ? EspType : role == "root" ? LinuxType : Guid.Empty;
            if (type == Guid.Empty || imageSize != (role == "esp" ? 2147483648L : 40800092160L) ||
                size < imageSize || (role == "esp" && size != imageSize)) throw new InvalidDataException("Unsupported partition payload or target bound");
            ValidateRange(start, size);
            if (FindPartition(ReadLayout(disk), id, start, size, type) != number) throw new InvalidDataException("Target partition changed");
            if (input == null || !input.CanRead) throw new InvalidDataException("Authenticated image stream is required");
            // The provider authenticates the encrypted envelope before handing
            // over this stream. Hash plaintext again as it is written; the
            // source stream is deliberately not required to seek or touch disk.
            using (var sourceHash = SHA256.Create()) {
                string device = @"\\?\GLOBALROOT\Device\Harddisk" + disk + @"\Partition" + number;
                using (var raw = CreateFile(device, READ | WRITE, 3, IntPtr.Zero, 3, 0x80000000, IntPtr.Zero)) {
                    if (raw.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot open newly allocated partition");
                    int returned;
                    var partitionInfo = new byte[Entry];
                    Check(DeviceIoControl(raw, 0x00070048, null, 0, partitionInfo, partitionInfo.Length, out returned, IntPtr.Zero), "Cannot reidentify opened partition handle");
                    if (returned < Entry || BitConverter.ToUInt32(partitionInfo, 0) != 1 ||
                        BitConverter.ToInt64(partitionInfo, 8) != start || BitConverter.ToInt64(partitionInfo, 16) != size ||
                        GuidAt(partitionInfo, 32) != type || GuidAt(partitionInfo, 48) != id)
                        throw new InvalidDataException("Opened partition handle differs from the approved GPT extent");
                    Check(DeviceIoControl(raw, 0x00090018, null, 0, null, 0, out returned, IntPtr.Zero), "Cannot exclusively lock new partition");
                    try {
                        Check(DeviceIoControl(raw, 0x00090020, null, 0, null, 0, out returned, IntPtr.Zero), "Cannot dismount new partition");
                        using (var output = new FileStream(raw, FileAccess.ReadWrite, 1048576, false)) {
                            var buffer = new byte[4 * 1048576]; long done = 0;
                            while (done < imageSize) {
                                int n = input.Read(buffer, 0, (int)Math.Min(buffer.Length, imageSize - done));
                                if (n == 0) throw new EndOfStreamException("Image changed while writing");
                                sourceHash.TransformBlock(buffer, 0, n, null, 0);
                                output.Write(buffer, 0, n); done += n;
                                if (progress != null && (done % (64 * 1048576) == 0 || done == imageSize)) progress("writing", done, imageSize);
                            }
                            if (input.ReadByte() != -1) throw new InvalidDataException("Plaintext image exceeds the authorized source bound");
                            sourceHash.TransformFinalBlock(new byte[0], 0, 0);
                            if (BitConverter.ToString(sourceHash.Hash).Replace("-", "").ToLowerInvariant() != sha256)
                                throw new InvalidDataException("Decrypted source image digest changed");
                            output.Flush(true); Check(FlushFileBuffers(raw), "Cannot flush partition contents");
                            output.Position = 0;
                            using (var verify = SHA256.Create()) {
                                done = 0;
                                while (done < imageSize) {
                                    int n = output.Read(buffer, 0, (int)Math.Min(buffer.Length, imageSize - done));
                                    if (n == 0) throw new EndOfStreamException("Short destination readback");
                                    verify.TransformBlock(buffer, 0, n, null, 0); done += n;
                                    if (progress != null && (done % (64 * 1048576) == 0 || done == imageSize)) progress("verifying", done, imageSize);
                                }
                                verify.TransformFinalBlock(new byte[0], 0, 0);
                                string actual = BitConverter.ToString(verify.Hash).Replace("-", "").ToLowerInvariant();
                                if (actual != sha256) throw new InvalidDataException("Mandatory destination readback failed");
                            }
                            Check(DeviceIoControl(raw, 0x0009001C, null, 0, null, 0, out returned, IntPtr.Zero), "Could not unlock new partition");
                        }
                    } catch { if (!raw.IsClosed) DeviceIoControl(raw, 0x0009001C, null, 0, null, 0, out returned, IntPtr.Zero); throw; }
                }
            }
        }
        static void FirmwarePrivilege() {
            SafeFileHandle token;
            Check(OpenProcessToken(System.Diagnostics.Process.GetCurrentProcess().Handle, 0x28, out token), "Cannot open process privilege token");
            using (token) {
                long luid; Check(LookupPrivilegeValue(null, "SeSystemEnvironmentPrivilege", out luid), "Firmware privilege is unavailable");
                var p = new Privileges { Count = 1, Luid = luid, Attributes = 2 };
                Check(AdjustTokenPrivileges(token, false, ref p, 0, IntPtr.Zero, IntPtr.Zero), "Cannot enable firmware privilege");
                if (Marshal.GetLastWin32Error() == 1300) throw new UnauthorizedAccessException("Administrator firmware privilege is required");
            }
        }
        static byte[] GetVariable(string name) {
            var b = new byte[65536]; uint n = GetFirmwareEnvironmentVariable(name, EfiGlobal, b, (uint)b.Length);
            if (n == 0) {
                int e = Marshal.GetLastWin32Error();
                if (e == 203) return null;
                throw new Win32Exception(e, "Cannot read UEFI variable " + name);
            }
            Array.Resize(ref b, (int)n); return b;
        }
        public static string FirmwarePreflight() {
            FirmwarePrivilege();
            byte[] order = GetVariable("BootOrder");
            if (order == null || order.Length % 2 != 0) throw new InvalidDataException("Existing UEFI BootOrder is unavailable");
            return Hash(order);
        }
        public static bool CanRestartToFirmware() {
            FirmwarePrivilege();
            byte[] supported = GetVariable("OsIndicationsSupported");
            return supported != null && supported.Length == 8 && (BitConverter.ToUInt64(supported, 0) & 1) != 0;
        }
        public sealed class WindowsBootTarget {
            public string entryName, description, entrySha256, partitionGuid, bootOrderSha256, bootOrderBase64;
            public string currentBootEntry, measuredBootSha256;
            public int partitionNumber;
            public long offsetBytes, sizeBytes;
        }
        public static WindowsBootTarget InspectWindowsBoot() {
            FirmwarePrivilege();
            byte[] order = GetVariable("BootOrder");
            // The pinned Limine EFI-entry protocol has a 128-entry order buffer.
            // Reserve one slot for our menu before changing anything.
            if (order == null || order.Length == 0 || order.Length % 2 != 0 || order.Length > 254)
                throw new InvalidDataException("The existing firmware boot order is unsupported");
            if (GetVariable("BootNext") != null)
                throw new InvalidDataException("A one-time firmware boot is scheduled. Complete that startup before installing");
            var indices = new System.Collections.Generic.HashSet<ushort>();
            WindowsBootTarget found = null;
            for (int i = 0; i < order.Length; i += 2) {
                ushort number = BitConverter.ToUInt16(order, i);
                if (!indices.Add(number)) throw new InvalidDataException("Duplicate firmware boot order entries");
                string name = "Boot" + number.ToString("X4"); byte[] option = GetVariable(name);
                if (option == null || option.Length < 8) continue;
                int descriptionEnd = 6;
                while (descriptionEnd + 1 < option.Length && BitConverter.ToUInt16(option, descriptionEnd) != 0) descriptionEnd += 2;
                if (descriptionEnd + 1 >= option.Length) throw new InvalidDataException("Invalid firmware entry description");
                string description = Encoding.Unicode.GetString(option, 6, descriptionEnd - 6);
                if (!string.Equals(description, "Windows Boot Manager", StringComparison.OrdinalIgnoreCase)) continue;
                if (found != null) throw new InvalidDataException("Multiple Windows Boot Manager entries are ambiguous");
                uint attributes = BitConverter.ToUInt32(option, 0);
                if ((attributes & 1) == 0 || (attributes & 0x1f00) != 0)
                    throw new InvalidDataException("Windows Boot Manager is not an active boot entry");
                int cursor = descriptionEnd + 2, end = cursor + BitConverter.ToUInt16(option, 4);
                if (end > option.Length) throw new InvalidDataException("Invalid Windows EFI device path");
                Guid partitionGuid = Guid.Empty; int partitionNumber = 0; long offset = 0, length = 0;
                string filePath = ""; bool terminated = false;
                while (cursor + 4 <= end) {
                    int nodeLength = BitConverter.ToUInt16(option, cursor + 2);
                    if (nodeLength < 4 || cursor + nodeLength > end) throw new InvalidDataException("Invalid EFI path node");
                    byte type = option[cursor], subtype = option[cursor + 1];
                    if (type == 4 && subtype == 1) {
                        if (nodeLength != 42 || partitionGuid != Guid.Empty || option[cursor + 40] != 2 || option[cursor + 41] != 2)
                            throw new InvalidDataException("Windows must use one GPT EFI system partition");
                        var bytes = new byte[16]; Array.Copy(option, cursor + 24, bytes, 0, 16); partitionGuid = new Guid(bytes);
                        partitionNumber = BitConverter.ToInt32(option, cursor + 4);
                        offset = checked((long)BitConverter.ToUInt64(option, cursor + 8) * 512);
                        length = checked((long)BitConverter.ToUInt64(option, cursor + 16) * 512);
                    } else if (type == 4 && subtype == 4) {
                        if (nodeLength < 6 || nodeLength % 2 != 0 || BitConverter.ToUInt16(option, cursor + nodeLength - 2) != 0)
                            throw new InvalidDataException("Invalid Windows loader path");
                        filePath += Encoding.Unicode.GetString(option, cursor + 4, nodeLength - 6);
                    } else if (type == 127) {
                        if (subtype != 255 || nodeLength != 4 || cursor + nodeLength != end)
                            throw new InvalidDataException("Multiple Windows EFI paths are unsupported");
                        terminated = true;
                    }
                    cursor += nodeLength;
                }
                if (!terminated || cursor != end || partitionGuid == Guid.Empty || partitionNumber < 1 || offset <= 0 || length <= 0
                    || !string.Equals(filePath, @"\EFI\Microsoft\Boot\bootmgfw.efi", StringComparison.OrdinalIgnoreCase))
                    throw new InvalidDataException("Windows Boot Manager does not identify a supported native Windows loader");
                found = new WindowsBootTarget { entryName = name, description = description, entrySha256 = Hash(option),
                    partitionGuid = partitionGuid.ToString("B"), partitionNumber = partitionNumber, offsetBytes = offset, sizeBytes = length,
                    bootOrderSha256 = Hash(order), bootOrderBase64 = Convert.ToBase64String(order) };
            }
            if (found == null) throw new InvalidDataException("A unique Windows Boot Manager firmware entry is required");
            byte[] current = GetVariable("BootCurrent");
            if (current == null || current.Length != 2) throw new InvalidDataException("Current firmware boot route is unavailable; restart directly into Windows Boot Manager");
            found.currentBootEntry = "Boot" + BitConverter.ToUInt16(current, 0).ToString("X4");
            if (found.currentBootEntry != found.entryName) throw new InvalidDataException("This Windows session did not start through its original firmware entry. Restart directly into Windows Boot Manager before installing");
            return found;
        }
        public static string RegisterBoot(int partition, long start, long size, Guid guid, string expectedOrderHash, string expectedWindowsHash) {
            var windows = InspectWindowsBoot();
            if (windows.entrySha256 != expectedWindowsHash || windows.bootOrderSha256 != expectedOrderHash)
                throw new InvalidDataException("Windows firmware target changed before menu registration");
            FirmwarePrivilege(); byte[] old = GetVariable("BootOrder");
            if (old == null || Hash(old) != expectedOrderHash) throw new InvalidDataException("Firmware BootOrder changed after confirmation");
            int index;
            for (index = 0; index <= 65535; index++) if (GetVariable("Boot" + index.ToString("X4")) == null) break;
            if (index > 65535) throw new InvalidOperationException("No free UEFI boot entry exists");
            byte[] path = Encoding.Unicode.GetBytes(@"\EFI\limine\limine_x64.efi" + "\0");
            var devicePath = new byte[42 + 4 + path.Length + 4];
            devicePath[0] = 4; devicePath[1] = 1; Put(devicePath, 2, BitConverter.GetBytes((ushort)42));
            Put(devicePath, 4, BitConverter.GetBytes(partition)); Put(devicePath, 8, BitConverter.GetBytes(start / 512));
            Put(devicePath, 16, BitConverter.GetBytes(size / 512)); Put(devicePath, 24, guid.ToByteArray());
            devicePath[40] = 2; devicePath[41] = 2;
            devicePath[42] = 4; devicePath[43] = 4; Put(devicePath, 44, BitConverter.GetBytes((ushort)(4 + path.Length)));
            Put(devicePath, 46, path); int end = devicePath.Length - 4; devicePath[end] = 127; devicePath[end + 1] = 255; devicePath[end + 2] = 4;
            byte[] desc = Encoding.Unicode.GetBytes("Omarchy OS menu\0"); var option = new byte[6 + desc.Length + devicePath.Length];
            Put(option, 0, BitConverter.GetBytes(1)); Put(option, 4, BitConverter.GetBytes((ushort)devicePath.Length));
            Put(option, 6, desc); Put(option, 6 + desc.Length, devicePath);
            string name = "Boot" + index.ToString("X4");
            if (GetVariable(name) != null || Hash(GetVariable("BootOrder") ?? new byte[0]) != expectedOrderHash)
                throw new InvalidDataException("Firmware configuration changed before registration");
            Check(SetFirmwareEnvironmentVariableEx(name, EfiGlobal, option, (uint)option.Length, 7), "Cannot register Omarchy UEFI entry");
            if (!option.SequenceEqual(GetVariable(name) ?? new byte[0])) throw new InvalidDataException("UEFI entry readback failed");
            if (Hash(GetVariable("BootOrder") ?? new byte[0]) != expectedOrderHash)
                throw new InvalidDataException("Firmware order changed during registration; original order was not overwritten");
            var updated = new byte[old.Length + 2]; Put(updated, 0, BitConverter.GetBytes((ushort)index)); Put(updated, 2, old);
            try {
                Check(SetFirmwareEnvironmentVariableEx("BootOrder", EfiGlobal, updated, (uint)updated.Length, 7), "Could not make the OS menu the first boot entry");
                if (!updated.SequenceEqual(GetVariable("BootOrder") ?? new byte[0])) throw new InvalidDataException("UEFI BootOrder readback failed");
            } catch {
                // Restore only a value we wrote, never an intervening firmware change.
                if (updated.SequenceEqual(GetVariable("BootOrder") ?? new byte[0])) {
                    Check(SetFirmwareEnvironmentVariableEx("BootOrder", EfiGlobal, old, (uint)old.Length, 7), "Could not restore the previous boot order");
                    if (!old.SequenceEqual(GetVariable("BootOrder") ?? new byte[0])) throw new InvalidDataException("Previous boot order restoration did not verify");
                }
                throw;
            }
            return name;
        }
    }
}
