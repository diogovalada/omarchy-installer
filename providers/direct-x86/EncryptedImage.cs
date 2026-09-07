// Bounded authenticated decoding of transient image-artifact envelopes.
// Wire contract: docs/evidence/artifact-encryption-contract.md.
// Provenance: gpt-6-astra. Compilation is not runtime crypto qualification.
using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;

namespace Omarchy.DirectX86 {
    public static class EncryptedImage {
        public const string Format = "openssl-aes-256-cbc-pbkdf2-sha256";
        public const int Iterations = 100000;
        public const long MaximumPlaintextLength = 64L * 1024 * 1024 * 1024;
        const int BufferSize = 1024 * 1024;
        static readonly byte[] Magic = Encoding.ASCII.GetBytes("Salted__");
        static readonly byte[] MacDomain = Encoding.ASCII.GetBytes("omarchy-artifact-mac-v1");

        static void Wipe(byte[] bytes) { if (bytes != null) Array.Clear(bytes, 0, bytes.Length); }
        static byte[] Digest(string value) {
            if (value == null || value.Length != 64) throw new InvalidDataException("A complete lowercase SHA-256 value is required");
            byte[] result = new byte[32];
            for (int i = 0; i < result.Length; i++) {
                int high = Hex(value[i * 2]), low = Hex(value[i * 2 + 1]);
                if (high < 0 || low < 0) throw new InvalidDataException("Invalid lowercase SHA-256 value");
                result[i] = (byte)((high << 4) | low);
            }
            return result;
        }
        static int Hex(char value) { return value >= '0' && value <= '9' ? value - '0' : value >= 'a' && value <= 'f' ? value - 'a' + 10 : -1; }
        static bool Equal(byte[] left, byte[] right) {
            if (left.Length != right.Length) return false;
            int difference = 0;
            for (int i = 0; i < left.Length; i++) difference |= left[i] ^ right[i];
            return difference == 0;
        }
        static void ReadExactly(Stream source, byte[] buffer, int count) {
            int position = 0;
            while (position < count) {
                int read = source.Read(buffer, position, count - position);
                if (read <= 0) throw new InvalidDataException("Encrypted image ended before its declared boundary");
                position += read;
            }
        }
        static string LocalPath(string path) {
            if (String.IsNullOrEmpty(path) || !Path.IsPathRooted(path) || path.StartsWith(@"\\", StringComparison.Ordinal) || path.IndexOf('\0') >= 0)
                throw new InvalidDataException("An ordinary absolute local ciphertext path is required");
            if (path.Length < 3 || !Char.IsLetter(path[0]) || path[1] != ':' || (path[2] != '\\' && path[2] != '/'))
                throw new InvalidDataException("Drive-relative ciphertext paths are not accepted");
            string full = Path.GetFullPath(path);
            if (full.Length < 3 || full[1] != ':' || full.Substring(2).Contains(":"))
                throw new InvalidDataException("Network/device paths and alternate data streams are not accepted");
            FileInfo file = new FileInfo(full);
            if (!file.Exists || (file.Attributes & (FileAttributes.Directory | FileAttributes.ReparsePoint)) != 0)
                throw new InvalidDataException("Ciphertext must be an existing regular file");
            DirectoryInfo parent = file.Directory;
            while (parent != null) {
                if ((parent.Attributes & FileAttributes.ReparsePoint) != 0) throw new InvalidDataException("Ciphertext directory links are not accepted");
                parent = parent.Parent;
            }
            return full;
        }
        static long StoredLength(long plaintextLength) {
            if (plaintextLength <= 0 || plaintextLength > MaximumPlaintextLength)
                throw new InvalidDataException("Plaintext length exceeds the fixed-recipe bound");
            return checked(16 + ((plaintextLength / 16) + 1) * 16);
        }

        public static Stream Open(string path, long plaintextLength, long storedLength,
            string storedSha256, string hmacSha256, byte[] rawKey) {
            return Open(path, plaintextLength, storedLength, storedSha256, hmacSha256, rawKey, null, null);
        }

