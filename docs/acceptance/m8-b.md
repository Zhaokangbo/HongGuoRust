# M8-B — explicit auto-next on genuine EOF (draft)

## Scope and invariants
- Add an opt-in **自动连播** checkbox within an existing M8-A temporary local playlist.
  Default is OFF on every app launch; no persistent preference, no loop/repeat.
- The current `library_play` Rust IPC already returns a monotonically increasing
  native playback **session ID**. Capture this ID for each successful queued play,
  including manual previous/next.
- Only advance when `get_snapshot` reports **Ended** for that exact queued
  native session ID, the user enabled auto-next, the current item has a next item,
  the IPC bridge is connected, and no playback transition is pending.
- `Ended` is produced by the Rust state machine for a natural libmpv EOF;
  manual Stop returns Idle; failed playback returns Error; no auto-next for either.
- A per-session consumed-EOF guard prevents repeated transitions from the same
  polled Ended snapshot. A failed next load displays an error and never loops.
- End-of-list stops naturally. Missing local files / stale queue IDs report
  the existing Rust validation error and preserve the queue index.
- Loading a standalone file, HTTPS URL or demo episode clears the queued
  session after a successful IPC; no cross-source auto-next.
- This is an in-memory UI orchestration layer. No changes to Rust libmpv
  threading, playback state enums, M7 history, local library metadata or M5 packaging.

## CI and manual acceptance
- [ ] Windows M1 and M2 both PASS on exact final PR head; M2 native
      audio/video, load, close and pixel test remain green.
- [ ] Three imported MP4s sorted into a stable temporary playlist.
- [ ] With auto OFF, playing to natural EOF **does not** advance.
- [ ] With auto ON, genuine natural EOF advances exactly once to the correct
      next MP4, with updated queue index and normal video/audio.
- [ ] At end-of-list the player stays Ended; no wraparound or second load.
- [ ] Stop during playback does not start another item; Error, close, seek,
      and pause/resume do not trigger auto-next.
- [ ] Rapid manual prev/next followed by stale EOF cannot switch to
      an unrelated item; previous/next retains M8-A behavior.
- [ ] Remove or move the next MP4: auto-load reports error, does not loop
      or skip silently, and user can still manually navigate elsewhere.
- [ ] Direct MP4, HTTPS or demo playback clears current temporary playlist;
      M7 history/resume/favorites remain correct.

## Release gate
Source-only draft PR until CI and owner Windows acceptance. Public NSIS build
still gated by Issue #7; no external/unauthorized content provider is added.
