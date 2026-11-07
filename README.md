# Quorum

Quorum: one [NSH](https://nsh.dev) Rust app that runs as a
native macOS window and as a static web site. It owns no PM state; it reads
GitHub (the PM repository's Project, implementation issues and their marker
comments, the API rate limit) and the council and guardians programs
(`/v1/health`, `/v1/sessions`, `/v1/sessions/{id}/log`, `/v1/bases`,
`/v1/queue`), shows what it read, and forwards one request: a base's
rewarm (`POST /v1/bases/{id}/rewarm`), which a person asks for and
confirms.

Windows at least 900 pt wide show a sidebar: Quorum's name, a search field
that opens the command bar, the pages with a count each (概览: what needs
you; 议题: open agenda issues; 会话: running sessions; 运行状况: failed and
stale bases), and at its foot the two programs with their versions (a Go
pseudo-version as its 12-digit commit, the full version in its tooltip)
and what is left of the GitHub API allowance. Narrower windows show the pages
as tabs in a top bar; there 议题, 会话 and 运行状况 show their list alone,
and what is selected opens full width under a 返回 bar instead of in an
inspector beside the list. Each page opens under a toolbar with its title, how
long ago its data loaded and a refresh button. ⌘K opens the command bar
(agenda issues, sessions and commands, picked with ↑↓ and ↵), ⌘1–⌘4 open
the pages and ⌘R refreshes every source.

The pages:

- **概览** (overview): one sentence on how things run, then 需要你处理,
  what needs a person in order of consequence: failed agenda issues,
  unreachable programs, failed bases, failed sessions grouped by program
  and kind (expand a group for its errors, counted by first line), asks
  escalated to an agenda issue, stale bases and a low API allowance; each
  with the action that settles it. Under it the council's queue of open
  agenda issues; beside it panels for the programs, the last day's
  sessions by hour, the guardian bases and the sessions' errors.
- **议题** (agenda issues: Project `Kind` human or agenda in the PM
  repository), as a list or a dependency graph of the same issues,
  filtered by state, by whether decided issues fold, show or hide, and by
  human issues and what they block, and searched by agenda number or
  title. A node's state is the council's `/v1/queue` verdict; while the
  council is unreachable it is the Project's `Status`, marked
  「议会不可达」. The list groups issues by what they need (需要处理, 在审,
  可召集, 等待, 已定案) with their progress (votes of the round, share of
  sub-issues done, a failure's note) and what they wait for or block; it
  shows the latest three decided issues and a line for the rest. The
  graph puts the open issues in columns by the "Blocked by" relations the
  council counts: 可推进 first, then each column waiting on the one
  before; decided issues fold into one line naming their chains.
  Selecting an issue keeps it and the issues next to it bright, dims the
  rest when it has any, and the inspector shows its round, why it has
  not started, its prerequisites, what it blocks, the "Blocked by" the
  council does not count and buttons that open it. Drag pans the graph;
  the zoom bar, Control + wheel and trackpad pinches zoom it.
- **会话** (sessions): the sessions of both programs in one table with
  their count by hour, filtered by state, program and kind and searched by
  issue, model or error. Selecting one shows its facts, its error, and its
  decision log, reduced by `dashboard.yaml`'s `digest` rules: the opening
  message, the program's later prompts, kept tools' calls with arguments
  and results, a count per run of a folded tool, the assistant's text when
  configured, and highlighted LLM errors, automatic retries (a run of the
  same error folded into one line) and continuation prompts; thinking
  never shows. 仅异常 keeps the abnormal steps; JSON shows the record line
  by line. On macOS the record is parsed on a background thread; the web
  build, which has no threads, parses it on the page's thread after the
  frame that says so.
- **运行状况** (runtime): the guardian bases grouped by repository
  (model, state, why a base is stale, when it was warmed, its forks in the
  last day), the programs (state, full version, uptime, work, last check) and
  the limits (GitHub REST and GraphQL allowance, sessions that hit an LLM
  rate limit). A hovered or selected base shows 重新预热, disabled while
  the base warms. The button asks for confirmation naming the guardian,
  repository and model; the guardians' answer (202 started, 409 warming,
  404 no such base, or the error) shows on the row and the bases load
  again. The inspector lists the sessions forked from the selected base
  (`/v1/sessions` rows whose `parent` is the base's session).

Every source loads, fails and refreshes on its own: a panel whose source
fails shows the error with a retry button and keeps the data it had.

Every page draws from one design system: the colors, type sizes,
spacings, radii and graph lengths of `dashboard/theme.json`, and one
vocabulary of state words and glyphs shared by all pages.

## Configuration

Both builds read the PM config directory (the layout shared with
pm-council and pm-guardians):

```
<config dir>/
  dashboard/dashboard.yaml
  dashboard/theme.json
  shared/repos.yaml
  shared/project.yaml
```

`config.example/` is a complete directory for local runs against the e2e
repositories.

`dashboard/dashboard.yaml`:

| Key | Meaning |
|---|---|
| `targets.macos.githubApi` | GitHub API root for the macOS app (`https://api.github.com`) |
| `targets.macos.council` | Council daemon address for the macOS app |
| `targets.macos.guardians` | Guardians program address for the macOS app |
| `targets.web.githubApi` | GitHub API root for the web build: the proxy's `/gh` path, absolute |
| `targets.web.council` | The proxy's `/council` path, absolute |
| `targets.web.guardians` | The proxy's `/guardians` path, absolute |
| `refreshSeconds` | Seconds between two refreshes of every source (at least 1) |
| `attention.sessionFailureHours` | A failed session needs a person for this many hours after it ended |
| `attention.rateLimitPercent` | Less than this share (0–100) of a GitHub API limit left needs a person |
| `digest.keepTools` | Tools whose calls the decision log shows with their arguments and result (the council and guardians plugin tools) |
| `digest.foldTools` | Tools whose consecutive calls the decision log shows as one count (the file-reading tools) |
| `digest.showAssistantText` | Whether the decision log shows the assistant's text |
| `digest.maxBytes` | A session record longer than this many bytes is parsed from its tail only, and the decision log says it is cut |
| `filters.hideDecided` | The 「已定案」 filter when the app starts: `true` folds the decided and superseded issues (the list shows the latest three, the graph one line), `false` shows them all |
| `filters.humanOnly` | The 「仅人类议题」 filter when the app starts: human issues and the issues they block, transitively |
| `appearance` | `system` follows the system's light or dark appearance (the browser's on the web), `light` or `dark` keeps that one whatever the system's; the macOS window, title bar included, opens in it |

`dashboard/theme.json` holds the design tokens: `cssVars.theme` (fonts
and lengths), `cssVars.light` and `cssVars.dark` (colors), each with
exactly the tokens of `config.example/dashboard/theme.json`. Colors are
`#rrggbb`, lengths `<n>px`, fonts CSS family lists. A missing token, an
unknown one or a value that does not parse is an error that names the
file, the token's key path (`cssVars.light.ok-text`) and what is wrong;
the web build checks the same file when it is built.

Addresses are absolute `http(s)` URLs without a query; NSH's `fetch` takes
absolute URLs only, so the web addresses name the proxy's origin.

`shared/repos.yaml` gives the PM repository and its Project
(`pm.repo`, `pm.project.owner`, `pm.project.number`) and the code, design
and omp repositories; `shared/project.yaml` gives the Project's field and
option names (`fields.status`, `fields.kind`, `fields.agenda`,
`fields.proposer`). Unknown keys and missing keys are errors that name the
file and the key path, for example
``dashboard/dashboard.yaml: missing field `refreshSeconds` ``.

### How each build gets it

- **macOS** reads the directory `PM_CONFIG_DIR` names when it starts. A
  missing variable, file or key shows a configuration error in the window.
- **Web** carries the config in the WebAssembly module: `build.rs` reads the
  directory `PM_CONFIG_DIR` names at build time, checks it with the same
  parsers, and embeds its four files. A bad config fails the build with the
  key path. Rebuild to change it.

## macOS

Needs Rust (cargo) and Xcode's command line tools.

```sh
PM_CONFIG_DIR=$PWD/config.example cargo run
```

The GitHub token is `GH_TOKEN`, or what `gh auth token` prints when
`GH_TOKEN` is empty.

## Web

Needs the [`nsh` CLI](https://nsh.dev/docs/installation.md)
(`brew install longbridge/tap/nsh`), `rustup target add
wasm32-unknown-unknown`, LLVM's `clang` and `llvm-ar` on `PATH`
(`brew install llvm`; Homebrew puts them in `/opt/homebrew/opt/llvm/bin`),
and the `wasm-bindgen` CLI at the version `nsh` names (the web build reads
the browser's time zone through jiff's `js` feature, which uses
wasm-bindgen; `nsh package` prints the exact `cargo install` line when the
CLI is missing).

```sh
PATH=/opt/homebrew/opt/llvm/bin:$PATH PM_CONFIG_DIR=$PWD/config.example \
  nsh package --target web -o /tmp/quorum-web
```

The site holds no credential. It reaches GitHub and the two programs through
a same-origin reverse proxy, `deploy/Caddyfile`, which adds the GitHub token
from its own environment. The token may be able to write; the proxy passes
reads to GitHub, and one kind of write: a `PUT` of a single file under the
config repo `CONFIG_REPO` (the contents API, with `sha` to update). To run it
locally (with the example config's web addresses):

```sh
LISTEN=:8088 SITE_DIR=/tmp/quorum-web GH_TOKEN=<token> \
  CONFIG_REPO=oh-my-omp/pm-config COUNCIL_URL=http://127.0.0.1:7421 \
  GUARDIANS_URL=http://127.0.0.1:7410 \
  caddy run --config deploy/Caddyfile --adapter caddyfile
```

then open `http://127.0.0.1:8088/`. The proxy maps `/gh/*` to
`https://api.github.com` with `Authorization: Bearer $GH_TOKEN` for reads
only: REST `GET` and `HEAD`, and `POST /gh/graphql` whose body contains
neither `mutation` (any case) nor a `\u` escape. The one write is
`PUT /gh/repos/$CONFIG_REPO/contents/<path>`, with a path free of `..`, `%`,
`//` and backslashes; with `CONFIG_REPO` unset no write is passed. Every
other `/gh/*` request gets `403` with an empty body and never reaches GitHub.
`/council/*` goes to
`COUNCIL_URL` and `/guardians/*` to `GUARDIANS_URL` (every method, so a
rewarm's `POST` goes through too), and `SITE_DIR` is served for everything
else.

## Layout

| Path | Holds |
|---|---|
| `src/lib.rs` | The root component `App` (config, appearance, tokens) and the NSH module |
| `src/main.rs` | `cargo run`: the macOS window, opened in the config's appearance |
| `src/config/` | The config files as types (`schema.rs`, shared with `build.rs`; `theme.rs`, the design tokens) and how each build loads them |
| `src/gh/` | The GitHub reader: Project items (GraphQL), comments, PM markers, open escalations, rate limit |
| `src/programs/` | The program reader: health, sessions, session records, bases, rewarm, queue |
| `src/digest.rs` | The decision log: a session record reduced by the `digest` rules (tests over real session files in `tests/fixtures/`) |
| `src/sources.rs` | One `resource` per source, refreshed together |
| `src/attention.rs` | What needs a person, in order of consequence, failed sessions grouped by program and kind |
| `src/dag.rs` | The 「议题」 model: the issue graph, its groups, readiness columns, folded decided issues and filters |
| `src/time.rs` | Timestamps, "how long ago" and hourly counts |
| `src/ui/shell.rs` | The sidebar or top bar, the toolbar, the shortcuts, a list beside its inspector or (narrow) the inspector full width under 返回, and the configuration error |
| `src/ui/command.rs` | The command bar (⌘K) |
| `src/ui/overview.rs`, `issues.rs`, `sessions.rs`, `runtime.rs` | The four pages |
| `src/ui/parts.rs`, `status.rs`, `icons.rs`, `tokens.rs` | The app's own controls, its state words and glyphs, icons, and the theme's tokens as NSH tokens and type roles |
| `src/ui/text.rs` | Every word the UI shows |
| `deploy/Caddyfile` | The web build's same-origin proxy |
| `config.example/` | A PM config directory for local runs |
