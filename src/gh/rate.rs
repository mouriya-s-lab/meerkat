//! What is left of the API's rate limits (`GET /rate_limit`): REST, which
//! reads comments, and GraphQL, which reads the Project.

use serde::Deserialize;

use super::GitHub;

/// The two limits Quorum spends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    pub rest: Bucket,
    pub graphql: Bucket,
}

/// One limit: its size, what is left, and when it resets (Unix seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Bucket {
    pub limit: u64,
    pub remaining: u64,
    pub reset: u64,
}

impl Bucket {
    /// Whether less than `percent` of the limit is left.
    pub fn below(&self, percent: u8) -> bool {
        self.remaining * 100 < self.limit * u64::from(percent)
    }
}

/// Which limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Api {
    Rest,
    GraphQl,
}

impl RateLimit {
    /// Each limit with its name.
    pub fn buckets(&self) -> [(Api, Bucket); 2] {
        [(Api::Rest, self.rest), (Api::GraphQl, self.graphql)]
    }
}

/// The current limits.
pub async fn rate_limit(github: &GitHub) -> Result<RateLimit, crate::failure::Failure> {
    #[derive(Deserialize)]
    struct Answer {
        resources: Resources,
    }
    #[derive(Deserialize)]
    struct Resources {
        core: Bucket,
        graphql: Bucket,
    }
    let answer: Answer = github.get("/rate_limit").await?;
    Ok(RateLimit {
        rest: answer.resources.core,
        graphql: answer.resources.graphql,
    })
}
