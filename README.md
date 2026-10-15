# Herdr PR Status

Show the newest associated GitHub pull request number beside each workspace in
Herdr’s expanded desktop sidebar. A workspace with a PR displays `#123`; a
workspace without one displays nothing extra. PR numbers are green for open,
gray for draft, purple for merged, and red for closed without merging. CI checks
appear beside the number: ✅ passed, ❌ failed, or 🟡 pending. No checks means no icon.
Merged PRs show only the purple number, without a CI icon.

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
and therefore does not need Ratatui or picker navigation tests. Real Herdr sidebar
PNG snapshots verify the rendered integration.

## Build and activate

Run this once from the checkout **on the Herdr server computer**. It builds the
release binary and registers the plugin:

```sh
just plugin-link
```

For remote use, the plugin, `git`, and authenticated `gh` belong on the server;
the sidebar configuration below belongs on the computer displaying the client.

**Edit the config on the computer displaying the Herdr sidebar.** When connected
remotely, installing the plugin on the server and editing the server’s config
are not enough: sidebar layouts come from the viewing computer’s local config.
Use `~/.config/herdr/config.toml` on macOS/Linux or
`%APPDATA%\herdr\config.toml` on Windows (`HERDR_CONFIG_PATH` overrides these).

Merge the four styled PR tokens into that config’s existing
`[ui.sidebar.spaces]` rows. If you have no custom layout, use:

```toml
[ui.sidebar.spaces]
rows = [
  [
    "state_icon", "workspace",
    { token = "$pr_open", fg = "#238636" },
    { token = "$pr_draft", fg = "#8b949e" },
    { token = "$pr_merged", fg = "#a371f7" },
    { token = "$pr_closed", fg = "#f85149" },
  ],
  ["branch", "git_status"],
]
```

