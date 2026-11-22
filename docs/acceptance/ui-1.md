# UI-1 — Cinematic shell and discovery home acceptance

## Scope

This phase refreshes the desktop visual system, app header, catalogue navigation and discovery home.

- Design tokens and a restrained dark cinematic palette.
- Branded desktop header with clearer app identity and connection status.
- More legible catalogue heading, navigation tabs, source controls and search.
- Poster-first catalogue cards using the existing provider artwork URL / cover relay.
- A clearer discovery hero, source/category filters and responsive layout.
- Focus-visible keyboard states and hover feedback.

## Explicit non-goals

- No Rust, libmpv worker, native HWND, Tauri IPC, Provider contract or playback-permission changes.
- No change to the existing 66px app header / 96px native-player footer geometry.
- No fabricated artwork, ranking, catalogue metadata or playback availability.
- No exposure of signed media URLs, credentials or local absolute paths.

## Windows manual acceptance

1. Launch the exact UI-1 Windows debug artifact from the matching green CI commit.
2. Open the content library. Confirm the header brand, connection indicator and return-to-player action remain visible and correctly aligned.
3. Confirm all existing catalogue tabs still switch to their original views: Home, Demo, HongGuo placeholder, opt-in Community SDK (when enabled), My Videos, Recent Plays and Favorites.
4. On Home, confirm the discovery banner, search, source filters and category filters remain usable.
5. Confirm the featured item and poster grid display the same real metadata and artwork as the underlying Provider response; no generated or substituted poster is introduced.
6. Confirm poster cards remain clickable and open the existing detail/episode view.
7. Confirm the existing Continue Watching shelf still resumes the same local/provider item and displays the same progress.
8. Resize the app to a narrower desktop window. Confirm the poster grid reflows without horizontal page overflow and controls remain reachable.
9. Use Tab / Shift+Tab and Enter / Space to confirm visible focus and keyboard activation for navigation, search, filters and cards.
10. Return to the player. Confirm native video rendering, playback controls, seek preview, audio/subtitle tracks, fullscreen and the 96px player footer are unchanged.

## Status

- Code submitted: pending exact-head Windows M1/M2 CI.
- Windows manual acceptance: pending.
- Merge: prohibited until the user confirms applicable manual acceptance and explicitly authorizes merge.
