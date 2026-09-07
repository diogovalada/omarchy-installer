//! Safe, platform-neutral media-writing primitives.
//!
//! This crate deliberately contains no raw-disk discovery or operating-system
//! device-opening code. Callers must supply an already-open [`BlockDevice`]
//! selected by a separate policy layer.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};

/// Largest allocation a plan can request for a single streaming chunk.
pub const MAX_CHUNK_SIZE: usize = 16 * 1024 * 1024;

/// Minimal random-access operations needed by the streaming writer.
///
/// Implementations must not grow beyond `capacity`, and must report short or
/// failed operations using normal `std::io` semantics.
pub trait BlockDevice {
    /// Fixed number of addressable bytes in this opened device.
    fn capacity(&self) -> u64;

    /// Read starting at an absolute byte offset.
    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> io::Result<usize>;

    /// Write starting at an absolute byte offset.
    fn write_at(&mut self, offset: u64, buffer: &[u8]) -> io::Result<usize>;

    /// Make all completed writes durable before verification begins.
    fn flush(&mut self) -> io::Result<()>;
}

/// Immutable bounds for one image-write operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WritePlan {
    target_offset: u64,
    image_length: u64,
    chunk_size: usize,
}

impl WritePlan {
    /// Validate and construct an operation plan.
    pub fn new(
        target_offset: u64,
        image_length: u64,
        chunk_size: usize,
    ) -> Result<Self, PlanError> {
        if image_length == 0 {
            return Err(PlanError::EmptyImage);
        }
        if chunk_size == 0 || chunk_size > MAX_CHUNK_SIZE {
            return Err(PlanError::InvalidChunkSize {
                requested: chunk_size,
                maximum: MAX_CHUNK_SIZE,
            });
        }
        target_offset
            .checked_add(image_length)
            .ok_or(PlanError::AddressOverflow)?;

        Ok(Self {
            target_offset,
            image_length,
            chunk_size,
        })
    }

    pub fn target_offset(&self) -> u64 {
        self.target_offset
    }

    pub fn image_length(&self) -> u64 {
        self.image_length
    }

    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    pub fn target_end(&self) -> u64 {
        // Construction proves this cannot overflow.
        self.target_offset + self.image_length
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanError {
    EmptyImage,
    InvalidChunkSize { requested: usize, maximum: usize },
    AddressOverflow,
}

impl fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyImage => write!(formatter, "an empty image cannot be written"),
            Self::InvalidChunkSize { requested, maximum } => write!(
                formatter,
                "chunk size {requested} is outside the allowed range 1..={maximum}"
            ),
            Self::AddressOverflow => write!(formatter, "target byte range overflows u64"),
        }
    }
}

impl std::error::Error for PlanError {}

/// Cooperative, thread-safe cancellation signal.
#[derive(Debug, Default)]
pub struct CancellationToken(AtomicBool);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    Writing,
    Flushing,
    Verifying,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Progress {
    pub stage: Stage,
    pub completed_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriteReport {
    pub written_bytes: u64,
    pub verified_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceOperation {
    Read,
    Write,
    Flush,
}

/// Failure from a write operation. Cancellation is never reported as success,
/// even if all bytes happened to reach the fake device before it was observed.
#[derive(Debug)]
pub enum WriteError {
    Plan(PlanError),
    TargetTooSmall {
        required_end: u64,
        capacity: u64,
    },
    SourceLength {
        expected: u64,
        actual: u64,
    },
    SourceIo(io::Error),
    DeviceIo {
        operation: DeviceOperation,
        offset: u64,
        source: io::Error,
    },
    Cancelled {
        stage: Stage,
        completed_bytes: u64,
    },
    VerificationMismatch {
        image_offset: u64,
        expected: u8,
        actual: u8,
    },
}

impl fmt::Display for WriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plan(error) => write!(formatter, "invalid write plan: {error}"),
            Self::TargetTooSmall {
                required_end,
                capacity,
            } => write!(
                formatter,
                "target ends at byte {capacity}, but plan requires byte {required_end}"
            ),
            Self::SourceLength { expected, actual } => write!(
                formatter,
                "source length is {actual} bytes, but plan requires exactly {expected} bytes"
            ),
            Self::SourceIo(error) => write!(formatter, "source I/O failed: {error}"),
            Self::DeviceIo {
                operation,
                offset,
                source,
            } => write!(
                formatter,
                "device {operation:?} failed at byte {offset}: {source}"
            ),
            Self::Cancelled {
                stage,
                completed_bytes,
            } => write!(
                formatter,
                "operation was cancelled during {stage:?} after {completed_bytes} bytes"
            ),
            Self::VerificationMismatch {
                image_offset,
                expected,
                actual,
            } => write!(
                formatter,
                "read-back mismatch at image byte {image_offset}: expected {expected:#04x}, got {actual:#04x}"
            ),
        }
    }
}

