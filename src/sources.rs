//! The overview's data: one `resource` per source, each loading, failing
//! and keeping its last value on its own. A refresh refetches every source
//! that reads a remote directly; the open escalations follow the Project,
//! loading again whenever its items load. The sources that read GitHub
//! wait for its credential (fetched apart on macOS) and load once it is in;
//! the programs' load at once.

use std::{future::Future, rc::Rc};

use nsh_core::prelude::*;

use crate::{
    config::AppConfig,
    gh::{
        Credential, GitHub,
        escalations::{self, OpenEscalation},
        project::{self, ProjectItem},
        rate::{self, RateLimit},
    },
    pmconfig::{self, ConfigRepoClient, Listing, Running},
    programs::{Base, Health, Program, ProgramClient, QueueEntry, Session},
    time,
};

/// A source's value and when it was loaded (Unix milliseconds): the time
/// stays with the value, so a failure after it still shows when the data
/// was last good.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded<T> {
    pub data: T,
    pub at: f64,
}

/// A source: its last value, its last error, whether it loads.
pub type Source<T> = Resource<Loaded<T>, String>;

/// A program's sources.
#[derive(Clone, Copy)]
pub struct ProgramSources {
    pub program: Program,
    pub health: Source<Health>,
    pub sessions: Source<Vec<Session>>,
    /// The config files it loaded at startup (`GET /v1/config`).
    pub config: Source<Running>,
}

/// Every source of the overview, the GitHub credential they read with and
/// when the last refresh began.
#[derive(Clone, Copy)]
pub struct Sources {
    /// `None` while it is fetched: the GitHub sources have not loaded yet.
    pub credential: Signal<Option<Credential>>,
    pub project: Source<Vec<ProjectItem>>,
    pub rate: Source<RateLimit>,
    pub escalations: Source<Vec<OpenEscalation>>,
    pub council: ProgramSources,
    pub queue: Source<Vec<QueueEntry>>,
    pub guardians: ProgramSources,
    pub bases: Source<Vec<Base>>,
    /// The config repository's branch: its head and files.
    pub config_files: Source<Listing>,
    pub last_refresh: Signal<f64>,
}

/// A source loading `load()` once now and again on each refetch.
fn source<T, Fut>(load: impl Fn() -> Fut + 'static) -> Source<T>
where
    T: 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
{
    source_from(|| Some(()), move |()| load())
}

/// A source loading `load(from)` whenever `from()` gives a new `Some`, and
/// again on each refetch while it does; nothing while it gives `None`.
fn source_from<S, T, Fut>(
    from: impl Fn() -> Option<S> + 'static,
    load: impl Fn(S) -> Fut + 'static,
) -> Source<T>
where
    S: 'static,
    T: 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
{
    resource(from, move |from, _| {
        let loading = load(from);
        async move {
            loading.await.map(|data| Loaded {
                data,
                at: time::now_ms(),
            })
        }
    })
}

impl Sources {
    /// Starts every source (the caller's owner keeps them); the GitHub
    /// ones once `credential` is in.
    pub fn new(config: &AppConfig, credential: Signal<Option<Credential>>, config_repo: Rc<ConfigRepoClient>) -> Self {
        let api = config.endpoints.github_api.clone();
        let github = computed(move || {
            credential.with(|credential| {
                credential
                    .clone()
                    .map(|credential| Rc::new(GitHub::new(api.clone(), credential)))
            })
        });
        let project_ref = Rc::new(config.repos.pm.project.clone());
        let fields = Rc::new(config.fields.clone());
        let council = Rc::new(ProgramClient::new(
            Program::Council,
            config.endpoints.council.clone(),
        ));
        let guardians = Rc::new(ProgramClient::new(
            Program::Guardians,
            config.endpoints.guardians.clone(),
        ));

        let project = source_from(
            move || github.get(),
            move |github| {
                let (project_ref, fields) = (project_ref.clone(), fields.clone());
                async move { project::items(&github, &project_ref, &fields).await }
            },
        );
        let rate = source_from(
            move || github.get(),
            |github| async move { rate::rate_limit(&github).await },
        );
        let escalations = source_from(
            move || {
                let github = github.get()?;
                project.with(|items| {
                    items.map(|items| (github, escalations::unfinished_implementations(&items.data)))
                })
            },
            |(github, issues)| async move { escalations::open_escalations(&github, issues).await },
        );
        let program = |client: &Rc<ProgramClient>| ProgramSources {
            program: client.program,
            health: source({
                let client = client.clone();
                move || {
                    let client = client.clone();
                    async move { client.health().await }
                }
            }),
            sessions: source({
                let client = client.clone();
                move || {
                    let client = client.clone();
                    async move { client.sessions().await }
                }
            }),
            config: source({
                let client = client.clone();
                move || {
                    let client = client.clone();
                    async move { pmconfig::running(&client.base).await }
                }
            }),
        };
        let queue = source({
            let council = council.clone();
            move || {
                let council = council.clone();
                async move { council.queue().await }
            }
        });
        let bases = source({
            let guardians = guardians.clone();
            move || {
                let guardians = guardians.clone();
                async move { guardians.bases().await }
            }
        });
        let config_files = source_from(
            move || github.get().map(|github| (github, ())),
            move |(github, ())| {
                let config_repo = config_repo.clone();
                async move { config_repo.listing_with(&github).await }
            },
        );
        Sources {
            credential,
            project,
            rate,
            escalations,
            council: program(&council),
            queue,
            guardians: program(&guardians),
            bases,
            config_files,
            last_refresh: signal(time::now_ms()),
        }
    }

    /// Loads every source again now.
    pub fn refresh(&self) {
        self.last_refresh.set(time::now_ms());
        batch(|| {
            self.project.refetch();
            self.rate.refetch();
            self.council.health.refetch();
            self.council.sessions.refetch();
            self.council.config.refetch();
            self.queue.refetch();
            self.guardians.health.refetch();
            self.guardians.sessions.refetch();
            self.guardians.config.refetch();
            self.bases.refetch();
            self.config_files.refetch();
        });
    }
}