        // checkCancel is intended for pre-allocation validation. Noninterruptible
        // native writes should use the overload without cancellation callbacks.
        public static Stream Open(string path, long plaintextLength, long storedLength,
            string storedSha256, string hmacSha256, byte[] rawKey,
            Action checkCancel, Action<long, long> authenticationProgress) {
            if (rawKey == null || rawKey.Length != 32) throw new InvalidDataException("A 32-byte in-memory operation key is required");
            if (storedLength != StoredLength(plaintextLength)) throw new InvalidDataException("Ciphertext size does not match the fixed PKCS7 envelope");
            byte[] expectedSha = Digest(storedSha256), expectedMac = Digest(hmacSha256);
            if (checkCancel != null) checkCancel();
            byte[] keyCopy = (byte[])rawKey.Clone(), macKey = null, password = null, derived = null;
            char[] passwordChars = new char[44];
            byte[] header = new byte[16], salt = new byte[8], aesKey = new byte[32], iv = new byte[16];
            FileStream ciphertext = null;
            Aes aes = null;
            ICryptoTransform decryptor = null;
            CryptoStream crypto = null;
            try {
                string full = LocalPath(path);
                // Hold this exact authenticated descriptor. Windows share mode
                // denies new writers/deleters for its entire decrypted-stream life.
                ciphertext = new FileStream(full, FileMode.Open, FileAccess.Read, FileShare.Read, BufferSize, FileOptions.SequentialScan);
                if (ciphertext.Length != storedLength) throw new InvalidDataException("Encrypted image length changed");
                using (HMACSHA256 domain = new HMACSHA256(keyCopy)) { macKey = domain.ComputeHash(MacDomain); }
                byte[] buffer = new byte[BufferSize];
                try {
                    using (SHA256 sha = SHA256.Create())
                    using (HMACSHA256 mac = new HMACSHA256(macKey)) {
                        long total = 0;
                        int read;
                        while ((read = ciphertext.Read(buffer, 0, buffer.Length)) != 0) {
                            if (checkCancel != null) checkCancel();
                            total = checked(total + read);
                            if (total > storedLength) throw new InvalidDataException("Encrypted image exceeded its stored boundary");
                            sha.TransformBlock(buffer, 0, read, buffer, 0);
                            mac.TransformBlock(buffer, 0, read, buffer, 0);
                            if (authenticationProgress != null) authenticationProgress(total, storedLength);
                        }
                        sha.TransformFinalBlock(new byte[0], 0, 0);
                        mac.TransformFinalBlock(new byte[0], 0, 0);
                        // Authenticate before interpreting or decrypting CBC data.
                        bool authenticated = Equal(mac.Hash, expectedMac);
                        bool shaMatches = Equal(sha.Hash, expectedSha);
                        if (total != storedLength || ciphertext.Length != storedLength || !authenticated || !shaMatches)
                            throw new CryptographicException("Encrypted image authentication failed");
                    }
                } finally { Wipe(buffer); }
                ciphertext.Position = 0;
                ReadExactly(ciphertext, header, header.Length);
                int magicDifference = 0;
                for (int i = 0; i < Magic.Length; i++) magicDifference |= Magic[i] ^ header[i];
                if (magicDifference != 0) throw new InvalidDataException("Unsupported encrypted image header");
                Buffer.BlockCopy(header, 8, salt, 0, salt.Length);
                int passwordLength = Convert.ToBase64CharArray(keyCopy, 0, keyCopy.Length, passwordChars, 0);
                if (passwordLength != 44) throw new CryptographicException("Unexpected operation-key encoding length");
                password = Encoding.ASCII.GetBytes(passwordChars);
                using (Rfc2898DeriveBytes kdf = new Rfc2898DeriveBytes(password, salt, Iterations, HashAlgorithmName.SHA256)) {
                    derived = kdf.GetBytes(48);
                }
                Buffer.BlockCopy(derived, 0, aesKey, 0, aesKey.Length);
                Buffer.BlockCopy(derived, 32, iv, 0, iv.Length);
                aes = Aes.Create();
                if (aes == null) throw new CryptographicException("AES is unavailable on this host");
                aes.KeySize = 256; aes.BlockSize = 128; aes.Mode = CipherMode.CBC; aes.Padding = PaddingMode.PKCS7;
                decryptor = aes.CreateDecryptor(aesKey, iv);
                crypto = new CryptoStream(ciphertext, decryptor, CryptoStreamMode.Read);
                Stream result = new PlaintextStream(ciphertext, crypto, decryptor, aes, plaintextLength, storedLength, checkCancel);
                ciphertext = null; crypto = null; decryptor = null; aes = null;
                return result;
            } finally {
                try {
                    try { if (crypto != null) crypto.Dispose(); }
                    finally { try { if (decryptor != null) decryptor.Dispose(); }
                        finally { try { if (aes != null) aes.Dispose(); } finally { if (ciphertext != null) ciphertext.Dispose(); } } }
                } finally {
                    Wipe(keyCopy); Wipe(macKey); Wipe(password); Wipe(derived); Wipe(aesKey); Wipe(iv); Wipe(salt); Wipe(header);
                    Array.Clear(passwordChars, 0, passwordChars.Length);
                }
            }
        }

