# UI-2 — Streaming catalog redesign acceptance

## Scope

UI-2 changes the catalog information architecture and content presentation:

- A persistent sidebar groups Home, Explore, local media, recent plays, favorites and currently registered Provider sources.
- Home uses the selected real Provider record as the featured content focus, then shows the real continue-watching shelf and horizontal rails grouped by returned genre metadata.
- Explore has a source context bar, search, genre filters, result counts, poster-first cards and the existing pagination controls.
- Detail pages use the actual cover for a cinematic backdrop and poster, with source/genre metadata, episode count, progress, resume/next-episode actions and a readable episode grid.
- Continue Watching cards use real Provider cover art; local MP4 entries use a neutral file tile.

## Explicit non-goals and safety constraints

- No Rust, libmpv worker, native HWND, Tauri IPC, Provider contract, media URL handling or playback permission changes.
- No fake posters, placeholder metadata presented as real content, popularity/rating scores, fabricated genre labels or synthetic episode availability.
- All remote artwork continues through the existing `CoverImage` and `safeCoverUrl` validation path.
- Provider `playable=false`, community browse-only mode, episode progress and resume semantics remain authoritative.
- Local media operations continue to store references only; no file copying, scanning or upload.
- The normal player window geometry and native rendering are unchanged. The catalog remains inside the existing catalog content row.

## Windows manual acceptance

1. Launch the exact UI-2 Windows Debug artifact from the same green CI HEAD.
2. Open Content Library. Confirm the new left sidebar is visible and the content area scrolls independently.
3. Click Home, Explore All, each registered Provider source, My Videos, Recent Plays and Favorites. Confirm the correct original view loads and the active sidebar entry is clear.
4. On Home, confirm the featured title, genre, episode count and cover correspond to the actual selected Provider result. If the Provider has no cover, confirm a neutral gradient remains rather than a fabricated poster.
5. Confirm Home search, source filters, genre filters, refresh, error retry and load-more behavior still work. Rapidly switch sources and confirm stale results do not overwrite the latest selection.
6. Confirm Continue Watching cards show actual Provider artwork and accurate local/episode progress. Resume the same item and verify playback position is preserved.
7. Open Explore All and a specific source. Confirm search, genre chips, result count, real 2:3 poster grid and load-more pagination work.
8. If the manifest source is available, import a user-selected JSON file and verify the existing second-click confirmation before clearing it remains intact.
9. Open a real Community SDK detail page. Confirm backdrop/poster use the same real cover, source label is accurate, episode count is correct and the browse-only warning remains when media allowlist is absent.
10. Confirm an upstream `playable=false` episode is visible but disabled and clearly labeled; confirm browse-only community episodes remain disabled.
11. Confirm first-episode, continue-watching and next-episode actions respect the existing playback-permission checks and saved progress.
12. Return from detail to the same catalog view without losing the selected source/search context.
13. Confirm local media, favorites and recent-play operations still add, remove, sort, search, clear history and play exactly as before.
14. Resize the window to its supported minimum (480×320) and a normal desktop size. Confirm navigation remains reachable, posters reflow and there is no unintended horizontal page overflow.
15. Use Tab / Shift+Tab / Enter / Space. Confirm visible focus and keyboard activation for sidebar navigation, search, filters, poster cards and episode cards.
16. Return to Player. Confirm native video HWND, play/pause, seeking, thumbnail preview, tracks, fullscreen, and the normal 96px player footer remain unchanged.

## Status

- Code submitted: pending exact-head Windows M1/M2 CI.
- Windows manual acceptance: pending.
- Merge: prohibited until the user confirms manual acceptance and explicitly authorizes merge.
