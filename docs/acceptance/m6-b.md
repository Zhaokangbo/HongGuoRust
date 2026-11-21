# M6-B — Local MP4 library (candidate)

Based on merged M6-A main at 2ec78cd.

## Scope
- Separate My Videos tab alongside unchanged demo-provider content.
- Add user-selected MP4 through the existing Windows file picker.
- Persist versioned JSON metadata in LOCALAPPDATA/HongGuoRust/library.json using Windows atomic file replacement.
- Deduplicate canonical paths and maintain stable IDs; cap at 1000 items and 2 MiB metadata.
- Reuse PlayerService to play selected local videos, without copying or uploading media.
- Mark missing source files unavailable; removing a record never deletes the original MP4.
- Preserve invalid/corrupt library data without overwriting it.

## Acceptance
- [ ] Windows M1 and M2 CI PASS for this exact commit.
- [ ] Add/play/remove a local MP4 from the UI.
- [ ] Restart the app: entries remain, duplicate import does not duplicate entries.
- [ ] Move the source video: entry becomes unavailable; deleting the entry does not delete source.
- [ ] Existing demonstration catalog and M5 player flows still work.

## Out of scope
- Remote or unauthorized drama content providers, downloads and directory scanning.
- Playback history or thumbnails; future milestones.
- Concurrent editing by multiple app instances.
- Public M5 installer release (Issue #7 remains open).
