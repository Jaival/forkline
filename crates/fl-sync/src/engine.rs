//! Background sync and action worker. The app creates one `Engine`, sets
//! the `data-changed` emitter, and spawns `run_sync` and `run_actions` on
//! the Tauri (tokio) runtime. Commands call the other methods.

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use fl_db::Db;
use fl_types::{Action, ActionId, AuthPoll, AuthState, DeviceCode, FailedAction, SyncStatus};
use serde_json::Value;
use tokio::sync::Notify;

use crate::auth::{self, Device};
use crate::github::{Github, LOW_REMAINING};
use crate::queue::{self, Request};
use crate::{Error, Result, now, read, sync, write};

/// Loop tick: picks up newly pinned repos and due work.
const TICK: Duration = Duration::from_secs(10);
const ACTION_TICK: Duration = Duration::from_secs(5);
const REPOS_EVERY: i64 = 30 * 60;
const DEEP_EVERY: i64 = 5 * 60;
const NOTIFICATIONS_MIN: i64 = 60;

pub struct Config {
    pub api: String,
    pub web: String,
    pub client_id: Option<String>,
    /// Real app: OS credential store (+ env override). Tests: memory only.
    pub use_keyring: bool,
}

impl Config {
    pub fn github() -> Self {
        Self {
            api: "https://api.github.com".into(),
            web: "https://github.com".into(),
            client_id: auth::CLIENT_ID.filter(|c| !c.is_empty()).map(Into::into),
            use_keyring: true,
        }
    }

    /// Local mock server for tests; token kept in memory.
    pub fn test(base: &str) -> Self {
        Self { api: base.into(), web: base.into(), client_id: Some("test-client".into()), use_keyring: false }
    }
}

#[derive(Default)]
struct Runtime {
    paused_until: Option<i64>,
    error: Option<String>,
    /// GitHub answered 401 for the current token.
    token_rejected: bool,
    device: Option<Device>,
    mem_token: Option<String>,
}

type Emitter = Box<dyn Fn(&[&str]) + Send + Sync>;

pub struct Engine {
    db: Arc<Db>,
    gh: Github,
    cfg: Config,
    wake_sync: Notify,
    wake_actions: Notify,
    rt: Mutex<Runtime>,
    emit: OnceLock<Emitter>,
}

impl Engine {
    pub fn new(db: Arc<Db>, cfg: Config) -> Arc<Self> {
        Arc::new(Self {
            gh: Github::new(&cfg.api, &cfg.web),
            db,
            cfg,
            wake_sync: Notify::new(),
            wake_actions: Notify::new(),
            rt: Mutex::new(Runtime::default()),
            emit: OnceLock::new(),
        })
    }

    /// Set once at startup; receives `data-changed` scopes.
    pub fn set_emitter(&self, f: impl Fn(&[&str]) + Send + Sync + 'static) {
        let _ = self.emit.set(Box::new(f));
    }

    fn emit(&self, scopes: &[&str]) {
        if let Some(f) = self.emit.get() {
            f(scopes);
        }
    }

    fn rt(&self) -> MutexGuard<'_, Runtime> {
        self.rt.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Current token, unless GitHub rejected it. Never log the result.
    pub fn token(&self) -> Option<String> {
        let rt = self.rt();
        if rt.token_rejected {
            return None;
        }
        if self.cfg.use_keyring { auth::token() } else { rt.mem_token.clone() }
    }

    /// Tests: sign in without the device flow.
    pub fn set_test_token(&self, token: Option<&str>) {
        let mut rt = self.rt();
        rt.mem_token = token.map(str::to_owned);
        rt.token_rejected = false;
    }

    /// Run sync soon (e.g. after pinning a repo).
    pub fn wake(&self) {
        self.wake_sync.notify_one();
    }

    fn paused(&self) -> bool {
        self.rt().paused_until.is_some_and(|t| t > now())
    }

    // ---- status --------------------------------------------------------------

    pub async fn status(&self) -> Result<SyncStatus> {
        let mut s = read(&self.db, |c| Ok(fl_db::queries::sync::status(c)?)).await?;
        let rt = self.rt();
        let now = now();
        if let Some(until) = rt.paused_until.filter(|t| *t > now) {
            s.paused = true;
            let mins = (until - now + 59) / 60;
            s.error = Some(format!("GitHub rate limit. Sync resumes in about {mins} min."));
        } else if rt.token_rejected {
            s.error = Some("GitHub rejected the token. Sign in again.".into());
        } else {
            s.error.clone_from(&rt.error);
        }
        Ok(s)
    }

    pub async fn failed_actions(&self) -> Result<Vec<FailedAction>> {
        read(&self.db, |c| Ok(queue::failed(c)?)).await
    }

    // ---- actions -------------------------------------------------------------