        // Call for every image before any partition allocation or raw write.
        // Open() authenticates stored bytes; this additionally checks the clear
        // image hash, PKCS7 finalization and exact clear length from the manifest.
        public static void ValidateHash(string path, long plaintextLength, long storedLength,
            string storedSha256, string hmacSha256, string plaintextSha256, byte[] rawKey) {
            ValidateHash(path, plaintextLength, storedLength, storedSha256, hmacSha256, plaintextSha256, rawKey, null, null);
        }
        public static void ValidateHash(string path, long plaintextLength, long storedLength,
            string storedSha256, string hmacSha256, string plaintextSha256, byte[] rawKey,
            Action checkCancel, Action<long, long> progress) {
            byte[] expected = Digest(plaintextSha256);
            using (Stream source = Open(path, plaintextLength, storedLength, storedSha256, hmacSha256, rawKey, checkCancel, null))
            using (SHA256 sha = SHA256.Create()) {
                byte[] buffer = new byte[BufferSize];
                try {
                    long total = 0; int read;
                    while ((read = source.Read(buffer, 0, buffer.Length)) != 0) {
                        total = checked(total + read);
                        sha.TransformBlock(buffer, 0, read, buffer, 0);
                        if (progress != null) progress(total, plaintextLength);
                    }
                    sha.TransformFinalBlock(new byte[0], 0, 0);
                    if (total != plaintextLength || !Equal(sha.Hash, expected)) throw new CryptographicException("Decrypted image digest does not match its manifest");
                } finally { Wipe(buffer); }
            }
        }

        // Consumes exactly the first 512 clear bytes. Use a fresh Open() for the
        // actual write. This inspects LUKS2 metadata; it never unlocks a volume.
        public static string InspectLuksHeader(Stream plaintext) {
            if (plaintext == null || !plaintext.CanRead) throw new ArgumentException("A readable authenticated image stream is required");
            byte[] header = new byte[512];
            try {
                ReadExactly(plaintext, header, header.Length);
                byte[] magic = { (byte)'L', (byte)'U', (byte)'K', (byte)'S', 0xba, 0xbe, 0, 2 };
                int difference = 0;
                for (int i = 0; i < magic.Length; i++) difference |= header[i] ^ magic[i];
                if (difference != 0) throw new InvalidDataException("Root image does not contain a primary LUKS2 header");
                for (int i = 204; i < 208; i++) if (header[i] != 0) throw new InvalidDataException("LUKS2 UUID field is not terminated");
                string text = Encoding.ASCII.GetString(header, 168, 36);
                Guid id;
                if (!Guid.TryParseExact(text, "D", out id) || id == Guid.Empty) throw new InvalidDataException("LUKS2 header contains an invalid UUID");
                return id.ToString("D");
            } finally { Wipe(header); }
        }

        sealed class PlaintextStream : Stream {
            readonly FileStream ciphertext;
            readonly CryptoStream crypto;
            readonly ICryptoTransform decryptor;
            readonly Aes aes;
            readonly long length, storedLength;
            readonly Action checkCancel;
            long position;
            bool disposed, finished;
            internal PlaintextStream(FileStream file, CryptoStream stream, ICryptoTransform transform, Aes algorithm,
                long plaintextLength, long encryptedLength, Action cancellation) {
                ciphertext = file; crypto = stream; decryptor = transform; aes = algorithm;
                length = plaintextLength; storedLength = encryptedLength; checkCancel = cancellation;
            }
            public override bool CanRead { get { return !disposed; } }
            public override bool CanSeek { get { return false; } }
            public override bool CanWrite { get { return false; } }
            public override long Length { get { EnsureOpen(); return length; } }
            public override long Position { get { EnsureOpen(); return position; } set { throw new NotSupportedException("Encrypted image streams cannot seek"); } }
            void EnsureOpen() { if (disposed) throw new ObjectDisposedException("EncryptedImage"); }
            void Finish() {
                if (finished) return;
                if (crypto.ReadByte() != -1 || ciphertext.Length != storedLength) throw new InvalidDataException("Decrypted image exceeds its declared boundary");
                finished = true;
            }
            public override int Read(byte[] buffer, int offset, int count) {
                EnsureOpen();
                if (buffer == null) throw new ArgumentNullException("buffer");
                if (offset < 0 || count < 0 || offset > buffer.Length - count) throw new ArgumentOutOfRangeException("count");
                if (checkCancel != null) checkCancel();
                if (count == 0) return 0;
                if (position == length) { Finish(); return 0; }
                int requested = (int)Math.Min(Math.Min((long)count, BufferSize), length - position);
                int read = crypto.Read(buffer, offset, requested);
                if (read <= 0) throw new InvalidDataException("Decrypted image ended before its declared boundary");
                position = checked(position + read);
                // Check final padding/EOF before exposing the final plaintext
                // bytes, even if the caller stops reading at Length exactly.
                if (position == length) Finish();
                return read;
            }
            public override void Flush() { EnsureOpen(); }
            public override long Seek(long offset, SeekOrigin origin) { throw new NotSupportedException("Encrypted image streams cannot seek"); }
            public override void SetLength(long value) { throw new NotSupportedException("Encrypted image streams are read-only"); }
            public override void Write(byte[] buffer, int offset, int count) { throw new NotSupportedException("Encrypted image streams are read-only"); }
            protected override void Dispose(bool disposing) {
                if (!disposed && disposing) {
                    disposed = true;
                    try { crypto.Dispose(); }
                    finally { try { decryptor.Dispose(); } finally { try { aes.Dispose(); } finally { ciphertext.Dispose(); } } }
                }
                base.Dispose(disposing);
            }
        }
    }
}
