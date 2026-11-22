# UI-4 — Personal media library, history and favorites

## Scope

UI-4 turns the existing local MP4, local playback history and favorites pages into a consistent personal-library experience.

- Replace dense text rows with responsive media cards using a neutral, clearly labeled local-video tile. No generated posters, guessed thumbnails, fake metadata or invented artwork.
- Add compact library statistics for total, available, missing, in-progress, completed and favorite records.
- Unify title/filename search, status filters and sort controls across local media, history and favorites.
- Keep playback queues derived from the current visible result order; unavailable files must never enter the queue.
- Preserve explicit confirmation for removing a media-library entry and clearing a history record.
- Keep favorites, playback progress and the original video file independent: un-favoriting does not clear history; clearing history does not remove a file or favorite; removing a library entry never deletes the original file.
- Keep Provider episode Continue Watching on the existing validated Provider path. This phase only restyles the personal local-media surfaces and does not merge Provider history into the local library database.

## Preserved contracts

- No Rust local-library persistence format, IPC command, libmpv worker, native session, HWND geometry, Provider data contract or playback authorization changes.
- Do not display absolute local paths or signed media URLs.
- No disk scan, file copy, upload, thumbnail extraction, transcoding or new native media worker.
- The card art is a neutral local-file placeholder, not a fabricated cover. A future thumbnail feature must be a separate authorized and resource-bounded milestone.
- Existing playlist order, manual next/previous behavior and auto-next preference remain unchanged.
- UI-3 Home, navigation, Explore, detail and browse-first startup behavior remain unchanged.

## Windows manual acceptance

- [ ] 1. Launch the exact UI-4 Debug artifact. Startup still opens the UI-3 Home surface.
- [ ] 2. Open My Videos. Existing local MP4 records render as consistent cards with neutral MP4 tiles; no fabricated posters or metadata appear.
- [ ] 3. Add a local MP4. The new record appears once; canceling the file picker changes nothing.
- [ ] 4. Search by title and by filename. Clearing the query restores the full current mode.
- [ ] 5. Filter available and missing files. Missing files remain visible with a clear unavailable state.
- [ ] 6. Sort by added order, recent playback, natural title order and progress.
- [ ] 7. Verify progress percentage, completed state and displayed duration against a known local file.
- [ ] 8. Play a card. The queue follows the visible filtered/sorted order and excludes unavailable files.
- [ ] 9. A missing/moved file cannot start playback but can still be removed from the library.
- [ ] 10. Toggle a favorite. The state persists after application restart and does not alter playback history.
- [ ] 11. Open Favorites. Only favorited local videos appear; search, filters, sort and playback work.
- [ ] 12. Cancel a favorite from Favorites. The card disappears from that view after refresh; its file and history remain.
- [ ] 13. Open Recent Plays. The existing Provider Continue Watching shelf still works and local history cards appear below it.
- [ ] 14. Continue an in-progress local video and replay a completed video. Both open the expected local media.
- [ ] 15. Clear a history record only after confirmation. The file remains in My Videos and its favorite state is unchanged.
- [ ] 16. Remove a local library item only after confirmation. The original file on disk remains untouched.
- [ ] 17. Verify loading, retry, empty state, no-match state, manual refresh and disabled action states; local-operation failures show one actionable alert rather than duplicate player errors.
- [ ] 18. Verify keyboard focus/Enter/Space, context-specific search labels for local/history/favorites, item-specific screen-reader labels for play/remove/clear actions, readable secondary text, 480×320 and normal desktop layouts.
- [ ] 19. Confirm no absolute local path, credential, signed URL or raw OS error details appear in the UI.
- [ ] 20. Regress UI-3 Home/Explore/detail, player pause/seek/tracks/volume/speed/fullscreen and normal 66px header / 96px footer.
- [ ] 21. Restart the app. Favorites and local history persist independently and Home remains the default page.

## Status

- Code: in development on feat/ui-4-personal-library.
- Windows M1/M2 CI: **PASS** on exact HEAD `95bbaca551ce8693d4ae0d39dd57a1cb6e9d4f03` — [M1 run 45047805176](https://github.com/Zhaokangbo/HongGuoRust/actions/runs/45047805176), [M2 playback run 45047805159](https://github.com/Zhaokangbo/HongGuoRust/actions/runs/45047805159). M2 Debug artifact ID `14764800311`; ZIP SHA-256 `9021693acc36b0b5f1b946f4e26f59313ada0b3c15a2d882f2fca90fa419425c`; inner EXE SHA-256 `44cde497eed65192c54d2dc7748388431f5596f487979b3c44e3e771266c0f6b`.
- Windows manual acceptance: pending.
- Merge: prohibited until exact-head CI, owner acceptance, stacked prerequisites and explicit owner merge authorization are satisfied.