See [examples/herdr.toml](examples/herdr.toml). Existing `$pr_number` layouts
continue to show the plain number; replace that token with the four styled tokens
to enable lifecycle colors and CI icons. Only the active lifecycle token is present.
Reload configuration from Herdr’s menu. Layouts belong to the client’s local configuration, including when
attached remotely. Missing tokens and their separators disappear automatically;
collapsed and mobile layouts remain host-controlled.
See [Herdr sidebar configuration](https://herdr.dev/docs/configuration/).

Start automatic refresh in an already running session:

```sh
just plugin-start
```

**You do not run `plugin-start` every time you open Herdr or start a server.**
The startup hook starts the watcher automatically after each server startup.
Installing/linking into an already-running server does not run that hook, so
`just plugin-start` activates it once without waiting for the next server start.
Reloading sidebar configuration does not require restarting the plugin.

| Situation | What to do |
| --- | --- |
| First install/link while the server is running | Run `just plugin-start` once. |
| Herdr server starts or restarts | Nothing; the startup hook runs automatically. |
| Client closes, reopens, or reconnects | Nothing; the server-side watcher keeps running. |
| Sidebar config changes | Reload config in the viewing client; keep the watcher running. |
| Plugin was stopped, disabled/re-enabled, or watcher exited | Run `just plugin-start` to resume. |

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

## PR number not visible?

1. **Check the viewing computer’s config first.** This is especially important
   with `herdr --remote`: changing only the server’s config does not change the
   client’s sidebar. Run `herdr --help` on the viewing computer, outside any SSH
   shell, to see its resolved config path. Add the styled PR tokens there and select
   **Reload config in that client’s in-app menu**. The server CLI reload command
   does not reload the remote client’s local sidebar layout.
2. **Use the expanded desktop sidebar.** Collapsed/mobile views do not render
   these custom rows. Expand a collapsed repository group to see its worktrees.
3. **Check whether metadata exists on the server.** Run `herdr workspace list`
   to find the workspace ID, then `herdr workspace get WORKSPACE_ID`. A token
   such as `"pr_number": "#95"` means the association has already been reported;
   focus on the viewing client’s config and layout rather than restarting the
   watcher.
4. **If metadata is absent, check discovery and refresh on the server.** Run
   `just plugin-check` for read-only association results and errors, then
   `just plugin-refresh` to request a refresh. Ensure `gh` can access the
   repository, the checkout is on a branch, and its remote points to GitHub.
5. **Inspect completed action logs and watcher diagnostics.** Run:

   ```sh
   herdr plugin log list --plugin herdr-pr-status --limit 5
   ```

   Actions run asynchronously: an invocation response with `status: running`
   is not confirmation that the action finished successfully. The completed
   start log prints the `watch.log` path. Inspect that file for per-workspace
   errors and a summary such as `12 workspace reports, 0 failures`. If the
   watcher exited, run `just plugin-start` after fixing the reported cause.

Allow one refresh cycle for new workspaces or branch changes: the watcher waits
60 seconds between completed cycles. `just plugin-refresh` requests an immediate
cycle; it does not restart the watcher. Starting it again is unnecessary when
metadata is already present.

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

After selecting the newest PR, `gh pr view` reads its lifecycle and current
check rollup from the resolved base repository. All reported checks count,
including optional checks. Merged PRs omit the CI icon. For other lifecycle
states, any failure wins over pending checks; pending wins over passing. Cancellation, timeout, action required,
stale, and startup failures count as failures. Neutral and skipped checks are
ignored; if no actionable checks remain, no CI icon appears.

Metadata uses only source `herdr-pr-status` and tokens `pr_number`, `pr_open`,
`pr_draft`, `pr_merged`, and `pr_closed`. The compatibility token `pr_number`
contains only `#123`; the active lifecycle token contains `#123` and any CI icon.
Each successful report clears inactive lifecycle tokens atomically. Confirmed
empty results clear all five owned tokens. Successful values expire after 180
seconds and are renewed each cycle. Authentication, network, malformed-data, and command
failures preserve the previous value until expiry and are retried on the next
cycle. Other workspaces continue refreshing when one fails. With many slow
repositories, a refresh cycle may exceed the metadata TTL.

The startup command is a short-lived launcher; the detached `watch` entrypoint
owns an OS file lock scoped to the socket path and its filesystem identity.
Duplicate starts reuse the watcher only when it belongs to the same executable.
Starting after an update or relink replaces outdated watchers, including older
versions without a recorded identity, and immediately refreshes PR metadata.
Binary replacements and changed install paths in Herdr’s plugin registry are
detected automatically between refresh cycles. The watcher reloads the
registered executable while keeping the session singleton, even when the old
managed checkout has been removed.
If the updated executable cannot be launched yet, the current watcher keeps
running and retries. Stop, disable, unlink, and session shutdown prevent reload.
Between cycles the watcher blocks on a local wakeup pipe for up to 60 seconds;
it does not wake every second or query Herdr every five seconds. Explicit stop
and replacement requests wake it immediately. Binary updates, disable/unlink,
and socket replacement/removal are detected at the next cycle (up to 60 seconds
while idle). It exits on disable/unlink or loss of access to the session API.
There is one blocking wait per session, owned by the process holding the session
lock. An update replaces that process rather than adding another timer. The OS
closes its descriptors on exit or successful reload; normal shutdown also removes
`wake.pipe` and `watcher.json`. A subsequent watcher replaces any stale pipe.
A stop/disable during a lookup takes effect after the bounded command finishes.
Stopped tokens expire within 180 seconds. Re-enable or recover the session API
with `start` again; the launcher is not an OS service or crash supervisor.

Logs and lock/stop files live beneath `HERDR_PLUGIN_STATE_DIR/session-*`.
`start` prints the `watch.log` path. Logs include per-workspace errors and cycle
counts and automatic update handoffs. `watcher.json` records the running process
and executable identity. Lock files intentionally remain on disk; the OS releases locks when a
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

### Real sidebar snapshot tests

With Docker running:

```sh
just test-snapshots
just test-snapshots --repeat 2
just record-snapshots
```

The pinned Linux x86_64 container runs real Herdr 0.9.5 servers, terminal clients,
Git repositories, linked worktrees, and the release plugin. Only GitHub is faked.
It captures terminal cells into PNGs with pinned fonts and checks committed
baselines. Recording is explicit and disabled in CI. Existing host sessions,
configuration, and plugin registrations are never used.

Nine cases verify grouped `export-import · #95`, a separate client config missing
the token (metadata exists but the PR is invisible), the client config containing
the token, adding the token and reloading the client, clearing the PR, and
draft/pending, open/failing, merged/passing, and closed/no-check displays. These
use independently configured clients and servers over temporary local sockets;
they exercise configuration ownership without requiring an SSH server.

CI compares all nine twice and uploads actual/expected/difference PNGs, screen
text, terminal ANSI output, and server logs. Local diagnostics live under
`target/snapshots/CASE/`. Review baseline images before committing. Snapshot
comparison is a separate required check alongside `just ready`.

### Manual integration check before release

Use a disposable Herdr session and isolated configuration/state directories.
Link this checkout there and add the example sidebar tokens. Open one workspace
whose branch has a known PR, one linked worktree with another PR, and one branch
without a PR. Start the plugin and confirm the first two show the expected
numbers while the third has no extra separator. Confirm a closed or merged PR
still appears, a branch switch updates on the next refresh, and manual refresh
updates immediately. Disable/unlink the plugin and confirm the watcher exits
and the token disappears within 180 seconds. Check narrow/collapsed sidebar
behavior is controlled by Herdr. Shut down and remove the disposable session.

Live GitHub authentication/API behavior and the actual remote transport still
require this manual check. Sidebar rendering and client-config ownership are
covered by the isolated snapshot suite.

Snapshot fonts are DejaVu Sans Mono 2.37, redistributed under the
[Bitstream Vera license](tests/snapshots/fonts/LICENSE), and Noto Emoji, under the
[SIL Open Font License](tests/snapshots/fonts/NotoEmoji-LICENSE.txt). Noto Emoji
renders the CI symbols as monochrome glyphs in PNGs; terminals may render them
as colored emoji.
