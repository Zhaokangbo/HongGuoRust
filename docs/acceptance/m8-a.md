# M8-A — local media search, sort and manual playlist navigation

Based on merged M7-B main at 9fc5406. Does not modify any native Rust
playback engine behavior, persistence schema, or M5 NSIS distribution flags.

## Scope
- Filter the existing local catalogue by case-insensitive title or filename.
  Applies independently to My Videos, Recently Played and Favorites.
- Sort the visible items by newest import, locale-aware title (natural number
  ordering), most recently played or completion/percentage watched.
- The visible **playable** items form an immutable in-memory session playlist
  when an item is selected. The player footer shows playlist index and offers
  previous/next by dispatching existing `library_play(id)` calls. Nonexistent
  media entries are omitted from the queue.
- The queue follows the user's filtered/sorted list as it existed when the
  first item was selected. It is cleared on standalone local/open HTTPS,
  stop, and demo episode playback.
- All video data and user metadata remain on the local machine. Sorting or
  filtering does not change stored JSON, watch progress, or favorites.

## Acceptance gates
- [ ] GitHub Windows M1 (TypeScript build, Rust format, native tests) PASS.
- [ ] GitHub Windows M2 (real video/audio, playback/close regressions) PASS.
- [ ] Import 3+ local MP4 files; title/filename search filters correctly;
  blank search restores the complete result list.
- [ ] Sort by title and recently played; ensure Recently Played and Favorites
  filter before sorting and count only their own subset.
- [ ] Start playing from sorted/filtered My Videos: next/previous follow the
  visible playlist order, never skipping to a demo or HTTPS item.
- [ ] The first/last controls are disabled at queue boundaries. Missing videos
  are excluded. A failed `library_play` must not change playlist index.
- [ ] Standalone player actions or demo playback clear the manual queue.
- [ ] Old M7-A resume and M7-B favorite/progress/history behavior still works.

## Explicit non-goals for M8-A
- No automatic play-next after EOF, no queue persistence, no remote content
  source, no folder scanner, and no playlists with drag-and-drop reordering.
- No installation or redistribution of a new public Windows build; M5 Issue #7
  still blocks public installer releases.
