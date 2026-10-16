# Integration and snapshot testing

Requires Docker. Tests use a pinned Linux x86_64 environment (emulated on Apple
Silicon) with Herdr 0.9.5. No host Herdr installation is needed; existing config,
sessions, and plugin registrations are untouched. The first run downloads the
environment and builds the plugin; caches live under `target/snapshots/`.

## Run checks

```sh
just test-snapshots
just test-snapshots grouped-worktree-pr       # compare one case
just test-snapshots --repeat 2                # check capture determinism
```

The harness runs real servers, independently configured terminal clients,
repositories, linked worktrees, and the release plugin. Only GitHub is faked.
Terminal cells are rendered into PNGs using fixed fonts and dimensions.

Nine cases cover grouped worktrees, missing/present client tokens, adding tokens
and reloading client config, clearing metadata, and draft/pending, open/failing,
merged/passing, and closed/no-check displays. CI compares all nine twice.

## Update baselines

```sh
just record-snapshots
just record-snapshots grouped-worktree-pr
```

Review PNGs and commit intentional changes with the sidebar change. Missing
baselines or any changed pixel fail comparison. Comparison never updates
baselines; recording is forbidden when `CI` is set. Noto Emoji renders check
symbols as monochrome glyphs; terminals may use colored emoji. Font licenses
are linked from the [README](../README.md#license).

## Diagnose failures

Diagnostics live under `target/snapshots/CASE/`: `expected.png`, `actual.png`,
and `difference.png` for mismatches, plus available screen text, raw ANSI output,
server logs, and `error.txt` for setup or capture failures. CI uploads previews
and diagnostics as `sidebar-snapshots` for seven days.

Read diagnostics before recording: a startup or decoder failure must not become
a baseline. See [development checks](development.md#checks) for the separate
Rust and smoke-test suite.

## Manual integration before release

Live GitHub authentication and remote transport need a manual check. Use a
disposable session and isolated configuration/state directories. Link the plugin
and configure the viewing client's sidebar. Check a branch with a known PR, a
linked worktree with a different PR, and a branch without a PR; verify numbers
and absence of extra separators. Include a closed or merged PR.

Switch branches and verify the next refresh, then request a manual refresh.
Disable/unlink and confirm watcher exit and token expiry within 180 seconds.
For remote use, verify the viewing client's configuration controls the layout.
Shut down and remove the disposable session afterward.
