# Quorum

Quorum: one [NSH](https://nsh.dev) Rust app that runs as a
native macOS window and as a static web site. It owns no PM state; it reads
GitHub (the PM repository's Project, implementation issues and their marker
comments, the API rate limit, and the files of the PM config repository's
branch) and the council and guardians programs (`/v1/health`,
`/v1/sessions`, `/v1/sessions/{id}/log`, `/v1/bases`, `/v1/queue`,
`/v1/config`), and shows what it read. It sends three kinds of request a
person asks for: a base's rewarm (`POST /v1/bases/{id}/rewarm`, after a
confirmation), the check of an edited config file by every program that
reads it (`POST /v1/config/check` to the council and the guardians,
`POST /pm-config/check` to the omp-dispatch launcher), and the commit of
that file to the config repository's branch once all of them pass.

Windows at least 900 pt wide show a sidebar: Quorum's name, a search field
that opens the command bar, the pages with a count each (概览: what needs
you; 议题: unsettled agenda issues; 实现: implementation issues waiting on
an answer; 会话: running sessions; 运行状况: failed and stale bases; 配置:
files a program runs an older version of than the branch has), and at its
foot the two programs with their versions (a Go pseudo-version as its
12-digit commit, the full version in its tooltip) and what is left of the
GitHub API allowance. Narrower windows show the pages as tabs in a top
bar; there 议题, 实现, 会话, 运行状况 and 配置 show their list alone, and
what is selected opens full width under a 返回 bar instead of in an
inspector beside the list. Each page opens under a toolbar with its title,
how long ago its data loaded and a refresh button. ⌘K opens the command
bar (agenda issues, sessions and commands, picked with ↑↓ and ↵), ⌘1–⌘6
open the pages and ⌘R refreshes every source.

The pages:

- **概览** (overview): one sentence on how things run (whether the
  programs answer, and while the council and its queue answer, whether
  the queue moves), then 需要你处理, what needs a person in order of
  consequence: failed agenda issues,
  unreachable programs, failed bases, failed sessions grouped by program
  and kind (expand a group for its errors, counted by first line), asks
  escalated to an agenda issue, stale bases and a low API allowance; each
  with the action that settles it. Under it the council's queue of
  unsettled agenda issues; beside it panels for the programs, the last
  day's sessions by hour, the guardian bases and the sessions' errors.
- **议题** (agenda issues: Project `Kind` human or agenda in the PM
  repository), as a list or a dependency graph of the same issues,
  filtered by state, by whether settled issues (decided, superseded or
  closed undecided) fold, show or hide, and by human issues and what
  they block, and searched by agenda number or title. A node's state is
  the council's `/v1/queue` verdict; while the council is unreachable,
  or when its queue does not list the issue, it is the Project's
  `Status`, marked 「议会不可达」 or 「议会队列未列出」.
  The list groups issues with a verdict by what they need (需要处理, 在审,
  可召集, 等待, 已了结) with their progress (votes of the round, share of
  sub-issues done, a failure's note) and what they wait for or block; it
  shows the latest three settled issues, each naming its state (已定案,
  已取代 or 已关闭), and a line counting the rest by state. After
  them, the issues showing their Project `Status` group under the
  option's name, each row naming its `Status` and why; the state filter
  names only the verdict groups, so these show under 全部 alone. While
  the queue fails to load, the page shows its error above the list. The
  graph puts the unsettled issues in columns by the "Blocked by"
  relations the council counts: 可推进 first, then each column waiting on
  the one before; settled issues fold into one line naming their chains.
  Selecting an issue keeps it and the issues next to it bright, dims the
  rest when it has any, and the inspector shows its state, the seat × row
  votes of its latest draft, why it has not started, its prerequisites,
  what it blocks, the "Blocked by" the council does not count and buttons
  that open its detail or GitHub. Drag pans the graph; the zoom bar,
  Control + wheel and trackpad pinches zoom it.
- **议题详情** (an agenda issue, opened from the list, the graph, the
  overview or any link to it; the toolbar's 议题 goes back): its state,
  number, kind, drafting seat and title; a track of its rounds and
  revisions, the selected step's seat × row matrix (✓ approve, ✕ oppose,
  ⊘ veto, ? counterexample, · not yet; a failed session's column hatched;
  a cell opens the vote's reason, counterexample, citation and
  classification), each row's result and threshold, a revision's row diff
  with its reason and the markers the council rejected; its body; the
  decision (until there is one, why: a panel that failed, was interrupted
  or waits for a prerequisite, with the error it posted, also ends its
  step on the track), rulings, its council sessions by round (each
  expands to its reduced decision log), its prerequisites, what it blocks
  and the implementation issues it created.
- **实现** (implementation issues: Project `Kind` implementation), grouped
  by what they wait on (等答复, 进行中, 待办, 完成), each with its
  repository, number, parent agenda issue, worker run and how long an open
  ask has waited. Selecting one shows its body or its marker timeline:
  worker runs and each ask framed with the guardian's reply, its
  replacement or its escalation to an agenda issue and the council's
  answer, the silent gaps between them, and open asks last.
- **会话** (sessions): the sessions of both programs in one table with
  their count by hour, filtered by state, program and kind and searched by
  issue, model or error. Selecting one shows its facts, its error, and its
  decision log, reduced by `quorum.yaml`'s `digest` rules: the opening
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
- **配置** (config): the files of the config repository's branch
  (`config.repo`, `config.branch`), grouped by top-level directory, each
  directory (or file, where they differ) with the programs that read it
  (`config.readers`), a file whose reader runs an older version marked ≠
  and a file with unsaved text marked ●. The selected file opens from the
  branch every time: 预览 cuts it by its top-level keys, each folding away;
  YAML is its whole text to edit; 改动 is its lines against the branch's,
  with each reader's errors under the line their key path points at. Once
  typing pauses, every reader of the file judges the edited file in the
  files it loads: the council and the guardians get the branch's files
  their `GET /v1/config` lists with the edited one in its place; the
  dispatch launcher gets `shared/repos.yaml`, `shared/project.yaml` and
  `dispatch/delivery.yaml`; Quorum parses its own four files with the same
  parsers it starts with. The rail shows each reader's conclusion (通过,
  the errors with their file, key path and reason, or 不可达 with why);
  「保存到 <branch>」 is enabled only when every reader passed this very
  text. A save commits the one file through the contents API
  (`quorum: edit <path>`, with the sha it was opened at) and links the
  commit; when the branch's file changed since it was opened, GitHub
  refuses (409), nothing is overwritten, and the page says so and opens the
  branch's file again. Under 正在用的版本 the council and the guardians
  (their `GET /v1/config` shas), and this Quorum (the shas it loaded), each
  read 「与 <branch> 一致」 or 「运行旧版」 with the files that differ; on
  macOS the local checkout `PM_CONFIG_DIR` shows how many commits it is
  behind `origin/<branch>` (after a `git fetch`) and 拉取 runs
  `git pull --ff-only` there. Unsaved text lives only in the window.

