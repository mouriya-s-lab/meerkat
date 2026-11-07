//! The four credential scenarios of #23, run against the real app on the
//! headless engine (see `tests/support/`): `gh` is a script the test writes
//! into a temporary directory and puts on `PATH`, and the council, guardians
//! and GitHub answers are the recorded ones. The app speaks Chinese
//! (`OMOMP_LANG=zh`): `gh_missing` reads the credential's reason, which
//! Quorum writes in Chinese.
//!
//! Each test rewrites this process's `PATH`, `GH_TOKEN` and `OMOMP_LANG`,
//! which no two tests may do at once: they take [`serial`]'s lock, so
//! `cargo test`'s parallel threads still run them one at a time (the issue's
//! commands add `--test-threads=1`).

mod support;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use nsh_testing::{StubHandler, StubRoute};
use quorum::{Lang, Msg};
use support::{Fixture, GITHUB, Sandbox};

/// The language the credential tests run Quorum in.
const LANG: Lang = Lang::Zh;

/// How long the window may take to show what a recorded answer holds.
const SHOWN: Duration = Duration::from_secs(5);

/// The fixture config's `refreshSeconds`: the next Project request comes this
/// long after the first.
const REFRESH: Duration = Duration::from_secs(5);

/// Holds the one credential test at a time: each rewrites the process's
/// `PATH` and `GH_TOKEN`, which the app reads.
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
    // `gh` never answers: the credential has no way to arrive.
    sandbox.script("gh", "#!/bin/sh\nexec sleep 600\n");
    support::path_with(sandbox.dir());
    support::clear_github_token();

    let mut quorum = Fixture::start(LANG);
    quorum.wait_for_all(&programs_and_bases(), SHOWN)?;
    // The GitHub panels say the credential is being fetched, while the
    // recorded council and guardians data above stays on screen.
    quorum.wait_for(&quorum.say(&Msg::FetchingCredential), SHOWN)?;
    Ok(())
}

#[test]
fn gh_answers() -> Result<(), String> {
    let _serial = serial();
    const TOKEN: &str = "gho_q23_recorded_token";
    let sandbox = Sandbox::new("gh-answers");
    sandbox.script("gh", &format!("#!/bin/sh\necho {TOKEN}\n"));
    support::path_with(sandbox.dir());
    support::clear_github_token();

    // GitHub answers only what carries the token the script printed.
    let expected = format!("Bearer {TOKEN}");
    let github: StubHandler = Arc::new(move |request, reply| {
        let authorised = request
            .headers
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("authorization") && *value == expected);
        reply.send(if authorised {
            support::project()
        } else {
            support::answer(401, r#"{"message":"Bad credentials"}"#)
        });
    });
    let mut quorum = Fixture::start_with(
        LANG,
        vec![(format!("POST {}", project_url()), StubRoute::Handler(github))],
    );

    quorum.open(Msg::PageIssues)?;
    quorum.wait_for_any(&support::project_titles(), SHOWN)?;
    // A refresh interval on (1 s): the Project is read again.
    quorum.wait_for_requests("POST", &project_url(), 2, REFRESH)?;
    assert!(
        quorum.requests("POST", &project_url()) >= 2,
        "the Project was not read again after a refresh interval"
    );
    Ok(())
}

#[test]
fn gh_missing() -> Result<(), String> {
    let _serial = serial();
    let sandbox = Sandbox::new("gh-missing");
    // `PATH` has no `gh` at all.
    support::path_without_gh(sandbox.dir());
    support::clear_github_token();

    let mut quorum = Fixture::start(LANG);
    // `Credential::Missing`'s reason, as the GitHub panels write it.
    quorum.wait_for_matching("无法执行 `gh auth token`", SHOWN)?;
    // The council and guardians are unaffected: their recorded data is on
    // screen with the missing credential.
    let shown = programs_and_bases();
    assert!(
        shown.iter().all(|text| quorum.shows(text)),
        "the recorded council and guardians data is not on screen with the missing credential"
    );
    Ok(())
}

#[test]
fn gh_token_env() -> Result<(), String> {
    let _serial = serial();
    let sandbox = Sandbox::new("gh-token-env");
    // The script writes a marker if it ever runs.
    let marker = sandbox.dir().join("gh-ran");
    sandbox.script(
        "gh",
        &format!("#!/bin/sh\ntouch {}\nexit 1\n", marker.display()),
    );
    support::path_with(sandbox.dir());
    support::set_github_token("gho_q23_env_token");

    let mut quorum = Fixture::start(LANG);
    quorum.open(Msg::PageIssues)?;
    quorum.wait_for_any(&support::project_titles(), SHOWN)?;
    assert!(
        !marker.exists(),
        "`gh` ran ({}) though GH_TOKEN was set",
        marker.display()
    );
    Ok(())
}
