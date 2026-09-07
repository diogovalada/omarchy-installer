// Read-only, bounded measured-boot inspection. No TPM commands, keys or PCR writes.
using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace Omarchy.DirectX86 {
    public static class NativeBootEvidence {
        const int MaxLog = 16 * 1024 * 1024;
        [DllImport("tbs.dll", ExactSpelling=true)]
        static extern uint Tbsi_Get_TCG_Log_Ex(uint logType, byte[] output, ref uint length);

        public static string Inspect(string partitionGuid, int partitionNumber, long offset, long size) {
            uint length = 0;
            uint result = Tbsi_Get_TCG_Log_Ex(0, null, ref length); // SRTM_CURRENT; never substitute an older boot log.
            if (result != 0 && result != 0x80284005)
                throw new InvalidDataException("Current measured boot log is unavailable. Restart Windows normally and inspect again.");
            if (length < 32 || length > MaxLog) throw new InvalidDataException("Unsupported measured boot log size");
            var bytes = new byte[length];
            result = Tbsi_Get_TCG_Log_Ex(0, bytes, ref length);
            if (result != 0 || length > bytes.Length) throw new InvalidDataException("Measured boot log changed while reading");
            Array.Resize(ref bytes, (int)length);
            return Validate(bytes, new Guid(partitionGuid), partitionNumber, offset, size);
        }

        static void Require(bool condition, string message) {
            if (!condition) throw new InvalidDataException(message);
        }
        static uint U32(byte[] bytes, ref int at) {
            Require(at >= 0 && at <= bytes.Length - 4, "Truncated measured boot log");
            uint value = BitConverter.ToUInt32(bytes, at); at += 4; return value;
        }
        static void Skip(byte[] bytes, ref int at, uint count) {
            Require(count <= bytes.Length - at, "Measured boot event exceeds its buffer");
            at += (int)count;
        }

        // Public pure parser permits synthetic malformed-log and route tests
        // without reading host firmware, invoking TBS or changing any disk.
        public static string Validate(byte[] bytes, Guid partitionGuid, int partitionNumber, long offset, long size) {
            Require(bytes != null && bytes.Length >= 32 && bytes.Length <= MaxLog, "Invalid measured boot log");
            int at = 0, events = 0, applications = 0;
            var sizes = new Dictionary<ushort, ushort>();
            bool agile = false;
            using (var route = new MemoryStream()) {
                while (at < bytes.Length) {
                    Require(++events <= 65536, "Too many measured boot events");
                    int start = at;
                    uint pcr = U32(bytes, ref at), type = U32(bytes, ref at);
                    Require(pcr <= 23, "Invalid measured boot PCR index");
                    if (agile) {
                        uint count = U32(bytes, ref at);
                        Require(count > 0 && count <= sizes.Count, "Invalid measured boot digest count");
                        var seen = new HashSet<ushort>();
                        for (int i = 0; i < count; i++) {
                            Require(at <= bytes.Length - 2, "Truncated digest algorithm");
                            ushort algorithm = BitConverter.ToUInt16(bytes, at); at += 2;
                            Require(sizes.ContainsKey(algorithm) && seen.Add(algorithm), "Unknown or duplicate measured boot digest");
                            Skip(bytes, ref at, sizes[algorithm]);
                        }
                    } else Skip(bytes, ref at, 20);
                    uint eventSize = U32(bytes, ref at); int data = at;
                    Skip(bytes, ref at, eventSize);
                    if (events == 1 && type == 3 && eventSize >= 16 &&
                        Encoding.ASCII.GetString(bytes, data, 16) == "Spec ID Event03\0") {
                        Require(pcr == 0 && eventSize >= 29, "Invalid measured boot specification header");
                        for (int i = start + 8; i < start + 28; i++) Require(bytes[i] == 0, "Invalid specification digest");
                        Require(bytes[data + 21] == 2 && (bytes[data + 23] == 1 || bytes[data + 23] == 2), "Unsupported measured boot specification");
                        int cursor = data + 24;
                        uint count = U32(bytes, ref cursor);
                        Require(count > 0 && count <= 16 && count * 4 + 29 <= eventSize, "Invalid digest-size table");
                        for (int i = 0; i < count; i++) {
                            ushort algorithm = BitConverter.ToUInt16(bytes, cursor), length = BitConverter.ToUInt16(bytes, cursor + 2);
                            Require(length > 0 && length <= 128 && !sizes.ContainsKey(algorithm), "Invalid digest-size entry");
                            sizes.Add(algorithm, length); cursor += 4;
                        }
                        Require(cursor + 1 + bytes[cursor] == at, "Invalid specification vendor data");
                        agile = true;
                    }
                    if (pcr != 4) continue;
                    // Preserve every PCR4 event/digest in the plan comparison,
                    // including firmware actions and Windows-specific events.
                    route.Write(bytes, start, at - start);
                    if (type != 0x80000003) continue; // EV_EFI_BOOT_SERVICES_APPLICATION
                    Require(++applications == 1, "Windows was started through additional EFI applications. Restart directly into Windows Boot Manager.");
                    Require(eventSize >= 36, "Truncated EFI image-load event");
                    ulong pathSize = BitConverter.ToUInt64(bytes, data + 24);
                    Require(pathSize == eventSize - 32, "Invalid EFI image device path size");
                    ValidatePath(bytes, data + 32, at, partitionGuid, partitionNumber, offset, size);
                }
                Require(applications == 1, "The current log does not establish a direct Windows boot. Restart Windows normally and inspect again.");
                return NativeDisk.Hash(route.ToArray());
            }
        }

        static void ValidatePath(byte[] bytes, int cursor, int end, Guid guid, int number, long offset, long size) {
            bool partition = false, terminated = false; string path = "";
            while (cursor <= end - 4) {
                int length = BitConverter.ToUInt16(bytes, cursor + 2);
                Require(length >= 4 && length <= end - cursor, "Invalid measured EFI device path");
                byte type = bytes[cursor], subtype = bytes[cursor + 1];
                if (type == 4 && subtype == 1) {
                    Require(!partition && length == 42 && bytes[cursor + 40] == 2 && bytes[cursor + 41] == 2,
                            "Measured Windows loader must use one GPT partition");
                    var id = new byte[16]; Array.Copy(bytes, cursor + 24, id, 0, 16);
                    Require(new Guid(id) == guid && BitConverter.ToInt32(bytes, cursor + 4) == number &&
                            BitConverter.ToUInt64(bytes, cursor + 8) == (ulong)(offset / 512) &&
                            BitConverter.ToUInt64(bytes, cursor + 16) == (ulong)(size / 512),
                            "Measured Windows partition differs from the current Windows firmware entry");
                    partition = true;
                } else if (type == 4 && subtype == 4) {
                    Require(length >= 6 && length % 2 == 0 && BitConverter.ToUInt16(bytes, cursor + length - 2) == 0,
                            "Invalid measured Windows loader path");
                    path += Encoding.Unicode.GetString(bytes, cursor + 4, length - 6);
                } else if (type == 127) {
                    Require(subtype == 255 && length == 4 && cursor + length == end, "Multiple measured EFI device paths are unsupported");
                    terminated = true;
                } else Require(type != 4, "Unsupported measured EFI media path");
                cursor += length;
            }
            Require(cursor == end && terminated && partition &&
                    String.Equals(path, @"\EFI\Microsoft\Boot\bootmgfw.efi", StringComparison.OrdinalIgnoreCase),
                    "Current measured boot did not load the original Windows Boot Manager directly");
        }
    }
}