Every source loads, fails and refreshes on its own: a panel whose source
fails shows the error with a retry button and keeps the data it had.

Every page draws from one design system: the colors, type sizes,
spacings, radii and graph lengths of `quorum/theme.json`, and one
vocabulary of state words and glyphs shared by all pages.

## Language

Quorum speaks Chinese or English, following the oh-my-omp convention for
person-facing text: `OMOMP_LANG` (`en` or `zh`), then the first of
`LC_ALL`, `LC_MESSAGES` and `LANG` that is set (`zh…` is Chinese), then the
platform's preferred languages (macOS: the system's, web: the browser's;
Chinese when a `zh…` language comes before the first `en…` one), then
English. The web has no environment, so only the browser's languages
count there. When the browser's language changes, the page switches
without a reload. Any `zh…` reads Simplified Chinese; the app's name is
Quorum in both. Data (issue titles, repositories, models, seats) and the
errors programs, GitHub and the config parser give are shown as they are.
For example `OMOMP_LANG=en Quorum.app/Contents/MacOS/Quorum` opens it in
English.

## Configuration

Both builds read the PM config directory (the layout shared with
pm-council and pm-guardians):

```
<config dir>/
  quorum/quorum.yaml
  quorum/theme.json
  shared/repos.yaml
  shared/project.yaml
```

`config.example/` is a complete directory for local runs against the e2e
repositories.

`quorum/quorum.yaml`:

