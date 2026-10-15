# Herdr PR Status

Show the newest associated GitHub pull request number beside each workspace in
Herdr’s expanded desktop sidebar. A workspace with a PR displays `#123`; a
workspace without one displays nothing extra.

Open, draft, closed, and merged PRs are all included. If a branch has several
PRs, the most recently created PR wins, regardless of its state. Association is
by exact head repository and branch, not by commit ancestry.

## Requirements

- macOS or Linux, Herdr 0.9.5 or later
- Rust 1.89 or later (the checked-in toolchain selects stable)
- `git` and authenticated [GitHub CLI](https://cli.github.com/), available on the
  Herdr server’s PATH; run `gh auth login` for the relevant host
- [Just](https://just.systems/) for convenience commands, Python 3 for smoke tests

This repository follows the Cargo, Just, rustfmt, Clippy, release build, and
macOS/Linux CI conventions of `herdr-picker`. It has no custom terminal renderer
and therefore does not need Ratatui, picker navigation tests, or PNG baselines.

## Build and activate

The following are opt-in commands. Building and testing do not change your
Herdr settings or register the plugin.

```sh
just ready
just plugin-link
```

Merge `$pr_number` into your existing Herdr `[ui.sidebar.spaces]` rows. If you
have no custom layout, use:

```toml
[ui.sidebar.spaces]
rows = [
  ["state_icon", "workspace", "$pr_number"],
  ["branch", "git_status"],
]
```

See [examples/herdr.toml](examples/herdr.toml). Reload configuration from Herdr’s
menu. Layouts belong to the client’s local configuration, including when
attached remotely. Missing tokens and their separators disappear automatically;
collapsed and mobile layouts remain host-controlled.
See [Herdr sidebar configuration](https://herdr.dev/docs/configuration/).

Start automatic refresh in an already running session:

```sh
just plugin-start
```

The startup hook starts it automatically after future session restores. Linking
alone should not be relied on to start the watcher in an existing session.
Commands launched through plugin actions inherit the session socket and plugin
state directory. For multiple sessions, invoke the action in each desired
session, using `herdr --session NAME plugin action invoke herdr-pr-status.start`.

```sh
just plugin-refresh   # one refresh, without starting a watcher
just plugin-check     # read associations; no metadata or state writes
just plugin-stop      # request watcher shutdown
just plugin-unlink    # unregister; leave checkout files intact
```

The equivalent qualified actions are `herdr-pr-status.start`,
`herdr-pr-status.refresh`, and `herdr-pr-status.stop`. The `check` command can
also be run directly as `./target/release/herdr-pr-status check` in a shell with
Herdr session context. No command edits Herdr’s configuration.

## Discovery and refresh

The plugin reads `herdr api snapshot`, preferring a workspace’s worktree
`checkout_path`. Otherwise it uses the foreground directory (or original cwd)
of the focused pane in the workspace’s active tab. It does not infer a project
from a workspace label or scan unrelated directories.

Git resolves the checkout’s current branch and its configured tracking remote,
falling back to `origin`. A tracking branch’s remote name is used when it differs
from the local branch name. `gh repo view` resolves the base repository using
GitHub CLI’s local repository configuration. The plugin ignores an inherited
`GH_REPO` override so each checkout resolves independently. PR lookup requests
all states and pages and verifies the exact head repository and branch before
choosing the newest creation date. Standard SSH and HTTPS Git remote URLs are
supported, including GitHub Enterprise hosts authenticated through `gh`.

Linked worktrees are independent. Repeated workspace contexts for the same
checkout are looked up once per cycle. Non-Git directories, local-only remotes,
and detached HEAD have no association. Other hosting providers have no PR
integration; unresolved host/authentication errors are logged, not presented as
a successful empty GitHub response.

The watcher refreshes immediately, then waits 60 seconds after each completed
cycle. Lookups run sequentially without overlapping refresh cycles. It rechecks
the workspace and branch before reporting, clearing obsolete metadata if the
checkout changed during lookup. A manual refresh that collides with another
refresh reports that one is already running and exits successfully.

Metadata uses only source `herdr-pr-status` and token `pr_number`. Confirmed
empty results clear that token. Successful values expire after 180 seconds and
are renewed each cycle. Authentication, network, malformed-data, and command
failures preserve the previous value until expiry and are retried on the next
cycle. Other workspaces continue refreshing when one fails. With many slow
repositories, a refresh cycle may exceed the metadata TTL.

The startup command is a short-lived launcher; the detached `watch` entrypoint
owns an OS file lock scoped to the socket path and its filesystem identity.
Duplicate starts are harmless. The watcher checks for a stop marker and socket
replacement/removal, and checks installation/enabled status every five seconds
between cycles. It exits on disable/unlink or loss of access to the session API.
A stop/disable during a lookup takes effect after the bounded command finishes.
Stopped tokens expire within 180 seconds. Re-enable or recover the session API
with `start` again; the launcher is not an OS service or crash supervisor.

Logs and lock/stop files live beneath `HERDR_PLUGIN_STATE_DIR/session-*`.
`start` prints the `watch.log` path. Logs include per-workspace errors and cycle
counts. Lock files intentionally remain on disk; the OS releases locks when a
process exits. Log output appends across restarts, so remove old logs only while
the watcher is stopped if needed.

## Development and verification

```sh
just build
just format
just ready
```

`just ready` runs formatting checks, Clippy with warnings denied, Rust tests,
a release build, and Python smoke tests. Without Just:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo build --locked --release --target-dir target
python3 tests/smoke.py
```

Tests use temporary real Git repositories and linked worktrees, fake executable
Herdr/`gh` boundaries, and temporary Unix sockets. They require no network,
credentials, running Herdr server, or access to your actual plugin registry.
They verify selection, metadata ownership, clearing, directory fallback, stale
lookup rejection, failures, refresh serialization, watcher singleton/restart,
and shutdown on stop, disable, unlink, or session loss. Rust tests also verify
paginated responses, fork disambiguation, and bounded subprocess cleanup.

Herdr requests and Git commands time out after five seconds; GitHub repository
resolution after 15 seconds and PR listing after 20 seconds. Captured command
output is limited to 4 MiB per stream. Timeouts terminate and reap the child’s
process group. Arguments are passed directly, without shell interpolation.

### Manual integration check before release

Use a disposable Herdr session and isolated configuration/state directories.
Link this checkout there and add the example sidebar token. Open one workspace
whose branch has a known PR, one linked worktree with another PR, and one branch
without a PR. Start the plugin and confirm the first two show the expected
numbers while the third has no extra separator. Confirm a closed or merged PR
still appears, a branch switch updates on the next refresh, and manual refresh
updates immediately. Disable/unlink the plugin and confirm the watcher exits
and the token disappears within 180 seconds. Check narrow/collapsed sidebar
behavior is controlled by Herdr. Shut down and remove the disposable session.

Actual sidebar rendering and live GitHub authentication/API behavior require
this manual check; automated tests do not modify your live Herdr installation.
