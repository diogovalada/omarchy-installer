using System;
using System.IO;
using System.Text;
using Omarchy.DirectX86;

public static class BootEvidenceTests {
    static readonly Guid DiskGuid = new Guid("27f20b4e-ae9d-47a3-9ad8-5468d0c6a17a");
    static byte[] Bytes(Action<BinaryWriter> action) {
        using (var stream = new MemoryStream()) {
            using (var writer = new BinaryWriter(stream, Encoding.UTF8, true)) action(writer);
            return stream.ToArray();
        }
    }
    static byte[] Image(string file) {
        var path = Bytes(w => {
            w.Write((byte)4); w.Write((byte)1); w.Write((ushort)42);
            w.Write(1); w.Write((ulong)2048); w.Write((ulong)4096); w.Write(DiskGuid.ToByteArray()); w.Write((byte)2); w.Write((byte)2);
            var name = Encoding.Unicode.GetBytes(file + "\0");
            w.Write((byte)4); w.Write((byte)4); w.Write((ushort)(name.Length + 4)); w.Write(name);
            w.Write((byte)127); w.Write((byte)255); w.Write((ushort)4);
        });
        return Bytes(w => { w.Write((ulong)0x1000); w.Write((ulong)65536); w.Write((ulong)0); w.Write((ulong)path.Length); w.Write(path); });
    }
    static void Event(BinaryWriter w, uint pcr, uint type, byte[] data, bool agile) {
        w.Write(pcr); w.Write(type);
        if (agile) { w.Write((uint)1); w.Write((ushort)11); w.Write(new byte[32]); }
        else w.Write(new byte[20]);
        w.Write((uint)data.Length); w.Write(data);
    }
    static byte[] Log(bool agile, string file, bool extra) {
        return Bytes(w => {
            if (agile) {
                var spec = Bytes(s => {
                    s.Write(Encoding.ASCII.GetBytes("Spec ID Event03\0")); s.Write((uint)0);
                    s.Write((byte)0); s.Write((byte)2); s.Write((byte)0); s.Write((byte)2);
                    s.Write((uint)1); s.Write((ushort)11); s.Write((ushort)32); s.Write((byte)0);
                });
                Event(w, 0, 3, spec, false);
            }
            Event(w, 4, 4, new byte[4], agile);
            Event(w, 4, 0x80000003, Image(file), agile);
            if (extra) Event(w, 4, 0x80000003, Image(file), agile);
        });
    }
    static string Validate(byte[] log) { return NativeBootEvidence.Validate(log, DiskGuid, 1, 1048576, 2097152); }
    static void Reject(byte[] log) {
        try { Validate(log); }
        catch (InvalidDataException) { return; }
        throw new Exception("An invalid boot route was accepted");
    }
    public static int Run() {
        int checks = 0;
        foreach (bool agile in new[] {false, true}) {
            var valid = Log(agile, @"\EFI\Microsoft\Boot\bootmgfw.efi", false);
            if (Validate(valid).Length != 64) throw new Exception("Expected boot evidence digest"); checks++;
            Reject(Log(agile, @"\EFI\limine\limine_x64.efi", false)); checks++;
            Reject(Log(agile, @"\EFI\Microsoft\Boot\bootmgfw.efi", true)); checks++;
            // Every truncated prefix must be rejected, including exact event
            // boundaries before a Windows application has been established.
            for (int length = 0; length < valid.Length; length++) {
                var truncated = new byte[length]; Array.Copy(valid, truncated, length); Reject(truncated); checks++;
            }
            var oversized = (byte[])valid.Clone(); oversized[28] = 255; oversized[29] = 255; oversized[30] = 255; oversized[31] = 127;
            Reject(oversized); checks++;
        }
        return checks;
    }
}
