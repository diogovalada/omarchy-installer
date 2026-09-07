use omarchy_fake_block_device::{
    FaultAction, FaultInjectingDevice, FaultRule, FileBlockDevice, MemoryBlockDevice,
};
use omarchy_media_writer::{
    write_and_verify, BlockDevice, CancellationToken, DeviceOperation, Stage, WriteError, WritePlan,
};
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};

const SENTINEL: u8 = 0xA5;

fn sample_image(length: usize) -> Vec<u8> {
    (0..length).map(|index| (index % 251) as u8).collect()
}

#[test]
fn bounded_write_flushes_and_fully_verifies_without_touching_sentinels() {
    let prefix = 71;
    let image = sample_image(4_099);
    let suffix = 89;
    let mut device = MemoryBlockDevice::new(prefix + image.len() + suffix, SENTINEL);
    let before = device.bytes().to_vec();
    let plan = WritePlan::new(prefix as u64, image.len() as u64, 127).unwrap();
    let mut source = Cursor::new(image.clone());
    let mut progress = Vec::new();

    let report = write_and_verify(
        &mut source,
        &mut device,
        &plan,
        &CancellationToken::new(),
        |event| progress.push(event),
    )
    .unwrap();

    assert_eq!(report.written_bytes, image.len() as u64);
    assert_eq!(report.verified_bytes, image.len() as u64);
    assert_eq!(&device.bytes()[..prefix], &before[..prefix]);
    assert_eq!(
        &device.bytes()[prefix..prefix + image.len()],
        image.as_slice()
    );
    assert_eq!(
        &device.bytes()[prefix + image.len()..],
        &before[prefix + image.len()..]
    );
    assert_eq!(device.flush_count(), 1);
    assert_eq!(progress.last().unwrap().stage, Stage::Verifying);
    assert_eq!(progress.last().unwrap().completed_bytes, image.len() as u64);
}

#[test]
fn two_non_target_sentinel_devices_remain_byte_identical() {
    let image = sample_image(1_024);
    let mut target = MemoryBlockDevice::new(2_048, 0);
    let sentinel_left = MemoryBlockDevice::new(2_048, 0x11);
    let sentinel_right = MemoryBlockDevice::new(2_048, 0x22);
    let left_before = sentinel_left.bytes().to_vec();
    let right_before = sentinel_right.bytes().to_vec();

    write_and_verify(
        &mut Cursor::new(image),
        &mut target,
        &WritePlan::new(0, 1_024, 257).unwrap(),
        &CancellationToken::new(),
        |_| {},
    )
    .unwrap();

    assert_eq!(sentinel_left.bytes(), left_before);
    assert_eq!(sentinel_right.bytes(), right_before);
}

#[test]
fn cancellation_stops_at_a_chunk_boundary_and_preserves_everything_after_it() {
    let image = sample_image(1_000);
    let before = vec![SENTINEL; 1_200];
    let mut device = MemoryBlockDevice::from_bytes(before.clone());
    let token = CancellationToken::new();
    let mut source = Cursor::new(image.clone());
    let plan = WritePlan::new(50, image.len() as u64, 100).unwrap();

    let error = write_and_verify(&mut source, &mut device, &plan, &token, |progress| {
        if progress.stage == Stage::Writing && progress.completed_bytes == 200 {
            token.cancel();
        }
    })
    .unwrap_err();

    assert!(matches!(
        error,
        WriteError::Cancelled {
            stage: Stage::Writing,
            completed_bytes: 200
        }
    ));
    assert_eq!(&device.bytes()[..50], &before[..50]);
    assert_eq!(&device.bytes()[50..250], &image[..200]);
    assert_eq!(&device.bytes()[250..], &before[250..]);
    assert_eq!(device.flush_count(), 0);
}

#[test]
fn short_writes_are_retried_without_crossing_the_plan_boundary() {
    let image = sample_image(300);
    let mut device = FaultInjectingDevice::new(
        MemoryBlockDevice::new(512, SENTINEL),
        [FaultRule {
            operation: DeviceOperation::Write,
            call: 1,
            action: FaultAction::Short(3),
        }],
    );

    write_and_verify(
        &mut Cursor::new(image.clone()),
        &mut device,
        &WritePlan::new(37, image.len() as u64, 64).unwrap(),
        &CancellationToken::new(),
        |_| {},
    )
    .unwrap();

    let device = device.into_inner();
    assert_eq!(&device.bytes()[..37], &[SENTINEL; 37]);
    assert_eq!(&device.bytes()[37..337], image.as_slice());
    assert_eq!(&device.bytes()[337..], &[SENTINEL; 175]);
}

