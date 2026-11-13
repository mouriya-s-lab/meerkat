//! The credential scenarios, run against the real app on the headless
//! engine (see `tests/support/`): `gh` is a script the test writes into a
//! temporary directory and puts on `PATH`, and the council, guardians and
//! GitHub answers are the recorded ones. The app speaks Chinese
//! (`OMOMP_LANG=zh`): `gh_missing` reads the credential's reason, which
//! Meerkat writes in Chinese.
//!
//! Each test rewrites this process's `PATH` and `OMOMP_LANG`, which no two
//! tests may do at once: they take [`serial`]'s lock, so `cargo test`'s
//! parallel threads still run them one at a time (the issue's commands add
//! `--test-threads=1`).

mod support;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use nsh_testing::{StubHandler, StubRoute};
use meerkat::{Lang, Msg};
use support::{Fixture, GITHUB, Sandbox};

/// The language the credential tests run Meerkat in.
const LANG: Lang = Lang::Zh;

/// How long the window may take to show what a recorded answer holds.
const SHOWN: Duration = Duration::from_secs(5);

/// The fixture config's `refreshSeconds`: the next Project request comes this
/// long after the first.
const REFRESH: Duration = Duration::from_secs(5);

/// Holds the one credential test at a time: each rewrites the process's
/// `PATH`, where the app finds `gh`.
fn serial() -> MutexGuard<'static, ()> {
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// The Project's request: every GitHub answer in the tests is this one.
fn project_url() -> String {
    format!("{GITHUB}/graphql")
}

/// The recorded council and guardians data a person reads on the overview
/// whatever the GitHub credential is doing: the programs' versions, the
/// queue's and the bases' panels with their recorded rows.
fn programs_and_bases() -> Vec<String> {
    let mut texts = vec![
        Msg::CouncilQueue.in_lang(LANG).to_string(),
        Msg::Bases.in_lang(LANG).to_string(),
        support::bases_note().in_lang(LANG).to_string(),
    ];
    texts.extend(support::base_repos());
    texts.extend(support::health_versions());
    texts
}

#[test]
fn gh_hangs() -> Result<(), String> {
    let _serial = serial();
    let sandbox = Sandbox::new("gh-hangs");
    // `gh` never answers: no GitHub request can be made.
    sandbox.script("gh", "#!/bin/sh\nexec sleep 600\n");
    support::path_with(sandbox.dir());

    let mut meerkat = Fixture::start(LANG);
    // The recorded council and guardians data shows while every GitHub
    // request waits for `gh`.
    meerkat.wait_for_all(&programs_and_bases(), SHOWN)?;
    Ok(())
}

#[test]
fn gh_is_asked_for_every_request() -> Result<(), String> {
    let _serial = serial();
    const FIRST: &str = "gho_q23_first_account";
    const SECOND: &str = "gho_q23_second_account";
    let sandbox = Sandbox::new("gh-answers");
    // `gh` prints whichever token the file holds: the account it is on.
    let account = sandbox.dir().join("token");
    std::fs::write(&account, FIRST).map_err(|error| error.to_string())?;
    sandbox.script("gh", &format!("#!/bin/sh\ncat {}\n", account.display()));
    support::path_with(sandbox.dir());

    // GitHub answers only what carries one of the two tokens, and keeps
    // what each request carried.
    let carried = Arc::new(Mutex::new(Vec::<String>::new()));
    let github: StubHandler = {
        let carried = carried.clone();
        Arc::new(move |request, reply| {
            let bearer = request
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
                .map(|(_, value)| value.clone());
            let authorised = bearer
                .as_deref()
                .is_some_and(|bearer| bearer == format!("Bearer {FIRST}") || bearer == format!("Bearer {SECOND}"));
            carried
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(bearer);
            reply.send(if authorised {
                support::project()
            } else {
                support::answer(401, r#"{"message":"Bad credentials"}"#)
            });
        })
    };
    let mut meerkat = Fixture::start_with(
        LANG,
        vec![(format!("POST {}", project_url()), StubRoute::Handler(github))],
    );

    meerkat.open(Msg::PageIssues)?;
    meerkat.wait_for_any(&support::project_titles(), SHOWN)?;
    // `gh` moves to another account while the app runs: a later request
    // carries the new token, with no restart.
    std::fs::write(&account, SECOND).map_err(|error| error.to_string())?;
    let second = format!("Bearer {SECOND}");
    let seen = carried.clone();
    meerkat.wait_until("a request with gh's new token", REFRESH * 2, move |_| {
        seen.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|bearer| *bearer == second)
    })?;
    Ok(())
}

#[test]
fn gh_missing() -> Result<(), String> {
    let _serial = serial();
    let sandbox = Sandbox::new("gh-missing");
    // `PATH` has no `gh` at all.
    support::path_without_gh(sandbox.dir());

    let mut meerkat = Fixture::start(LANG);
    // The reason `gh` gave no credential, as the GitHub panels write it.
    meerkat.wait_for_matching("无法执行 `gh auth token`", SHOWN)?;
    // The council and guardians are unaffected: their recorded data is on
    // screen with the missing credential.
    let shown = programs_and_bases();
    assert!(
        shown.iter().all(|text| meerkat.shows(text)),
        "the recorded council and guardians data is not on screen with the missing credential"
    );
    Ok(())
}
