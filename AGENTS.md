# Repository agent entry point — HongGuoRust

Before continuing any multi-session development or modifying this repository:

1. **Read `docs/AI_DEVELOPMENT_HANDOFF.md` from the current `main` branch** for architecture, current milestone, user preferences, CI/acceptance status, stacked Draft PR dependencies and next steps.
2. **Verify facts with GitHub**, including actual PR HEADs and workflow runs. The handoff is a checkpoint, not a substitute for checking current code/CI.
3. Maintain player worker ownership: the native libmpv playback instance is owned by the Rust worker; native session IDs guard async responses; never expose credential-bearing media URLs in UI or history.
4. **Do not merge Draft PRs** before all the applicable Windows manual acceptance criteria are explicitly confirmed and the user authorizes merging. Respect chronological stacked PR dependencies.
5. **Do not poll GitHub Actions while building**. Once CI has started, wait for the user's “编译完成” notice before inspecting its result.
6. At every meaningful milestone or CI handoff, update `docs/AI_DEVELOPMENT_HANDOFF.md` on `main` with the newest verified branch/HEAD, status, tests, user signoff, and next action. Never mark unverified work as passed.
7. All Windows debug artifacts must come from the exact green CI commit; verify downloaded ZIP/EXE hashes before sharing; do not overwrite `%LOCALAPPDATA%\\HongGuoRust` user data.
8. User communicates in Chinese; prefers the assistant to modify GitHub code directly and then let the user perform native Windows acceptance.

The canonical cross-chat starting request is in the handoff document's **“跨对话接力约定”** section.