| Key | Meaning |
|---|---|
| `config.repo` | The PM config repository (`owner/name`) the 配置 page shows and commits to, e.g. `oh-my-omp/pm-config` |
| `config.branch` | The branch the 配置 page lists, compares what runs with and commits to (the deployed instance: `main`) |
| `config.readers` | Directory prefix (`council/`, ending in `/`) → the programs that read and check the files under it, each of `council`, `guardians`, `quorum`, `dispatch`, at least one, none twice; a file's readers are those of every prefix it starts with |
| `targets.macos.githubApi` | GitHub API root for the macOS app (`https://api.github.com`) |
| `targets.macos.council` | Council daemon address for the macOS app |
| `targets.macos.guardians` | Guardians program address for the macOS app |
| `targets.macos.dispatch` | The omp-dispatch launcher's PM config check listener (`configCheckHost:configCheckPort`, default `http://127.0.0.1:47593`) for the macOS app |
| `targets.web.githubApi` | GitHub API root for the web build: the proxy's `/gh` path, absolute |
| `targets.web.council` | The proxy's `/council` path, absolute |
| `targets.web.guardians` | The proxy's `/guardians` path, absolute |
| `targets.web.dispatch` | The proxy's `/dispatch` path, absolute; where the site cannot reach a launcher, dispatch's verdict is 不可达 and the files it reads (`dispatch/`, `shared/`) cannot be saved from the web |
| `refreshSeconds` | Seconds between two refreshes of every source (at least 1) |
| `attention.sessionFailureHours` | A failed session needs a person for this many hours after it ended |
| `attention.rateLimitPercent` | Less than this share (0–100) of a GitHub API limit left needs a person |
| `digest.keepTools` | Tools whose calls the decision log shows with their arguments and result (the council and guardians plugin tools) |
| `digest.foldTools` | Tools whose consecutive calls the decision log shows as one count (the file-reading tools) |
| `digest.showAssistantText` | Whether the decision log shows the assistant's text |
| `digest.maxBytes` | A session record longer than this many bytes is parsed from its tail only, and the decision log says it is cut |
| `filters.hideDecided` | The 「已了结」 filter when the app starts: `true` folds the settled issues, decided, superseded or closed (the list shows the latest three, the graph one line), `false` shows them all |
| `filters.humanOnly` | The 「仅人类议题」 filter when the app starts: human issues and the issues they block, transitively |
| `appearance` | `system` follows the system's light or dark appearance (the browser's on the web), `light` or `dark` keeps that one whatever the system's; the macOS window, title bar included, opens in it |

`quorum/theme.json` holds the design tokens: `cssVars.theme` (fonts
and lengths), `cssVars.light` and `cssVars.dark` (colors), each with
exactly the tokens of `config.example/quorum/theme.json`. Colors are
`#rrggbb`, lengths `<n>px`, fonts CSS family lists. A missing token, an
unknown one or a value that does not parse is an error that names the
file, the token's key path (`cssVars.light.ok-text`) and what is wrong;
both ends show that error while they load.

Addresses are absolute `http(s)` URLs without a query; NSH's `fetch` takes
absolute URLs only, so the web addresses name the proxy's origin.

`shared/repos.yaml` gives the PM repository and its Project
(`pm.repo`, `pm.project.owner`, `pm.project.number`) and the code, design
and omp repositories; `shared/project.yaml` gives the Project's field and
option names (`fields.status`, `fields.kind`, `fields.agenda`,
`fields.proposer`). Unknown keys and missing keys are errors that name the
file and the key path, for example
``quorum/quorum.yaml: missing field `refreshSeconds` ``.

### How each build gets it

Both ends use the same `schema::parse` and `theme::parse`, so a config that
loads on one loads on the other.

- **macOS** reads the directory `PM_CONFIG_DIR` names when it starts. A
  missing variable, file or key shows a configuration error in the window.
- **Web** fetches `/config/quorum/quorum.yaml`, `/config/quorum/theme.json`,
  `/config/shared/repos.yaml` and `/config/shared/project.yaml` when the page
  starts, from the same origin it was served by. `deploy/Caddyfile` serves
  them out of `CONFIG_DIR`. Nothing is embedded in the WebAssembly module, so
  editing those files takes effect on the next reload, with no rebuild. A
  missing file, a bad status or an invalid key shows the file, the key path
  and the reason in the window instead of a blank page.

Each end records the git blob sha (`sha1("blob <len>\0" + bytes)`, what
`git hash-object` gives the same file) of every file it loaded, in
`AppConfig.loaded`, which the 配置 page compares with the branch's files.

## macOS

Needs Rust (cargo) and Xcode's command line tools.

```sh
PM_CONFIG_DIR=$PWD/config.example cargo run
```

The GitHub token is `GH_TOKEN`, or what `gh auth token` prints when
`GH_TOKEN` is empty. Until `gh` answers, the panels that read GitHub say
the credential is being fetched; the council's and the guardians' load at
once.

`deploy/package-macos.sh <dir>` builds `<dir>/Quorum.app` (`nsh package
--target macos`, with the same `nsh` and LLVM as the web build below) and
lets the app load plain `http://` addresses: the council, guardians and
dispatch addresses in `targets.macos` are the operator's HTTP endpoints,
which App Transport Security blocks in a bundled app otherwise.

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
PATH=/opt/homebrew/opt/llvm/bin:$PATH nsh package --target web -o /tmp/quorum-web
```

The site holds no credential. It reaches GitHub, the two programs and the
dispatch launcher's check through a same-origin reverse proxy,
`deploy/Caddyfile`, which adds the GitHub token from its own environment.
The token may be able to write; the proxy passes reads to GitHub, and one
kind of write: a `PUT` of a single file under the config repo `CONFIG_REPO`
(the contents API, with `sha` to update), which is how the 配置 page commits
from the web with no login of its own. To run it locally (with the example
config's web addresses):

```sh
LISTEN=:8088 SITE_DIR=/tmp/quorum-web CONFIG_DIR=$PWD/config.example \
  GH_TOKEN=<token> CONFIG_REPO=oh-my-omp/pm-config \
  COUNCIL_URL=http://127.0.0.1:7421 GUARDIANS_URL=http://127.0.0.1:7410 \
  DISPATCH_URL=http://127.0.0.1:47593 \
  caddy run --config deploy/Caddyfile --adapter caddyfile