    /// Validate, apply optimistically, queue, and wake the worker. No network.
    pub async fn enqueue(&self, action: Action) -> Result<ActionId> {
        let done = write(&self.db, move |c| queue::enqueue(c, &action, now())).await?;
        self.emit(done.scopes);
        self.wake_actions.notify_one();
        Ok(done.id)
    }

    pub async fn retry(&self, id: ActionId) -> Result<()> {
        let scopes = write(&self.db, move |c| queue::retry(c, id, now())).await?;
        self.emit(scopes);
        self.wake_actions.notify_one();
        Ok(())
    }

    pub async fn discard(&self, id: ActionId) -> Result<()> {
        write(&self.db, move |c| queue::discard(c, id)).await?;
        self.emit(&["sync"]);
        Ok(())
    }

    pub async fn run_actions(self: Arc<Self>) {
        // Nothing is in flight after a restart.
        let _ = write(&self.db, |c| Ok(queue::reset_in_flight(c)?)).await;
        loop {
            if let Err(e) = self.process_actions().await {
                self.rt().error = Some(e.to_string());
                self.emit(&["sync"]);
            }
            tokio::select! {
                () = self.wake_actions.notified() => {}
                () = tokio::time::sleep(ACTION_TICK) => {}
            }
        }
    }

    /// Send every due action, oldest first. Stops early when offline,
    /// rate limited or signed out. Returns how many were handled.
    pub async fn process_actions(&self) -> Result<usize> {
        let Some(token) = self.token() else { return Ok(0) };
        let mut handled = 0;
        while !self.paused() {
            let Some((id, action)) = write(&self.db, |c| queue::claim(c, now())).await? else { break };
            let a = action.clone();
            let sent = match read(&self.db, move |c| queue::plan(c, &a)).await {
                Ok(req) => self.send(&token, req).await,
                Err(e) => Err(e),
            };
            handled += 1;
            let stop = match sent {
                Ok(resp) => {
                    let a = action.clone();
                    write(&self.db, move |c| queue::complete(c, id, &a, &resp)).await?;
                    false
                }
                Err(e) => {
                    let stop = matches!(e, Error::Network(_) | Error::Unauthorized | Error::RateLimited { .. });
                    self.note_github_error(&e);
                    write(&self.db, move |c| queue::fail(c, id, &e, now())).await?;
                    stop
                }
            };
            self.emit(queue::scopes(&action));
            if stop {
                break;
            }
        }
        Ok(handled)
    }

    async fn send(&self, token: &str, req: Request) -> Result<Value> {
        match req {
            Request::Rest { method, path, body } => Ok(self.gh.rest(token, method, &path, body.as_ref(), &[]).await?.body),
            Request::Graphql { query, vars } => self.gh.graphql(token, query, vars).await,
        }
    }

    /// Pause on rate limits; forget a rejected token.
    fn note_github_error(&self, e: &Error) {
        let mut rt = self.rt();
        match e {
            Error::RateLimited { until } => rt.paused_until = Some(*until),
            Error::Unauthorized => rt.token_rejected = true,
            _ => {}
        }
    }

    // ---- sync ------------------------------------------------------------------

    pub async fn run_sync(self: Arc<Self>) {
        loop {
            self.sync_once().await;
            tokio::select! {
                () = self.wake_sync.notified() => {}
                () = tokio::time::sleep(TICK) => {}
            }
        }
    }

    /// One pass over whatever is due. Errors end up in `SyncStatus`.
    pub async fn sync_once(&self) {
        let was_paused = self.paused();
        let result = self.try_sync().await;
        let mut rt = self.rt();
        match result {
            Ok(()) => rt.error = None,
            Err(e @ (Error::RateLimited { .. } | Error::Unauthorized)) => {
                drop(rt);
                self.note_github_error(&e);
                rt = self.rt();
            }
            Err(e) => rt.error = Some(e.to_string()),
        }
        if was_paused && !rt.paused_until.is_some_and(|t| t > now()) {
            rt.paused_until = None;
        }
        drop(rt);
        self.emit(&["sync"]);
    }

    /// Pause before the budget runs out.
    fn check_budget(&self) -> Result<()> {
        match self.gh.rate() {
            Some((remaining, reset)) if remaining < LOW_REMAINING && reset > now() => Err(Error::RateLimited { until: reset }),
            _ => Ok(()),
        }
    }

