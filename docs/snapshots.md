# Integration and snapshot testing

Requires Docker. Tests use a pinned Linux x86_64 environment (emulated on Apple
Silicon) with Herdr 0.9.5. No host Herdr installation is needed; existing config,
sessions, and plugin registrations are untouched. The first run downloads the
environment and builds the plugin; caches live under `target/snapshots/`.

## Run checks

```sh
just test                                    # all Rust and Python tests
uv run --locked pytest                       # all Python tests, including snapshots
uv run --locked pytest tests/test_snapshots.py --snapshot-case grouped-worktree-pr
uv run --locked pytest tests/test_snapshots.py --repeat 2
```

The harness runs real servers, independently configured terminal clients,
repositories, linked worktrees, and the release plugin. Only GitHub is faked.
Terminal cells are rendered into PNGs using fixed fonts and dimensions.

Nine cases cover grouped worktrees, missing/present client tokens, adding tokens
and reloading client config, clearing metadata, and draft/pending, open/failing,
merged/passing, and closed/no-check displays. CI compares all nine twice.
The container uses uv 0.13.1, Python 3.15.0, and the same `uv.lock` as local
tests. A session-scoped pytest fixture builds the container image once, and each
snapshot case runs inside it automatically. The child pytest session builds its
Linux binary using the shared Cargo cache. Default pytest discovery includes
every suite; the `snapshot` marker is available for optional debugging filters.

## Update baselines

```sh
uv run --locked pytest tests/test_snapshots.py --record
uv run --locked pytest tests/test_snapshots.py --record --snapshot-case grouped-worktree-pr
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
a baseline. See [development checks](development.md#checks) for the
combined test command.

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
