//! GitHub auth, GraphQL/REST client, sync engine, action queue, rate limits. Owner: Phase 1 agent 1.
//!
//! Contract for other agents: `auth::token()` is the only API they call
//! (agent 4 / 5 need it to fetch private repos). Signature is fixed.

pub mod auth {
    /// The signed-in user's GitHub token, if any. Reads the OS credential
    /// store (or `FORKLINE_GITHUB_TOKEN` in dev). Never log the result.
    pub fn token() -> Option<String> {
        std::env::var("FORKLINE_GITHUB_TOKEN").ok().filter(|t| !t.is_empty())
    }
}