    async fn try_sync(&self) -> Result<()> {
        let Some(token) = self.token() else { return Ok(()) };
        if self.paused() {
            return Ok(());
        }
        self.check_budget()?;
        let now = now();
        let emit = |s: &[&str]| self.emit(s);
        let (has_viewer, repos, notes, due) = read(&self.db, move |c| {
            let has_viewer = fl_db::queries::layout::setting(c, "viewer")?.is_some();
            let repos = sync::state(c, "repos")?.last_synced_at.unwrap_or(0);
            let n = sync::state(c, "notifications")?;
            let notes = n.last_synced_at.unwrap_or(0) + n.poll_interval.unwrap_or(0).max(NOTIFICATIONS_MIN);
            let due: Vec<(i64, String)> = c
                .prepare(
                    "SELECT r.id, r.owner || '/' || r.name FROM repos r
                     LEFT JOIN sync_state s ON s.key = 'repo:' || r.id
                     WHERE r.pinned = 1 AND r.sync_enabled = 1 AND COALESCE(s.last_synced_at, 0) + ?1 <= ?2",
                )?
                .query_map([DEEP_EVERY, now], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            Ok((has_viewer, repos, notes, due))
        })
        .await?;

        if !has_viewer {
            sync::viewer(&self.gh, &token, &self.db).await?;
        }
        if repos + REPOS_EVERY <= now {
            sync::repos(&self.gh, &token, &self.db, now, &emit).await?;
        }
        if notes <= now {
            sync::notifications(&self.gh, &token, &self.db, now, &emit).await?;
        }
        let mut repo_error = None;
        for (repo_id, full_name) in due {
            self.check_budget()?;
            match sync::deep(&self.gh, &token, &self.db, repo_id, now, &emit).await {
                Ok(()) => {}
                Err(e @ (Error::RateLimited { .. } | Error::Unauthorized | Error::Network(_) | Error::Db(_))) => return Err(e),
                Err(e) => {
                    // One broken repo (deleted, no access) must not block the rest.
                    repo_error = Some(format!("{full_name}: {e}"));
                    let key = format!("repo:{repo_id}");
                    write(&self.db, move |c| {
                        Ok(sync::save_state(c, &key, &sync::State { last_synced_at: Some(now), ..Default::default() })?)
                    })
                    .await?;
                }
            }
        }
        match repo_error {
            Some(e) => Err(Error::Invalid(e)),
            None => Ok(()),
        }
    }

    // ---- auth ------------------------------------------------------------------

    pub async fn auth_state(&self) -> Result<AuthState> {
        let viewer = read(&self.db, |c| Ok(fl_db::queries::layout::setting(c, "viewer")?)).await?;
        let login = viewer
            .and_then(|v| serde_json::from_str::<Value>(&v).ok())
            .and_then(|v| v["login"].as_str().map(str::to_owned));
        let signed_in = self.token().is_some();
        Ok(AuthState {
            signed_in,
            login: login.filter(|_| signed_in),
            client_id_configured: self.cfg.client_id.is_some(),
            env_override: self.cfg.use_keyring && auth::env_token().is_some(),
        })
    }

    pub async fn auth_start(&self) -> Result<DeviceCode> {
        let Some(client_id) = &self.cfg.client_id else {
            return Err(Error::Invalid(
                "This build has no GitHub OAuth client ID. Set FORKLINE_GITHUB_TOKEN to a personal access token instead."
                    .into(),
            ));
        };
        let (code, device) = auth::start(&self.gh, client_id).await?;
        self.rt().device = Some(device);
        Ok(code)
    }

    /// One poll of the device flow. On success stores the token, reads the
    /// viewer and wakes sync.
    pub async fn auth_poll(&self) -> Result<AuthPoll> {
        let (Some(client_id), Some(mut device)) = (&self.cfg.client_id, self.rt().device.clone()) else {
            return Err(Error::Invalid("sign-in was not started".into()));
        };
        let (poll, token) = auth::poll(&self.gh, client_id, &mut device).await?;
        let Some(token) = token else {
            let mut rt = self.rt();
            rt.device = matches!(poll, AuthPoll::Pending { .. }).then_some(device);
            return Ok(poll);
        };
        let login = sync::viewer(&self.gh, &token, &self.db).await?;
        if self.cfg.use_keyring {
            auth::store_token(&token)?;
        }
        {
            let mut rt = self.rt();
            if !self.cfg.use_keyring {
                rt.mem_token = Some(token);
            }
            rt.device = None;
            rt.token_rejected = false;
            rt.error = None;
        }
        self.wake();
        self.wake_actions.notify_one();
        self.emit(&["sync"]);
        Ok(AuthPoll::Done { login })
    }

    /// Delete the stored token. Cached data stays.
    pub async fn sign_out(&self) -> Result<()> {
        if self.cfg.use_keyring {
            auth::delete_token()?;
        }
        {
            let mut rt = self.rt();
            rt.mem_token = None;
            rt.device = None;
        }
        write(&self.db, |c| Ok(c.execute("DELETE FROM settings WHERE key = 'viewer'", []).map(|_| ())?)).await?;
        self.emit(&["sync"]);
        Ok(())
    }
}
