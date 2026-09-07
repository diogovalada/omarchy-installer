# Omarchy media writer

Platform-neutral streaming, durable flush, and full read-back verification.
This crate cannot discover or open a physical disk; a future privileged helper
must provide an already-authorized `BlockDevice` implementation.
