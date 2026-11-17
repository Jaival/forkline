//! App-wide handles: the database, the sync engine, and the
//! `DataChanged` events views subscribe to.
//!
//! Views read SQLite through [`Store::read`] (background thread) and call the
//! engine through [`Store::run`] (it needs the tokio runtime). Subscribe to the
//! store entity to refresh when sync or an action changed a scope:
//!
//! ```ignore
//! cx.subscribe(&Store::global(cx), |this, _, ev: &DataChanged, cx| {
//!     if ev.scopes.iter().any(|s| s == "issues") { this.reload(cx) }
//! })
//! ```

use std::future::Future;
use std::sync::Arc;

use fl_db::Db;
use fl_db::queries::pr;
use fl_git::GitStore;
use fl_sync::Engine;
use fl_types::{DataChanged, DiffSummary, IssueId};
use gpui_kit::{App, AppContext as _, Entity, EventEmitter, Global, Task};
use rusqlite::Connection;

pub struct Store {
    db: Arc<Db>,
    sync: Arc<Engine>,
    git: Arc<GitStore>,
    /// No sync engine and no network: git reads use local clones only.
    offline: bool,
    read_only: bool,
    runtime: tokio::runtime::Runtime,
}

impl EventEmitter<DataChanged> for Store {}

struct GlobalStore(Entity<Store>);

impl Global for GlobalStore {}

impl Store {
    /// Open the database in `data_dir`, set the store as a global, and (unless
    /// `offline`) start the sync engine and action worker. `read_only` (a
    /// bench run): views skip the writes that opening an item makes.
    pub fn init(data_dir: std::path::PathBuf, offline: bool, read_only: bool, cx: &mut App) -> anyhow::Result<()> {
        std::fs::create_dir_all(&data_dir)?;
        let db = Arc::new(Db::open(&fl_db::db_path(&data_dir))?);
        let sync = Engine::new(db.clone(), fl_sync::Config::github());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("fl-sync")
            .enable_all()
            .build()?;
        let git = Arc::new(GitStore::new(&data_dir));
        let store = cx.new(|_| Self {
            db,
            sync: sync.clone(),
            git,
            offline,
            read_only,
            runtime,
        });
        cx.set_global(GlobalStore(store.clone()));

        // Offline too: a queued action still changes local rows.
        let (tx, rx) = async_channel::unbounded::<Vec<String>>();
        sync.set_emitter(move |scopes| {
            let _ = tx.try_send(scopes.iter().map(|s| (*s).to_owned()).collect());
        });
        cx.spawn(async move |cx| {
            while let Ok(scopes) = rx.recv().await {
                store.update(cx, |_, cx| cx.emit(DataChanged { scopes }));
            }
        })
        .detach();
        if offline {
            return Ok(());
        }
        let store = Self::global(cx).read(cx);
        store.runtime.spawn(sync.clone().run_sync());
        store.runtime.spawn(sync.run_actions());
        Ok(())
    }

    pub fn global(cx: &App) -> Entity<Store> {
        cx.global::<GlobalStore>().0.clone()
    }

    pub fn db(&self) -> &Arc<Db> {
        &self.db
    }

    pub fn sync(&self) -> &Arc<Engine> {
        &self.sync
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// Tell the views that local rows of `scopes` changed (a write the sync
    /// engine did not make, e.g. `last_viewed_at`).
    pub fn emit_changed(scopes: &[&str], cx: &mut App) {
        let scopes = scopes.iter().map(|s| (*s).to_owned()).collect();
        Self::global(cx).update(cx, |_, cx| cx.emit(DataChanged { scopes }));
    }

    /// Changed files and the whole unified patch of a PR (input of
    /// gpui-component's `DiffFile::parse`), from the local clone. Online,
    /// missing commits are fetched first; offline, a PR without a local clone
    /// is an error.
    pub fn diff_patch(&self, cx: &App, pr_id: IssueId) -> Task<Result<(DiffSummary, String), String>> {
        let git = self.git.clone();
        let resolve = self.resolve_clone(pr_id);
        cx.background_spawn(async move {
            let (clone, target) = resolve()?;
            git.diff_patch(&clone, &target.base_oid, &target.head_oid).map_err(|e| e.to_string())
        })
    }

    /// A blocking closure that finds the diff target of a PR and its clone
    /// path. Online, missing commits are fetched; offline, the clone path is
    /// only computed.
    fn resolve_clone(&self, pr_id: IssueId) -> impl FnOnce() -> Result<(std::path::PathBuf, pr::DiffTarget), String> + Send + 'static {
        let (db, git, sync, offline) = (self.db.clone(), self.git.clone(), self.sync.clone(), self.offline);
        move || {
            let target = db
                .read(|c| pr::diff_target(c, pr_id))
                .map_err(|e| e.to_string())?
                .ok_or("This pull request has no diff data yet.")?;
            let clone = if offline {
                git.root().join(&target.owner).join(format!("{}.git", target.name))
            } else {
                let oids = [target.base_oid.as_str(), target.head_oid.as_str()];
                git.ensure_commits(&target.owner, &target.name, sync.token().as_deref(), &oids)
                    .map_err(|e| e.to_string())?
            };
            Ok((clone, target))
        }
    }

    /// Run `f` on a reader connection, on a background thread.
    pub fn read<T: Send + 'static>(
        &self,
        cx: &App,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> Task<rusqlite::Result<T>> {
        let db = self.db.clone();
        cx.background_spawn(async move { db.read(f) })
    }

    /// Run a future that needs tokio (engine calls, reqwest) on the sync runtime.
    pub fn run<T: Send + 'static>(
        &self,
        cx: &App,
        fut: impl Future<Output = T> + Send + 'static,
    ) -> Task<Result<T, tokio::task::JoinError>> {
        let handle = self.runtime.spawn(fut);
        cx.background_spawn(handle)
    }
}
