# M1 dependency advisory follow-up

The pre-fix Windows audit reported one high-severity package. HongGuoRust declares Vite 6.4.3 directly as a devDependency. Vite incorporates launch-editor in its development-server middleware. The high advisory GHSA-587h-7wmj-h5pm affects the Windows file-existence check in that middleware. Exposure requires a running affected development server and an editor-launch request. The packaged Tauri app serves the built dist assets and ships neither the Vite server nor the editor launcher. The same job reported zero advisories with npm audit --omit=dev.

The advisory lists launch-editor 2.14.2 and Vite 6.4.4 as patched. The full audit also lists lower-severity Vite advisories. npm recommended Vite 6.4.6 within the existing major. Windows npm regenerated the lock using npm install --save-dev --save-exact --package-lock-only vite@6.4.6 and reported zero vulnerabilities. The package and lock diffs were applied verbatim. Windows CI runs npm ci, both audits, TypeScript and Vite build against the committed lock.

[Advisory](https://github.com/advisories/GHSA-587h-7wmj-h5pm) · [Original audit](https://github.com/Zhaokangbo/HongGuoRust/actions/runs/44754032754) · [Lock regeneration](https://github.com/Zhaokangbo/HongGuoRust/actions/runs/44754571655)
