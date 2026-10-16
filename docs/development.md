# Development

Use stable Rust/Cargo, [Just](https://github.com/casey/just), and
[uv](https://docs.astral.sh/uv/getting-started/installation/).
Run `uv sync --locked` to install the pinned Python 3.15.0 environment and test
dependencies from `uv.lock`. Run `just` to list recipes. End-user installation
still uses system Python 3 and does not require uv or pytest.

## Local plugin

With Herdr 0.9.5 or later installed, run from this checkout on the server:

```sh
just plugin-link
just plugin-check
just plugin-start
```

`plugin-link` builds and registers the checkout. `plugin-check` builds and reads
associations without writing metadata or state. `plugin-start` builds and invokes
the start action in the selected session. Plugin registry/action recipes honor
`HERDR_BIN_PATH`. Add the [sidebar tokens](../README.md#sidebar-configuration)
on the viewing client separately.

Use `just plugin-refresh` for one refresh and `just plugin-stop` to stop the
watcher. Keep the checkout in place while linked; `just plugin-unlink` unregisters
it without deleting files. The `watch` binary command is an internal entrypoint.

## Checks

```sh
just test
just ready
```

`just test` runs all Rust tests and one pytest invocation covering every Python
suite, including installer syntax and real sidebar snapshots. `just ready` adds
formatting checks and Clippy with warnings denied. Docker must be running;
pytest builds the release binary once and prepares the pinned snapshot image
automatically. There are no separate suite-specific test recipes.

`uv run --locked pytest` runs all Python tests directly with the same automatic
setup. Use standard pytest selection when debugging, for example
`uv run --locked pytest tests/test_configure.py` or `-k legacy`. Tests use pytest
fixtures and plain assertions, including `pytest.raises` for expected errors.
The shared lockfile uses a one-day minimum release age for dependency resolution.

Tests use temporary repositories, linked worktrees, fake Herdr/`gh` executables,
and Unix sockets. After the initial dependency/container downloads, tests need
no network, credentials, existing Herdr server, or access to your plugin registry.
CI runs `just ready` on macOS and Linux. Linux includes every suite and repeats
snapshots twice; hosted macOS runners exclude only the Docker snapshot marker
because they have no Docker daemon. See [snapshot testing](snapshots.md) for
baseline updates and diagnostics.

## Installer and config migrations

`install.sh` resolves the latest published GitHub release and asks Herdr to
install its exact tag. It uses the installed executable's `configure` command
for local setup. Config-only mode builds that helper from a temporary release
source archive without registering a plugin. Publishing the first release with
`configure` is required before the combined/config-only public installer works;
the older `0.1.0` release supports manual config setup only.

The explicit `configure` CLI is the sole owner of config writes. Watcher
commands and startup hooks must never invoke it. `src/configure.rs` owns
`CONFIG_VERSION`, initially 1, and state under
`${XDG_STATE_HOME:-$HOME/.local/state}/herdr-pr-status/configs/`, keyed by the
resolved config path. State survives plugin updates/reinstalls. Each entry
contains a lock, `migration.json` with the full path and completed version,
and `before-1.toml` when migration 1 changed an existing file.

Do not bump `CONFIG_VERSION` for package releases, cosmetic changes, or to
restore user customizations. Add an ordered, targeted migration and advance the
version only when config consumption or required sidebar settings change.
Future migrations must run only when newer than the completed version, preserve
unrelated user edits, and record success only after atomic persistence. Add
upgrade and rerun tests in `tests/test_configure.py`; existing first-run setup must
remain retryable after interruption without duplicating tokens or backups.

## Maintenance behavior

Discovery prefers the workspace's worktree checkout path, then the focused
pane's foreground directory or original cwd in its active tab. Git resolves the
tracking remote, falling back to `origin`; inherited `GH_REPO` is ignored.
Repeated checkout contexts share one lookup per cycle. Refresh rechecks the
workspace and branch before reporting to reject stale results.

Metadata belongs to source `herdr-pr-status`: `pr_number` contains only `#123`,
and one of `pr_open`, `pr_draft`, `pr_merged`, or `pr_closed` contains the styled
value and check icon. Reports clear inactive tokens atomically. Empty results
clear all five; failures retain prior values until the 180-second expiry.

One watcher runs per session socket identity. Refresh cycles do not overlap;
a colliding manual refresh exits successfully. Binary replacements and registry
path changes trigger watcher reload between cycles; failed replacements retry
while the current watcher continues. Stop, disable, unlink, and session loss
prevent reload. The launcher is not a crash supervisor.

State and append-only logs live under `HERDR_PLUGIN_STATE_DIR/session-*`.
The start action prints the log path. Remove old logs only while stopped.

## Releases

The [Release workflow](../.github/workflows/release.yml) uses Release Please
with the built-in `GITHUB_TOKEN`. In GitHub's **Settings → Actions → General →
Workflow permissions**, enable **Allow GitHub Actions to create and approve
pull requests**. Require CI checks on `main` through branch protection.

Tracking starts after `fe743a602f9d252677afc29c8edb83c0ac7324bf`, using `0.1.0`
as the baseline. The automation setup alone does not request a release.
The next `fix:` or `feat:` commit on `main` starts the flow:

1. Land changes with Conventional Commit titles (`fix:` for patches, `feat:`
   for features, `!` for breaking changes). Use the same format for squash
   commit titles. Before 1.0, breaking changes bump the minor version;
   `docs:` and `chore:` alone do not request a release.
2. Review the generated release PR's versions and changelog. Approve workflow
   runs if GitHub requests it, and wait for CI to pass.
3. Merge preserving the generated title and release metadata. The workflow
   creates the version tag and GitHub Release. No manual version edits,
   crates.io publication, or binary upload are needed.

For example, the first `fix:` requests `0.1.1`, synchronizing Cargo and plugin
versions and the changelog; merging that release PR publishes `v0.1.1`.
For transient failures, rerun **Actions → Release → Run workflow** on `main`.
Source installations follow `main`, so merged changes are available through
`herdr plugin update herdr-pr-status` before a versioned release is published.