#[test]
fn a_zero_length_short_write_fails_instead_of_spinning() {
    let mut device = FaultInjectingDevice::new(
        MemoryBlockDevice::new(100, SENTINEL),
        [FaultRule {
            operation: DeviceOperation::Write,
            call: 1,
            action: FaultAction::Short(0),
        }],
    );
    let error = write_and_verify(
        &mut Cursor::new(vec![1; 50]),
        &mut device,
        &WritePlan::new(0, 50, 50).unwrap(),
        &CancellationToken::new(),
        |_| {},
    )
    .unwrap_err();

    assert!(matches!(
        error,
        WriteError::DeviceIo {
            operation: DeviceOperation::Write,
            ..
        }
    ));
}

#[test]
fn injected_flush_failure_prevents_verification_and_success() {
    let mut device = FaultInjectingDevice::new(
        MemoryBlockDevice::new(100, SENTINEL),
        [FaultRule {
            operation: DeviceOperation::Flush,
            call: 1,
            action: FaultAction::Error(std::io::ErrorKind::Other),
        }],
    );
    let error = write_and_verify(
        &mut Cursor::new(vec![1; 50]),
        &mut device,
        &WritePlan::new(0, 50, 17).unwrap(),
        &CancellationToken::new(),
        |_| {},
    )
    .unwrap_err();

    assert!(matches!(
        error,
        WriteError::DeviceIo {
            operation: DeviceOperation::Flush,
            ..
        }
    ));
}

#[test]
fn corrupt_readback_is_reported_with_its_image_offset() {
    let mut device = FaultInjectingDevice::new(
        MemoryBlockDevice::new(256, 0),
        [FaultRule {
            operation: DeviceOperation::Read,
            call: 2,
            action: FaultAction::CorruptRead { xor: 0x80 },
        }],
    );
    let error = write_and_verify(
        &mut Cursor::new(sample_image(128)),
        &mut device,
        &WritePlan::new(10, 128, 32).unwrap(),
        &CancellationToken::new(),
        |_| {},
    )
    .unwrap_err();

    assert!(matches!(
        error,
        WriteError::VerificationMismatch {
            image_offset: 32,
            ..
        }
    ));
}

#[test]
fn size_checks_fail_before_mutating_the_target() {
    let before = vec![SENTINEL; 100];
    let mut device = MemoryBlockDevice::from_bytes(before.clone());
    let error = write_and_verify(
        &mut Cursor::new(vec![1; 80]),
        &mut device,
        &WritePlan::new(30, 80, 16).unwrap(),
        &CancellationToken::new(),
        |_| {},
    )
    .unwrap_err();
    assert!(matches!(error, WriteError::TargetTooSmall { .. }));
    assert_eq!(device.bytes(), before);

    let error = write_and_verify(
        &mut Cursor::new(vec![1; 79]),
        &mut device,
        &WritePlan::new(0, 80, 16).unwrap(),
        &CancellationToken::new(),
        |_| {},
    )
    .unwrap_err();
    assert!(matches!(error, WriteError::SourceLength { .. }));
    assert_eq!(device.bytes(), before);
}

#[test]
fn file_backend_is_fixed_size_and_round_trips_through_writer() {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "omarchy-fake-block-{}-{}.img",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));

    let image = sample_image(333);
    let mut device = FileBlockDevice::create(&path, 512, SENTINEL).unwrap();
    write_and_verify(
        &mut Cursor::new(image.clone()),
        &mut device,
        &WritePlan::new(100, image.len() as u64, 71).unwrap(),
        &CancellationToken::new(),
        |_| {},
    )
    .unwrap();
    let snapshot = device.snapshot().unwrap();
    drop(device);
    std::fs::remove_file(path).unwrap();

    assert_eq!(&snapshot[..100], &[SENTINEL; 100]);
    assert_eq!(&snapshot[100..433], image.as_slice());
    assert_eq!(&snapshot[433..], &[SENTINEL; 79]);
}

#[test]
fn backends_refuse_offsets_past_capacity() {
    let mut memory = MemoryBlockDevice::new(10, 0);
    assert!(memory.write_at(11, &[1]).is_err());
    assert!(memory.read_at(11, &mut [0]).is_err());
}
