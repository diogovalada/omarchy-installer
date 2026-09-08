// Read-only lifetime guard for a source authenticated by the native helper.
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace Omarchy.DirectX86 {
    public sealed class NativeSource : IDisposable {
        [StructLayout(LayoutKind.Sequential)]
        struct Information {
            public uint Attributes, CreationLow, CreationHigh, AccessLow, AccessHigh;
            public uint WriteLow, WriteHigh, VolumeSerial, SizeHigh, SizeLow, Links, IndexHigh, IndexLow;
        }
        [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
        static extern SafeFileHandle CreateFile(string path, uint access, uint sharing, IntPtr security, uint creation, uint flags, IntPtr template);
        [DllImport("kernel32.dll", SetLastError=true)]
        static extern bool GetFileInformationByHandle(SafeFileHandle handle, out Information information);
        readonly List<SafeFileHandle> handles = new List<SafeFileHandle>();
        public uint VolumeSerial { get; private set; }
        public ulong FileIndex { get; private set; }

        public NativeSource(string path, long length) {
            if (String.IsNullOrEmpty(path) || path.Length < 4 || path[1] != ':' || path[2] != '\\' || path.IndexOf(':', 2) >= 0 || Path.GetFullPath(path) != path)
                throw new IOException("Source must use an absolute local path without traversal or alternate streams.");
            try {
                var parents = new Stack<string>();
                for (var parent = Path.GetDirectoryName(path); !String.IsNullOrEmpty(parent); parent = Path.GetDirectoryName(parent)) parents.Push(parent);
                while (parents.Count > 0) Open(parents.Pop(), true);
                var info = Open(path, false);
                if (length <= 0 || (((ulong)info.SizeHigh << 32) | info.SizeLow) != (ulong)length)
                    throw new IOException("The verified source length changed.");
                VolumeSerial = info.VolumeSerial;
                FileIndex = ((ulong)info.IndexHigh << 32) | info.IndexLow;
                if (FileIndex == 0) throw new IOException("The source has no stable file identity.");
            } catch { Dispose(); throw; }
        }
        Information Open(string path, bool directory) {
            var handle = CreateFile(path, directory ? 0x80u : 0x80000000u,
                directory ? 3u : 1u, IntPtr.Zero, 3, 0x00200000u | (directory ? 0x02000000u : 0u), IntPtr.Zero);
            if (handle.IsInvalid) { var error = new Win32Exception(); handle.Dispose(); throw error; }
            handles.Add(handle);
            Information info;
            if (!GetFileInformationByHandle(handle, out info)) throw new Win32Exception();
            if ((info.Attributes & 0x400) != 0 || ((info.Attributes & 0x10) != 0) != directory)
                throw new IOException("The source path contains a reparse point or unexpected file type.");
            return info;
        }
        public void Match(uint volumeSerial, ulong fileIndex) {
            if (volumeSerial != VolumeSerial || fileIndex != FileIndex)
                throw new IOException("The source does not match the helper's verified file.");
        }
        public void Dispose() {
            for (int i = handles.Count - 1; i >= 0; --i) handles[i].Dispose();
            handles.Clear();
        }
    }
}
