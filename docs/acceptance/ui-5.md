# UI-5 — Detail page and player control-bar experience

## Scope

UI-5 improves the two highest-frequency surfaces after UI-4:

- Detail page: expandable synopsis, an episode-completion overview, exact episode-number/title search, watched-state filters, ascending/descending episode order, clear empty states and a visible current-resume episode.
- Episode list: preserve provider order semantics by sorting on the explicit episode number; keep upstream `playable=false` distinct from a provider that is globally browse-only; never enable playback from browse-only metadata.
- Player control bar: group playback/navigation, seek, audio/subtitle/rate/volume and utility/diagnostic controls; add an explicit mute/restore control that returns to the last audible volume.
- Improve keyboard focus, accessible group names, progress text and responsive behavior while retaining the existing horizontally scrollable control row.

## Preserved contracts

- No Rust source, libmpv worker ownership, native session ID, Tauri IPC command, playback authorization, media URL handling or Provider contract changes.
- Keep the 66px application header, 96px player footer and native video HWND geometry unchanged.
- Keep the existing native snapshot as the source of playback state; no optimistic UI state is introduced for play/pause, seek, rate, volume, track changes or fullscreen.
- Episode search/filter/sort is a frontend view over the current detail only. It does not alter provider metadata, persisted history or episode queue order used by the player.
- Provider history remains scoped to its series ID. A stale progress record from another series must never mark an episode as watched.
- Keep Community browse-only and upstream unavailable states distinct. Do not resolve a media URL or attempt playback from browse-only records.

## Automated validation

- `tests/detail-episodes.test.mjs`: title and exact number search, whitespace normalization, watched/incomplete/unavailable filters, ascending/descending order, no mutation, series-scoped progress and summary counts.
- `tests/player-volume.test.mjs`: mute, restore last audible volume and safe bounds/fallback.
- Both test suites are part of `npm run build`; existing player shortcut, thumbnail, track, preference and runtime-health suites remain in the same build chain.
- Isolated Node 22.16 execution of the two new pure-logic suites: **8/8 tests passed**. This validates the copied pure helpers/tests only; it is not a full repository `npm run build`, TypeScript typecheck or Windows CI result.
- Windows M1/M2: pending exact final UI-5 HEAD; wait for the owner’s “编译完成” notice before checking Actions. Do not poll while the owner is building.

## Windows manual acceptance (20 checks)

- [ ] 1. Launch the exact UI-5 Debug artifact. Home remains the default browse-first surface.
- [ ] 2. Open a long-description detail. The synopsis is clamped; “展开完整简介” and “收起简介” work without losing detail state.
- [ ] 3. Verify the episode summary against a known series: total, watched/completed counts, in-progress count and unavailable count are accurate; loading/error states are clear.
- [ ] 4. Search by exact episode number, including a query such as “第 12 集”; it must not also return episode 112 merely because the digit is a substring.
- [ ] 5. Search by episode title with/without spaces; clearing the query restores the current status-filter result.
- [ ] 6. Filter all, incomplete, completed and upstream-unavailable episodes. Community browse-only must remain a separate state, not be mislabeled as upstream unavailable.
- [ ] 7. Toggle ascending/descending order. The list is sorted by explicit episode number and does not mutate the source order or player queue.
- [ ] 8. Search/filter to zero results. The empty message and “清除筛选” action restore the full episode list.
- [ ] 9. The latest resumed episode is visibly highlighted; progress percentage and completion state match the stored history.
- [ ] 10. Browse-only community episodes remain disabled; upstream `playable=false` episodes remain visible and disabled; no playback resolver is called.
- [ ] 11. Continue Watching, completed-episode replay and next-episode entry still target the correct series/episode.
- [ ] 12. In the player, the grouped transport row exposes playback/navigation, seek, media settings and utility controls; horizontal scrolling reaches every control.
- [ ] 13. Pause/resume works through the button and existing Space/K shortcuts; UI follows the native player snapshot.
- [ ] 14. Provider previous/next episode and local playlist previous/next remain independent; EOF, Stop and failure behavior are unchanged.
- [ ] 15. Timeline dragging, keyboard seek and thumbnail preview work; preview hover never seeks the main player.
- [ ] 16. Audio and subtitle selection still work; no available track and track-change error states remain understandable.
- [ ] 17. Playback speed and volume selection work and persist as before.
- [ ] 18. The new mute button silences playback; restoring sound returns the last non-zero volume, including after manually changing volume before muting.
- [ ] 19. Fullscreen, shortcut help, runtime diagnostics and copy-diagnostics still work. Tab focus is visible and controls have meaningful accessible names.
- [ ] 20. Verify 480×320 and normal desktop layouts, unchanged 66px header / 96px footer and native video HWND geometry; regress UI-3 Home/Explore, UI-4 local library/history/favorites, M7/M8 queues, tracks, seek, volume, fullscreen and normal close/exit.

## Status

- Code: in development on `feat/ui-5-detail-player-experience`.
- UI-4 prerequisite: owner confirmed 21/21 Windows real-device checks passed; PR #37 remains Draft/unmerged.
- Windows M1/M2 CI: pending exact UI-5 HEAD.
- Windows manual acceptance: pending.
- Merge: prohibited until exact-head CI, owner acceptance, stacked prerequisites and explicit owner merge authorization are satisfied.
