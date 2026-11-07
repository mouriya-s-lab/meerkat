# Quorum

Quorum: one [NSH](https://nsh.dev) Rust app that runs as a
native macOS window and as a static web site. It owns no PM state; it reads
GitHub (the PM repository's Project, implementation issues and their marker
comments, the API rate limit) and the council and guardians programs
(`/v1/health`, `/v1/sessions`, `/v1/sessions/{id}/log`, `/v1/bases`,
`/v1/queue`), shows what it read, and forwards one request: a base's
rewarm (`POST /v1/bases/{id}/rewarm`), which a person asks for and
confirms.

The pages:

- **概览** (overview): what needs a person now (failed or stale bases,
  sessions failed recently, failed agenda issues, unsettled escalations,
  unreachable programs, a low API allowance), then a card each for agenda
  issues, implementations, guardian bases, recent LLM errors and program
  health.
- **守护** (guardians): every base (guardian, repository, model, state,
  stale and why, when it was warmed, error) with a 重新预热 button,
  disabled while the base warms. The button asks for confirmation naming
  the guardian, repository and model; the guardians' answer (202 started,
  409 warming, 404 no such base, or the error) shows on the row and the
  bases load again. Selecting a base lists the sessions forked from it
  (`/v1/sessions` rows whose `parent` is the base's session).
- **会话** (sessions): the sessions of both programs in one list, filtered
  by program, kind and state. Opening one fetches its record and shows its
  decision log, reduced by `dashboard.yaml`'s `digest` rules: the opening
  message, the program's later prompts, kept tools' calls with arguments and
  results, a count per run of a folded tool, the assistant's text when
  configured, and highlighted LLM errors, automatic retries and
  continuation prompts; thinking never shows. 原始记录 shows the record
  line by line. On macOS the record is parsed on a background thread; the
  web build, which has no threads, parses it on the page's thread after the
  frame that says so.

Every source loads, fails and refreshes on its own; the top bar shows the
last refresh, how many sources fail, what is left of the API allowance,
and a button that refreshes everything now.

The second page, 「议题」, draws the agenda issues (Project `Kind` human or
agenda in the PM repository) as a DAG of their GitHub "Blocked by"
relations, or, switched with 图/列表, as a table of the same issues. A
node's state is the council's `/v1/queue` verdict; while the council is
unreachable it is the Project's `Status`, marked 「议会不可达」. Layers run
left to right along the longest chain of prerequisites; a "Blocked by" the
council does not count is drawn dashed. Issues with no prerequisite
relation at all sit in a grid under the drawing, in agenda order. Pressing
a node selects it: its prerequisites and what it blocks stay, the rest
dims, and the panel beside the drawing shows its panel, votes and
relations with a button that opens it on GitHub; a table row selects and
opens it. Drag pans the DAG; the buttons, Control + wheel and trackpad
pinches zoom it.

Every page draws from one design system: the colors, type sizes,
spacings, radii and DAG lengths of `dashboard/theme.json`, and one
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
| `list.columns` | The table's columns, left to right, each once: `agenda`, `title`, `kind`, `state`, `proposer`, `blockedBy`, `progress` |
| `list.defaultSort` | `{column, descending}`: the table's order when the page opens; `column` is one of `list.columns` |
| `filters.hideDecided`, `filters.hideSuperseded`, `filters.humanOnly` | The filters' values when the app starts (`humanOnly`: human issues and the issues they block, transitively) |

`dashboard/theme.json` holds the design tokens: `cssVars.theme` (fonts
and lengths), `cssVars.light` and `cssVars.dark` (colors), each with
exactly the tokens of `config.example/dashboard/theme.json`. Colors are
`#rrggbb`, lengths `<n>px`, fonts CSS family lists. A missing token, an
unknown one or a value that does not parse is an error that names the
file, the token's key path (`cssVars.light.good-text`) and what is wrong;
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
  parser, and embeds the three files. A bad config fails the build with the
  key path. Rebuild to change it.

## macOS

Needs Rust (cargo) and Xcode's command line tools.

```sh
PM_CONFIG_DIR=$PWD/config.example cargo run
```

The GitHub token is `GH_TOKEN`, or what `gh auth token` prints when
`GH_TOKEN` is empty. Windows at least 900 pt wide show a sidebar beside the
page; narrower ones show a single column.

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
| `src/lib.rs` | The root component `App` and the NSH module |
| `src/main.rs` | `cargo run`: the macOS window |
| `src/config/` | The config files as types (`schema.rs`, shared with `build.rs`) and how each build loads them |
| `src/gh/` | The GitHub reader: Project items (GraphQL), comments, PM markers, open escalations, rate limit |
| `src/programs/` | The program reader: health, sessions, session records, bases, rewarm, queue |
| `src/digest.rs` | The decision log: a session record reduced by the `digest` rules (tests over real session files in `tests/fixtures/`) |
| `src/sources.rs` | One `resource` per source, refreshed together |
| `src/attention.rs` | What needs a person, derived from the sources |
| `src/dag.rs` | The 「议题」 model: the issue DAG, its layers and filters, the table's order |
| `src/ui/` | The shell, the pages (overview, 「议题」, guardians, sessions) and the app's own controls |
| `deploy/Caddyfile` | The web build's same-origin proxy |
| `config.example/` | A PM config directory for local runs |
