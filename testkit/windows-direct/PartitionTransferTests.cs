// Exercise the production transfer loop through real Windows unbuffered I/O.
// Only newly created ordinary files are opened. No disk or firmware methods run.
using System;
using System.IO;
using System.Linq;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
using Omarchy.DirectX86;

public static class PartitionTransferTests {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern SafeFileHandle CreateFile(string n, uint a, uint s, IntPtr sa, uint c, uint f, IntPtr t);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool WriteFile(SafeFileHandle h, IntPtr b, int n, out int written, IntPtr ov);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool SetFilePointerEx(SafeFileHandle h, long distance, out long position, uint method);

    sealed class FragmentedSource : MemoryStream {
        public FragmentedSource(byte[] bytes) : base(bytes, false) { }
        public override int Read(byte[] bytes, int offset, int count) { return base.Read(bytes, offset, Math.Min(count, 997)); }
    }
    static void Assert(bool condition, string message) { if (!condition) throw new Exception(message); }
    static void Corrupt(SafeFileHandle handle) {
        IntPtr allocation=Marshal.AllocHGlobal(131071);
        IntPtr aligned=new IntPtr((allocation.ToInt64()+65535)&~65535L);
        try {
            Marshal.Copy(Enumerable.Repeat((byte)0x5a,65536).ToArray(),0,aligned,65536);
            long position; int written;
            Assert(SetFilePointerEx(handle,0,out position,0),"Cannot seek fixture for corruption");
            Assert(WriteFile(handle,aligned,65536,out written,IntPtr.Zero)&&written==65536,"Cannot corrupt fixture");
            Assert(SetFilePointerEx(handle,0,out position,0),"Cannot rewind fixture after corruption");
        } finally { Marshal.FreeHGlobal(allocation); }
    }
    public static int Run(string directory) {
        directory=Path.GetFullPath(directory);
        Assert(directory.StartsWith(Path.GetFullPath(Path.GetTempPath()),StringComparison.OrdinalIgnoreCase),"Tests require their own temporary directory");
        var bytes=new byte[5*1048576];
        for(int i=0;i<bytes.Length;i++) bytes[i]=(byte)(i*31+(i>>8));
        string digest=NativeDisk.Hash(bytes);
        int checks=0;
        foreach(string fault in new[]{"none","short-source","long-source","wrong-hash","corrupt-readback","flush-failure","read-failure"}) {
            string path=Path.Combine(directory,Guid.NewGuid().ToString("N")+".img");
            byte[] original=Enumerable.Repeat((byte)0xa5,bytes.Length+65536).ToArray();
            using(var file=new FileStream(path,FileMode.CreateNew,FileAccess.Write,FileShare.None)) file.Write(original,0,original.Length);
            try {
                using(var handle=CreateFile(path,0xc0000000,0,IntPtr.Zero,3,0xa0000000,IntPtr.Zero)) {
                    Assert(!handle.IsInvalid,"Cannot open the temporary file for unbuffered I/O");
                    byte[] source=fault=="short-source"?bytes.Take(bytes.Length-1).ToArray():fault=="long-source"?bytes.Concat(new byte[]{1}).ToArray():bytes;
                    var stages=new List<string>();
                    bool rejected=false;
                    using(var input=new FragmentedSource(source)) {
                        try {
                            NativeDisk.TransferVerifiedImage(handle,input,bytes.Length,fault=="wrong-hash"?new string('0',64):digest,(phase,done,total)=>{
                                stages.Add(phase);
                                if(phase=="flushing") { Assert(total==0,"Flush must not display completed byte progress"); if(fault=="flush-failure")handle.Dispose(); }
                                if(phase=="verifying"&&done==0) {
                                    if(fault=="corrupt-readback")Corrupt(handle);
                                    if(fault=="read-failure")handle.Dispose();
                                }
                            });
                        } catch { rejected=true; }
                    }
                    Assert(rejected==(fault!="none"),"Unexpected transfer outcome: "+fault);
                    if(fault=="none") Assert(stages.IndexOf("flushing")>stages.IndexOf("writing")&&stages.IndexOf("verifying")>stages.IndexOf("flushing"),"Verification must follow flushing");
                }
                byte[] actual=File.ReadAllBytes(path);
                Assert(actual.Length==original.Length,"Transfer changed the target file length");
                Assert(actual.Skip(bytes.Length).SequenceEqual(original.Skip(bytes.Length)),"Transfer exceeded its approved image boundary");
                if(fault=="none")Assert(actual.Take(bytes.Length).SequenceEqual(bytes),"Successful transfer did not preserve exact source bytes");
                checks++;
            } finally { File.Delete(path); }
        }
        return checks;
    }
}
