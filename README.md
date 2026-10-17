> [!WARNING]
> This plugin is deprecated in favor of [herdrpp](https://github.com/kamaal111/herdrpp),
> which includes the PR status feature.

# Herdr PR Status

Show the newest associated GitHub pull request beside each workspace in Herdr's
expanded desktop sidebar. PR numbers are green for open, gray for draft, purple
for merged, and red for closed. Checks appear as ✅ passed, ❌ failed, or 🟡 pending.
Merged PRs and PRs without actionable checks show no check icon.

## Installation

Requires macOS or Linux, Herdr 0.9.5 or later, Git, Rust/Cargo (Rust 1.89 minimum),
curl, Python 3, and authenticated [GitHub CLI](https://cli.github.com/).
Run on the computer hosting both the Herdr server and viewing client:

```sh
curl -fsSL https://raw.githubusercontent.com/kamaal111/herdr-pr-status/main/install.sh | bash
```

The installer resolves the latest published stable release, installs that exact
tag through Herdr, initializes the local sidebar configuration, and requests
watcher startup if a session is running. Then select **Reload config** in the
viewing client's menu. If no session is running, the startup hook activates the
watcher when the server starts. Authenticate `gh` on the server with
`gh auth login`; the installer never changes your authentication.

Run the same command again to install the newest release. Configuration is set
up **once per config path**, tracked outside `config.toml`. Repeated installs
leave it untouched, even if you later edit/remove tokens or delete the file.
Only a new explicit config migration can modify it again; ordinary plugin
releases do not trigger config edits. Existing rows, styles, comments, and
unrelated settings are preserved; legacy `$pr_number` tokens are upgraded.
Invalid or incompatible layouts cause an error without changing the file.

`HERDR_CONFIG_PATH` selects a different config; `HERDR_BIN_PATH` selects a
different Herdr executable. Migration records and backups live under
`${XDG_STATE_HOME:-$HOME/.local/state}/herdr-pr-status/configs/`, outside the
plugin installation. Keep these records to retain the once-only guarantee.

The installer requires a published release containing the `configure` command.
Older releases such as `0.1.0` require the manual sidebar setup below.

### Remote server and separate viewing computer

On the **server**, install without touching its sidebar configuration:

```sh
curl -fsSL https://raw.githubusercontent.com/kamaal111/herdr-pr-status/main/install.sh | bash -s -- --plugin-only
```

On the **viewing computer**, configure without registering a plugin:

```sh
curl -fsSL https://raw.githubusercontent.com/kamaal111/herdr-pr-status/main/install.sh | bash -s -- --config-only
```

Config-only setup needs curl, Python 3, tar, and Rust/Cargo: it temporarily builds
the release's config helper and removes the temporary source/build afterward.
It needs no GitHub authentication, `gh`, or running server. On Windows clients,
use the manual sidebar instructions below.

Use `bash install.sh --help` or `bash install.sh --dry-run` from a checkout to
inspect the options or preview setup without making changes.

### Manual installation

For manual source installation, run on the Herdr server computer with `git` and
authenticated `gh` on its PATH:

```sh
gh auth login
herdr plugin install kamaal111/herdr-pr-status
herdr plugin action invoke herdr-pr-status.start
```

The start action activates the watcher in an already running session. After
server restarts, the startup hook starts it automatically. Closing or reconnecting
a client does not stop it. For multiple sessions, invoke the action in each one:

```sh
herdr --session work plugin action invoke herdr-pr-status.start
```

To update a manual installation, run on each server computer:

```sh
herdr plugin update herdr-pr-status
```

Manual installations without `--ref` build from source and follow `main`.
The scripted installer pins a release tag; rerun the script to select a newer
release (`plugin update` alone follows the previously installed ref).
Running watchers detect
updates between refresh cycles. If switching from a local development link,
run `herdr plugin unlink herdr-pr-status` before installing; your source files
and sidebar configuration are preserved.

## Sidebar configuration

Edit the configuration on the **computer displaying the Herdr client**, including
when connected remotely. Use `~/.config/herdr/config.toml` on macOS/Linux or
`%APPDATA%\herdr\config.toml` on Windows (`HERDR_CONFIG_PATH` overrides these).

Merge the four styled tokens into your existing `[ui.sidebar.spaces]` rows.
If you have no custom layout, use:

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

Select **Reload config** in that client's in-app menu. The server CLI reload
command does not reload a remote client's layout. See
[example configuration](examples/herdr.toml).

Existing `$pr_number` layouts show the plain number; replace that token with
the four styled tokens to enable colors and check icons. Missing tokens and
their separators disappear automatically. Collapsed and mobile layouts are
controlled by Herdr.

## Actions

```sh
herdr plugin action invoke herdr-pr-status.refresh  # one immediate refresh
herdr plugin action invoke herdr-pr-status.stop     # stop automatic refresh
herdr plugin action invoke herdr-pr-status.start    # resume after stopping
```

Refresh does not start or restart a watcher. No action edits Herdr's config.
Action invocations are asynchronous; a `running` response is not confirmation
of completion. Inspect completed logs with:

```sh
herdr plugin log list --plugin herdr-pr-status --limit 5
```

## Behavior and limitations

- The newest PR by creation date wins, across all states, matching the exact
  head repository and branch. Linked worktrees are independent.
- Supports GitHub and GitHub Enterprise hosts authenticated through `gh`.
  Detached HEAD, non-Git directories, and local-only remotes have no association.
- Refresh runs immediately, then waits 60 seconds between completed cycles.
  Lookups run sequentially; slow repositories can delay updates.
- Metadata expires after 180 seconds unless renewed. Lookup failures preserve
  the previous value until expiry; confirmed empty results clear it.
- All reported checks count, including optional checks. Failure wins over pending,
  then passing; neutral and skipped checks are ignored.
- After stop, disable, or unlink, displayed tokens expire within 180 seconds.
  Run the start action after re-enabling the plugin or recovering an exited watcher.

## Troubleshooting

- **PR missing:** check the viewing client's config and expanded sidebar first.
  Expand repository groups to see their worktrees.
- **Metadata or layout?** Run `herdr workspace list`, then
  `herdr workspace get WORKSPACE_ID` on the server. If `pr_number` already
  contains a value such as `#95`, check the client layout and reload its config.
- **No metadata:** run `just plugin-check` from a development checkout
  with Herdr session context, or `./target/release/herdr-pr-status check` from
  the installed plugin directory.
  Verify the branch, GitHub remote, and `gh` access, then invoke refresh.
- **Watcher errors:** inspect completed action logs. The start log prints the
  `watch.log` path; check it for per-workspace failures. Fix the cause and invoke
  start if the watcher exited.
- **Local checkout:** follow the [development setup](docs/development.md#local-plugin).

## Development

See [development and releases](docs/development.md) and
[integration and snapshot testing](docs/snapshots.md).

## License

Project code is [MIT licensed](LICENSE). Snapshot fonts use the
[Bitstream Vera license](tests/snapshots/fonts/LICENSE) (DejaVu Sans Mono) and
[SIL Open Font License](tests/snapshots/fonts/NotoEmoji-LICENSE.txt) (Noto Emoji).
