//! Harmless block-device implementations for tests and dry-run providers.

use omarchy_media_writer::{BlockDevice, DeviceOperation};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Fixed-size fake device kept entirely in memory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryBlockDevice {
    bytes: Vec<u8>,
    flush_count: u64,
}

impl MemoryBlockDevice {
    pub fn new(capacity: usize, fill: u8) -> Self {
        Self {
            bytes: vec![fill; capacity],
            flush_count: 0,
        }
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            flush_count: 0,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    pub fn flush_count(&self) -> u64 {
        self.flush_count
    }
}

impl BlockDevice for MemoryBlockDevice {
    fn capacity(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        let start = checked_start(offset, self.bytes.len())?;
        let count = buffer.len().min(self.bytes.len() - start);
        buffer[..count].copy_from_slice(&self.bytes[start..start + count]);
        Ok(count)
    }

    fn write_at(&mut self, offset: u64, buffer: &[u8]) -> io::Result<usize> {
        let start = checked_start(offset, self.bytes.len())?;
        let count = buffer.len().min(self.bytes.len() - start);
        self.bytes[start..start + count].copy_from_slice(&buffer[..count]);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_count += 1;
        Ok(())
    }
}

/// Fixed-size file-backed fake. Writes beyond the initial capacity are
/// rejected instead of silently extending the file.
#[derive(Debug)]
pub struct FileBlockDevice {
    file: File,
    capacity: u64,
}

impl FileBlockDevice {
    /// Create or truncate a fake device and initialize every byte to `fill`.
    pub fn create(path: impl AsRef<Path>, capacity: u64, fill: u8) -> io::Result<Self> {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(path)?;
        file.set_len(capacity)?;

        if fill != 0 {
            let block = vec![fill; 64 * 1024];
            let mut remaining = capacity;
            file.seek(SeekFrom::Start(0))?;
            while remaining > 0 {
                let count = remaining.min(block.len() as u64) as usize;
                file.write_all(&block[..count])?;
                remaining -= count as u64;
            }
            file.sync_all()?;
        }

        Ok(Self { file, capacity })
    }

    /// Open an existing regular file as a fixed-size fake device.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "fake block device must be a regular file",
            ));
        }
        Ok(Self {
            capacity: metadata.len(),
            file,
        })
    }

    pub fn snapshot(&mut self) -> io::Result<Vec<u8>> {
        let length = usize::try_from(self.capacity).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "fake device is too large to snapshot on this host",
            )
        })?;
        let mut bytes = vec![0_u8; length];
        self.file.seek(SeekFrom::Start(0))?;
        self.file.read_exact(&mut bytes)?;
        Ok(bytes)
    }
}

impl BlockDevice for FileBlockDevice {
    fn capacity(&self) -> u64 {
        self.capacity
    }

    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        let count = bounded_count(offset, buffer.len(), self.capacity)?;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read(&mut buffer[..count])
    }

    fn write_at(&mut self, offset: u64, buffer: &[u8]) -> io::Result<usize> {
        let count = bounded_count(offset, buffer.len(), self.capacity)?;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write(&buffer[..count])
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.sync_all()
    }
}

/// Operation-relative, one-shot deterministic fault.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FaultRule {
    pub operation: DeviceOperation,
    /// One-based call number for this operation type.
    pub call: u64,
    pub action: FaultAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FaultAction {
    /// Return this OS-independent I/O error kind without touching the device.
    Error(io::ErrorKind),
    /// Allow at most this many bytes through. Zero is useful for testing
    /// no-progress detection.
    Short(usize),
    /// Perform the read, then corrupt its first returned byte.
    CorruptRead { xor: u8 },
}

/// Wrapper that injects reproducible failures into any fake backend.
#[derive(Debug)]
pub struct FaultInjectingDevice<D> {
    inner: D,
    rules: Vec<PendingFault>,
    read_calls: u64,
    write_calls: u64,
    flush_calls: u64,
}

#[derive(Debug)]
struct PendingFault {
    rule: FaultRule,
    consumed: bool,
}

impl<D> FaultInjectingDevice<D> {
    pub fn new(inner: D, rules: impl IntoIterator<Item = FaultRule>) -> Self {
        Self {
            inner,
            rules: rules
                .into_iter()
                .map(|rule| PendingFault {
                    rule,
                    consumed: false,
                })
                .collect(),
            read_calls: 0,
            write_calls: 0,
            flush_calls: 0,
        }
    }

    pub fn inner(&self) -> &D {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut D {
        &mut self.inner
    }

    pub fn into_inner(self) -> D {
        self.inner
    }

    fn take_action(&mut self, operation: DeviceOperation, call: u64) -> Option<FaultAction> {
        self.rules
            .iter_mut()
            .find(|pending| {
                !pending.consumed
                    && pending.rule.operation == operation
                    && pending.rule.call == call
            })
            .map(|pending| {
                pending.consumed = true;
                pending.rule.action.clone()
            })
    }
}

impl<D: BlockDevice> BlockDevice for FaultInjectingDevice<D> {
    fn capacity(&self) -> u64 {
        self.inner.capacity()
    }

    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
        self.read_calls += 1;
        match self.take_action(DeviceOperation::Read, self.read_calls) {
            Some(FaultAction::Error(kind)) => Err(injected_error(kind)),
            Some(FaultAction::Short(limit)) => {
                let count = limit.min(buffer.len());
                self.inner.read_at(offset, &mut buffer[..count])
            }
            Some(FaultAction::CorruptRead { xor }) => {
                let count = self.inner.read_at(offset, buffer)?;
                if count > 0 {
                    buffer[0] ^= xor;
                }
                Ok(count)
            }
            None => self.inner.read_at(offset, buffer),
        }
    }

    fn write_at(&mut self, offset: u64, buffer: &[u8]) -> io::Result<usize> {
        self.write_calls += 1;
        match self.take_action(DeviceOperation::Write, self.write_calls) {
            Some(FaultAction::Error(kind)) => Err(injected_error(kind)),
            Some(FaultAction::Short(limit)) => {
                let count = limit.min(buffer.len());
                self.inner.write_at(offset, &buffer[..count])
            }
            Some(FaultAction::CorruptRead { .. }) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "CorruptRead is valid only for read faults",
            )),
            None => self.inner.write_at(offset, buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_calls += 1;
        match self.take_action(DeviceOperation::Flush, self.flush_calls) {
            Some(FaultAction::Error(kind)) => Err(injected_error(kind)),
            Some(FaultAction::Short(_)) | Some(FaultAction::CorruptRead { .. }) => {
                Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "byte-oriented fault is invalid for flush",
                ))
            }
            None => self.inner.flush(),
        }
    }
}

fn checked_start(offset: u64, length: usize) -> io::Result<usize> {
    let start = usize::try_from(offset)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "offset does not fit in usize"))?;
    if start > length {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "offset is beyond fake device capacity",
        ));
    }
    Ok(start)
}

fn bounded_count(offset: u64, requested: usize, capacity: u64) -> io::Result<usize> {
    if offset > capacity {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "offset is beyond fake device capacity",
        ));
    }
    Ok((capacity - offset).min(requested as u64) as usize)
}

fn injected_error(kind: io::ErrorKind) -> io::Error {
    io::Error::new(kind, "deterministically injected fake-device failure")
}
