# UI-3 — Netflix-inspired browse-first desktop experience

## Scope

UI-3 replaces the green/sidebar-first presentation with a dark, cinematic streaming-library experience inspired by the supplied Netflix reference:

- The application opens directly into the content Home screen instead of an empty player surface.
- A compact horizontal navigation bar replaces the tall left sidebar. It retains Home, Explore, local media, recent playback, favorites and dynamic Provider sources.
- The Home page is hero-first: the selected real Provider record supplies the title, description, genre, episode count and cover art; the hero CTA opens the existing detail page.
- Search, source filters and real genre filters remain available below the hero.
- Continue Watching and genre shelves use horizontal poster/card rails with hover and keyboard focus states.
- Explore, detail, local media, recent playback and favorites are restyled to the same black/charcoal/red visual system.
- The player is shown only after the user explicitly chooses playback or opens the player surface.

## Preserved contracts

- No Rust, libmpv worker, native session, HWND sizing/positioning, Tauri IPC command contract, Provider data contract or playback permission changes.
- Startup uses the existing set_catalog_visible(true) IPC to hide the native video HWND behind the browse-first Home surface. Playback continues through the existing player path.
- The player keeps its original 66px header / 96px footer geometry outside catalog mode.
- Remote covers still use CoverImage and safeCoverUrl; no generated/fabricated artwork, fake ratings, popularity ranks or fake metadata.
- playable=false, community browse-only mode, local file ownership, progress persistence and favorite semantics remain unchanged.

## Windows manual acceptance

- [ ] 1. Launch the exact UI-3 Debug artifact. The first screen is the Home page; no blank player screen appears.
- [ ] 2. Confirm the native video HWND is hidden while browsing and does not cover the Home UI.
- [ ] 3. Confirm the brand header and horizontal navigation are visible, usable and styled in black/charcoal/red.
- [ ] 4. Switch Home, Explore All, each registered Provider source, My Videos, Recent Plays and Favorites. The correct original view opens and active navigation is clear.
- [ ] 5. Confirm the Hero uses a real returned cover, title, description, genre and episode count. If cover is absent or fails, the existing neutral fallback remains; no fabricated poster appears.
- [ ] 6. Click the Hero CTA. The matching real detail opens; the Hero must not start playback.
- [ ] 7. Confirm search, source filters, real genre filters, refresh, retry and load-more work; when no results match, search/source controls remain available and reset restores all sources. Rapid source changes must not allow stale responses to overwrite current results.
- [ ] 8. Confirm Continue Watching appears only when real history exists and displays accurate progress; empty history does not create an empty shelf.
- [ ] 9. Confirm genre shelves are horizontal, scrollable, poster-first and contain only records returned by the selected Provider.
- [ ] 10. Hover a poster: it scales without clipping, shows its detail affordance and remains clickable. Tab/Shift+Tab/Enter/Space work.
- [ ] 11. Open a real Community SDK detail. Backdrop and poster use the real cover; description, source label, episode count and browse-only warning remain accurate.
- [ ] 12. Confirm playable=false episodes remain visible but disabled, and community browse-only episodes cannot start playback.
- [ ] 13. Start a permitted episode from detail or Continue Watching. The app switches to the player and native video/audio playback works.
- [ ] 14. Return to content browsing from the player. The Home/Explore state is usable and the native HWND is hidden again.
- [ ] 15. Verify local MP4 add/play/remove, favorites, recent history, resume and clearing history retain their previous behavior.
- [ ] 16. Resize to 480×320 and a normal desktop resolution. Navigation remains reachable, posters reflow and no unintended horizontal page overflow occurs.
- [ ] 17. Verify player pause/seek/thumbnail preview, audio/subtitle tracks, volume, speed, fullscreen, shortcuts and normal 96px footer are unchanged.
- [ ] 18. Restart the application. It returns to Home by default; it does not reopen an empty player surface or overwrite local history/preferences.

## Status

- Code: submitted for exact-head Windows M1/M2 CI.
- Windows manual acceptance: pending.
- Merge: prohibited until all applicable stacked prerequisites, exact-head CI, owner acceptance and explicit owner merge authorization are satisfied.
