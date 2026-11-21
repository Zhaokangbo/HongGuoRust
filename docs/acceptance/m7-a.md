# M7-A — Rust playback history and resume candidate

Depends on merged M6-B main at d4595c5. Does not change existing M5 installer distribution status.

## Data and implementation
- Adds defaulted per-item playback metadata to M6-B schema=1 `library.json` so old imported files survive.
- Only imported **local** MP4 items produce playback history; manual file picker, demo and HTTPS sources do not.
- The Rust libmpv worker tracks active media library ID and session ID, uses actual mpv progress,
  saves at most once per five seconds and on pause, seek, stop, replacement, failure and normal close.
- On `FileLoaded` attempts a one-time resume seek if saved position is at least 5 seconds
  and not within 5 seconds or the last 5% of a known duration. A natural EOF marks completed.
- `最近播放` displays watched local entries, last position/time, and a per-item clear-history action.
- Clearing history preserves the media directory entry and source file.
- This increment does not implement favorites, thumbnails, remote content or multi-process database synchronization.

## CI and manual acceptance
- [ ] Windows M1/M2 green for exact PR SHA, Rust fmt and unit tests passing.
- [ ] Existing M6-A demo and M6-B local media UI / 20-cycle playback regression passing.
- [ ] Import a long MP4, play >10s, stop and re-open: resumes close to last position.
- [ ] Close app while playing, reopen, select local media: same persisted resume behavior.
- [ ] Play to natural EOF: history shows completed; replay begins from 0.
- [ ] Clear history: item still in My Videos, original file remains untouched.
- [ ] Existing legacy library.json entries remain loadable with default history fields.
- [ ] No history should be created for demo/HTTPS sources or a media file never imported.

## Limitations
- No hard guarantee against power loss in the five-second checkpoint interval.
- A play request's IPC success means enqueued, not guaranteed rendered media; native playback errors still surface in the player.
- Single local application process only. Disable duplicate app instances before validating history saves.
