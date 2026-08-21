# Terminal-host protocol policy

Protocol version 1 uses a four-byte big-endian payload length followed by a
protobuf `Frame`. Frames larger than 1 MiB are rejected. Terminal output is
chunked to 32 KiB.

Compatibility rules:

1. A connection starts with `Hello` and the supported version range.
2. The host selects one overlapping version or returns `INCOMPATIBLE_PROTOCOL`.
3. Optional protobuf fields and new message variants may be added without
   changing the protocol version when older peers can safely ignore them.
4. Existing field numbers and meanings are never reused.
5. A semantic change to process, input, resize, replay, or close behavior
   requires a new protocol version.
6. Daemon upgrades must not restart a compatible running host.
7. Host replacement is always treated as destructive while terminals are active.

Output sequence numbers count raw PTY bytes. A daemon may request its last known
byte or request zero after process restart to rebuild a new server-side stream.
If that byte has fallen out of the rolling buffer, replay starts at
`retained_sequence`; the forward jump makes the loss explicit instead of
silently fabricating continuity.
