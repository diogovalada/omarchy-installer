# Fake block device testkit

In-memory and regular-file implementations of the media writer's `BlockDevice`
contract, plus deterministic short-I/O, error, and read-corruption injection.
It has no code capable of opening platform raw-disk paths.