```

then open `http://127.0.0.1:8088/`. The proxy serves `/config/*` out of
`CONFIG_DIR` for `GET` and `HEAD` only (`Cache-Control: no-store`, and `404`
when `CONFIG_DIR` is unset), which is where the page loads its configuration
from. It maps `/gh/*` to
`https://api.github.com` with `Authorization: Bearer $GH_TOKEN` for reads
only: REST `GET` and `HEAD`, and `POST /gh/graphql` whose body contains
neither `mutation` (any case) nor a `\u` escape. The one write is
`PUT /gh/repos/$CONFIG_REPO/contents/<path>`, with a path free of `..`, `%`,
`//` and backslashes; with `CONFIG_REPO` unset no write is passed. Every
other `/gh/*` request gets `403` with an empty body and never reaches GitHub.
`/council/*` goes to
`COUNCIL_URL`, `/guardians/*` to `GUARDIANS_URL` and `/dispatch/*` to
`DISPATCH_URL` (every method, so a rewarm's or a config check's `POST` goes
through too), and `SITE_DIR` is served for everything else.

## Layout

| Path | Holds |
|---|---|
| `src/lib.rs` | The root component `App` (config, appearance, tokens) and the NSH module |
| `src/main.rs` | `cargo run`: the macOS window, opened in the config's appearance |
| `src/config/` | The config files as types (`schema.rs`, `theme.rs`, the design tokens), how each end loads them from bytes, and the git blob sha of each loaded file |
| `src/gh/` | The GitHub reader: Project items (GraphQL), comments, PM markers, implementation issues' comments, rate limit, and the one write (`PUT`) |
| `src/programs/` | The program reader: health, sessions, session records, bases, rewarm, queue |
| `src/pmconfig/` | The config repository's branch: its files, opening one, every reader's check of an edited file, the commit, what each program runs against the branch (`mod.rs`); line diffs, top-level sections and key paths to lines (`lines.rs`); the macOS checkout's fetch, distance and pull (`checkout.rs`) |
| `src/digest.rs` | The decision log: a session record reduced by the `digest` rules (tests over real session files in `tests/fixtures/`) |
| `src/sources.rs` | One `resource` per source, refreshed together |
| `src/attention.rs` | What needs a person, in order of consequence, failed sessions grouped by program and kind |
| `src/dag.rs` | The 「议题」 model: the issue graph, its groups, readiness columns, folded settled issues and filters |
| `src/detail.rs` | An agenda issue's history (rounds, drafts, votes per seat and row, rulings, decision, its council sessions) and an implementation issue's ask timeline, from their markers |
| `src/time.rs` | Timestamps, "how long ago", durations and hourly counts |
| `src/failure.rs` | Why something Quorum asked for did not come: its own reasons as variants (worded by `src/ui/text.rs`), others' words as they are |
| `src/ui/shell.rs` | The sidebar or top bar, the toolbar, the shortcuts, a list beside its inspector or (narrow) the inspector full width under 返回, and the configuration error |
| `src/ui/command.rs` | The command bar (⌘K) |
| `src/ui/overview.rs`, `issues.rs`, `implementations.rs`, `sessions.rs`, `runtime.rs`, `config.rs` | The six pages |
| `src/ui/agenda.rs` | The agenda detail and the inspector's vote matrix |
| `src/ui/parts.rs`, `status.rs`, `icons.rs`, `tokens.rs` | The app's own controls, its state words and glyphs, icons, and the theme's tokens as NSH tokens and type roles |
| `src/ui/text.rs` | Every word the UI shows, in Chinese and English |
| `src/ui/lang.rs` | Which of the two the UI speaks, and redrawing in a new one |
| `deploy/Caddyfile` | The web build's same-origin proxy: the site, its `/config`, GitHub, the two programs and the dispatch launcher's check |
| `deploy/package-macos.sh` | The macOS `Quorum.app`: `nsh package --target macos` plus the `Info.plist` entry that lets it load the `http://` targets |
| `config.example/` | A PM config directory for local runs |