impl std::error::Error for WriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Plan(error) => Some(error),
            Self::SourceIo(error) => Some(error),
            Self::DeviceIo { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<PlanError> for WriteError {
    fn from(error: PlanError) -> Self {
        Self::Plan(error)
    }
}

/// Stream an image into the plan's exact target range, durably flush it, and
/// compare every byte read back from the device with the source.
///
/// The source must be seekable so the same bytes can be used for full
/// verification without retaining the entire image in memory. Its remaining
/// length is checked before any target mutation.
pub fn write_and_verify<S, D, P>(
    source: &mut S,
    target: &mut D,
    plan: &WritePlan,
    cancellation: &CancellationToken,
    mut on_progress: P,
) -> Result<WriteReport, WriteError>
where
    S: Read + Seek,
    D: BlockDevice,
    P: FnMut(Progress),
{
    if plan.target_end() > target.capacity() {
        return Err(WriteError::TargetTooSmall {
            required_end: plan.target_end(),
            capacity: target.capacity(),
        });
    }

    let source_start = source.stream_position().map_err(WriteError::SourceIo)?;
    let source_end = source
        .seek(SeekFrom::End(0))
        .map_err(WriteError::SourceIo)?;
    source
        .seek(SeekFrom::Start(source_start))
        .map_err(WriteError::SourceIo)?;
    let source_length = source_end.saturating_sub(source_start);
    if source_length != plan.image_length {
        return Err(WriteError::SourceLength {
            expected: plan.image_length,
            actual: source_length,
        });
    }

    check_cancel(cancellation, Stage::Writing, 0)?;
    let mut buffer = vec![0_u8; plan.chunk_size];
    let mut written = 0_u64;
    while written < plan.image_length {
        check_cancel(cancellation, Stage::Writing, written)?;
        let wanted = next_chunk(plan.image_length - written, buffer.len());
        read_source_exact(source, &mut buffer[..wanted])?;
        write_device_exact(target, plan.target_offset + written, &buffer[..wanted])?;
        written += wanted as u64;
        on_progress(Progress {
            stage: Stage::Writing,
            completed_bytes: written,
            total_bytes: plan.image_length,
        });
    }

    check_cancel(cancellation, Stage::Flushing, written)?;
    target.flush().map_err(|source| WriteError::DeviceIo {
        operation: DeviceOperation::Flush,
        offset: plan.target_offset + written,
        source,
    })?;
    on_progress(Progress {
        stage: Stage::Flushing,
        completed_bytes: written,
        total_bytes: plan.image_length,
    });

    source
        .seek(SeekFrom::Start(source_start))
        .map_err(WriteError::SourceIo)?;
    let mut expected = vec![0_u8; plan.chunk_size];
    let mut actual = vec![0_u8; plan.chunk_size];
    let mut verified = 0_u64;
    while verified < plan.image_length {
        check_cancel(cancellation, Stage::Verifying, verified)?;
        let wanted = next_chunk(plan.image_length - verified, expected.len());
        read_source_exact(source, &mut expected[..wanted])?;
        read_device_exact(target, plan.target_offset + verified, &mut actual[..wanted])?;

        if let Some(index) = expected[..wanted]
            .iter()
            .zip(&actual[..wanted])
            .position(|(left, right)| left != right)
        {
            return Err(WriteError::VerificationMismatch {
                image_offset: verified + index as u64,
                expected: expected[index],
                actual: actual[index],
            });
        }

        verified += wanted as u64;
        on_progress(Progress {
            stage: Stage::Verifying,
            completed_bytes: verified,
            total_bytes: plan.image_length,
        });
    }

    Ok(WriteReport {
        written_bytes: written,
        verified_bytes: verified,
    })
}

fn next_chunk(remaining: u64, buffer_length: usize) -> usize {
    remaining.min(buffer_length as u64) as usize
}

fn check_cancel(
    cancellation: &CancellationToken,
    stage: Stage,
    completed_bytes: u64,
) -> Result<(), WriteError> {
    if cancellation.is_cancelled() {
        Err(WriteError::Cancelled {
            stage,
            completed_bytes,
        })
    } else {
        Ok(())
    }
}

fn read_source_exact<S: Read>(source: &mut S, mut buffer: &mut [u8]) -> Result<(), WriteError> {
    while !buffer.is_empty() {
        match source.read(buffer) {
            Ok(0) => {
                return Err(WriteError::SourceIo(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "source ended during streaming",
                )))
            }
            Ok(count) => buffer = &mut buffer[count..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(WriteError::SourceIo(error)),
        }
    }
    Ok(())
}

fn write_device_exact<D: BlockDevice>(
    target: &mut D,
    mut offset: u64,
    mut buffer: &[u8],
) -> Result<(), WriteError> {
    while !buffer.is_empty() {
        match target.write_at(offset, buffer) {
            Ok(0) => {
                return Err(WriteError::DeviceIo {
                    operation: DeviceOperation::Write,
                    offset,
                    source: io::Error::new(io::ErrorKind::WriteZero, "device made no progress"),
                })
            }
            Ok(count) => {
                offset += count as u64;
                buffer = &buffer[count..];
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(source) => {
                return Err(WriteError::DeviceIo {
                    operation: DeviceOperation::Write,
                    offset,
                    source,
                })
            }
        }
    }
    Ok(())
}

fn read_device_exact<D: BlockDevice>(
    target: &mut D,
    mut offset: u64,
    mut buffer: &mut [u8],
) -> Result<(), WriteError> {
    while !buffer.is_empty() {
        match target.read_at(offset, buffer) {
            Ok(0) => {
                return Err(WriteError::DeviceIo {
                    operation: DeviceOperation::Read,
                    offset,
                    source: io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "device ended during read-back",
                    ),
                })
            }
            Ok(count) => {
                offset += count as u64;
                buffer = &mut buffer[count..];
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(source) => {
                return Err(WriteError::DeviceIo {
                    operation: DeviceOperation::Read,
                    offset,
                    source,
                })
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_reject_empty_images_and_unsafe_chunks() {
        assert_eq!(WritePlan::new(0, 0, 1), Err(PlanError::EmptyImage));
        assert!(matches!(
            WritePlan::new(0, 1, 0),
            Err(PlanError::InvalidChunkSize { .. })
        ));
        assert!(matches!(
            WritePlan::new(0, 1, MAX_CHUNK_SIZE + 1),
            Err(PlanError::InvalidChunkSize { .. })
        ));
    }

    #[test]
    fn plans_reject_address_overflow() {
        assert_eq!(
            WritePlan::new(u64::MAX, 2, 1),
            Err(PlanError::AddressOverflow)
        );
    }
}
