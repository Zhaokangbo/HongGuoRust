# M7-B — Favorites and progress visualization (draft)

## Scope
- Store `favorite` flag on each imported local MP4 in the existing versioned library.json;
  `#[serde(default)]` preserves compatibility with M6-B and M7-A imported entries.
- `library_set_favorite` accepts an existing ID and boolean and persists atomically
  using the existing LocalLibrary store_data implementation; no file deletion.
- React exposes a favorite button on media rows, filters a dedicated My Favorites tab,
  and displays a noninteractive progress bar from M7-A native playback history values.
- Clearing playback history does not clear favorites. Removing a library entry removes its
  favorite status because it removes the media reference, **not the original video**.
- Legacy M1–M7-A playback and local media workflows are left unchanged.

## Acceptance
- [ ] Windows M1 + M2 CI green at this PR's exact head.
- [ ] Existing M7-A history and resume tests continue passing.
- [ ] Mark a local MP4 favorite; My Favorites tab lists it; restart app to confirm persistence.
- [ ] Unfavorite from My Favorites; entry disappears from favorites but remains in My Videos.
- [ ] Playback progress indicator reflects 30+ sec recorded progress and completed playback.
- [ ] Clear playback history while favorite: favorite remains, progress is reset.
- [ ] No demo/HTTPS item appears in the favorites list; media is never uploaded or deleted.

## Release boundary
Source-code PR only; public Windows installer distribution remains gated in Issue #7.
