//! Pull data from GitHub into SQLite.
//!
//! - `repos`: metadata for every accessible repo plus the `starred` flag (REST, ETag).
//! - `deep`: labels, overview, issues and PRs of one pinned repo (GraphQL,
//!   incremental by `updatedAt` cursor stored in `sync_state`).
//! - `notifications`: inbox threads (REST, `If-Modified-Since`, `X-Poll-Interval`).
//!
//! Upserts never overwrite fields that have a row in `pending_fields`.
//! Every page is one transaction; issues are re-indexed for search in it.

use std::sync::Arc;

use fl_db::Db;
use reqwest::Method;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Value, json};

use crate::github::Github;
use crate::queue::is_pending;
use crate::{Result, parse_ts, read, write};

/// Callback for `data-changed` after each written batch.
pub type Emit<'a> = &'a (dyn Fn(&[&str]) + Send + Sync);

/// Pages fetched on a repo's first deep sync (newest first). Older items
/// arrive when they next change.
pub const FIRST_SYNC_PAGES: usize = 6;
/// Upper bound for one incremental run; the rest follows next run.
pub const MAX_PAGES: usize = 30;

// ---- sync_state ------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct State {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub cursor: Option<String>,
    pub last_synced_at: Option<i64>,
    pub poll_interval: Option<i64>,
}

pub fn state(conn: &Connection, key: &str) -> rusqlite::Result<State> {
    Ok(conn
        .prepare_cached("SELECT etag, last_modified, cursor, last_synced_at, poll_interval FROM sync_state WHERE key = ?1")?
        .query_row([key], |r| {
            Ok(State {
                etag: r.get(0)?,
                last_modified: r.get(1)?,
                cursor: r.get(2)?,
                last_synced_at: r.get(3)?,
                poll_interval: r.get(4)?,
            })
        })
        .optional()?
        .unwrap_or_default())
}

/// Upsert; `None` fields keep their stored value.
pub fn save_state(conn: &Connection, key: &str, s: &State) -> rusqlite::Result<()> {
    conn.prepare_cached(
        "INSERT INTO sync_state (key, etag, last_modified, cursor, last_synced_at, poll_interval)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(key) DO UPDATE SET
           etag = COALESCE(excluded.etag, etag),
           last_modified = COALESCE(excluded.last_modified, last_modified),
           cursor = COALESCE(excluded.cursor, cursor),
           last_synced_at = COALESCE(excluded.last_synced_at, last_synced_at),
           poll_interval = COALESCE(excluded.poll_interval, poll_interval)",
    )?
    .execute(params![key, s.etag, s.last_modified, s.cursor, s.last_synced_at, s.poll_interval])?;
    Ok(())
}

fn touched(now: i64) -> State {
    State { last_synced_at: Some(now), ..State::default() }
}

/// GET every page of a REST list. `None` when the first page is a 304 for
/// the stored ETag. The new ETag is saved with `last_synced_at`.
async fn list_cached(gh: &Github, token: &str, db: &Arc<Db>, key: &str, path: &str, now: i64) -> Result<Option<Vec<Value>>> {
    let k = key.to_owned();
    let old = read(db, move |c| Ok(state(c, &k)?)).await?;
    let headers: Vec<(&str, &str)> = old.etag.as_deref().map(|e| ("If-None-Match", e)).into_iter().collect();
    let mut resp = gh.rest(token, Method::GET, path, None, &headers).await?;
    let etag = resp.header("etag").map(str::to_owned);
    let mut items = Vec::new();
    if resp.status != 304 {
        for _ in 0..MAX_PAGES {
            items.extend(resp.body.as_array().cloned().unwrap_or_default());
            let Some(next) = resp.next_link() else { break };
            resp = gh.rest(token, Method::GET, &next, None, &[]).await?;
        }
    }
    let k = key.to_owned();
    let not_modified = resp.status == 304;
    write(db, move |c| Ok(save_state(c, &k, &State { etag, ..touched(now) })?)).await?;
    Ok((!not_modified).then_some(items))
}

// ---- shared upserts ---------------------------------------------------------

/// Upsert a user from a REST user or GraphQL actor. Returns its id when the
/// actor has a numeric id (users, bots).
fn user(tx: &Transaction<'_>, a: &Value) -> rusqlite::Result<Option<i64>> {
    let (Some(id), Some(login)) = (a["databaseId"].as_i64().or(a["id"].as_i64()), a["login"].as_str()) else {
        return Ok(None);
    };
    // A login can move to another account; free it first (UNIQUE login).
    tx.prepare_cached("UPDATE users SET login = login || '~' || id WHERE login = ?1 AND id <> ?2")?
        .execute(params![login, id])?;
    tx.prepare_cached(
        "INSERT INTO users (id, login, name, avatar_url) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET login = excluded.login,
           name = COALESCE(excluded.name, name), avatar_url = COALESCE(excluded.avatar_url, avatar_url)",
    )?
    .execute(params![id, login, a["name"].as_str(), a["avatarUrl"].as_str().or(a["avatar_url"].as_str())])?;
    Ok(Some(id))
}

/// Upsert a REST repository. `full` = from `/user/repos` (all fields);
/// otherwise a minimal repo (notifications) that only fills a new row.
fn repo(tx: &Transaction<'_>, r: &Value, full: bool) -> rusqlite::Result<Option<i64>> {
    let (Some(id), Some(name), Some(owner)) = (r["id"].as_i64(), r["name"].as_str(), r["owner"]["login"].as_str()) else {
        return Ok(None);
    };
    let ts = |k: &str| r[k].as_str().map(parse_ts);
    let p = params![
        id,
        r["node_id"].as_str().unwrap_or(""),
        owner,
        name,
        r["description"].as_str(),
        r["language"].as_str(),
        r["default_branch"].as_str().unwrap_or("main"),
        r["license"]["spdx_id"].as_str(),
        r["private"].as_bool().unwrap_or(false),
        r["fork"].as_bool().unwrap_or(false),
        r["stargazers_count"].as_i64().unwrap_or(0),
        r["forks_count"].as_i64().unwrap_or(0),
        ts("pushed_at"),
        ts("updated_at").unwrap_or(0),
    ];
    let insert = "INSERT INTO repos (id, node_id, owner, name, description, language, default_branch, license,
           is_private, is_fork, stars, forks, pushed_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)";
    if full {
        tx.prepare_cached(&format!(
            "{insert} ON CONFLICT(id) DO UPDATE SET node_id = excluded.node_id, owner = excluded.owner,
               name = excluded.name, description = excluded.description, language = excluded.language,
               default_branch = excluded.default_branch, license = excluded.license,
               is_private = excluded.is_private, is_fork = excluded.is_fork, stars = excluded.stars,
               forks = excluded.forks, pushed_at = excluded.pushed_at, updated_at = excluded.updated_at"
        ))?
        .execute(p)?;
    } else {
        tx.prepare_cached(&format!("{insert} ON CONFLICT DO NOTHING"))?.execute(p)?;
    }
    Ok(Some(id))
}

fn set_links(tx: &Transaction<'_>, table: &str, col: &str, issue: i64, ids: &[i64]) -> rusqlite::Result<()> {
    tx.prepare_cached(&format!("DELETE FROM {table} WHERE issue_id = ?1"))?.execute([issue])?;
    let mut stmt = tx.prepare_cached(&format!("INSERT OR IGNORE INTO {table} (issue_id, {col}) VALUES (?1, ?2)"))?;
    for id in ids {
        stmt.execute([issue, *id])?;
    }
    Ok(())
}

fn nodes(v: &Value) -> &[Value] {
    v["nodes"].as_array().map_or(&[], Vec::as_slice)
}

fn ids_json(ids: &[i64]) -> String {
    json!(ids).to_string()
}

// ---- viewer ------------------------------------------------------------------

/// Read `/user`, store the user row and the `viewer` setting. Returns the login.
pub async fn viewer(gh: &Github, token: &str, db: &Arc<Db>) -> Result<String> {
    let me = gh.rest(token, Method::GET, "/user", None, &[]).await?.body;
    let (Some(id), Some(login)) = (me["id"].as_i64(), me["login"].as_str().map(str::to_owned)) else {
        return Err(crate::Error::Api { status: 200, message: "unexpected /user response".into() });
    };
    let l = login.clone();
    write(db, move |c| {
        let tx = c.transaction()?;
        user(&tx, &me)?;
        fl_db::queries::layout::set_setting(&tx, "viewer", &json!({ "id": id, "login": l }).to_string())?;
        tx.commit()?;
        Ok(())
    })
    .await?;
    Ok(login)
}

// ---- repo metadata ---------------------------------------------------------

/// Sync metadata of every accessible repo and the starred flags.
pub async fn repos(gh: &Github, token: &str, db: &Arc<Db>, now: i64, emit: Emit<'_>) -> Result<()> {
    let owned = list_cached(
        gh,
        token,
        db,
        "repos",
        "/user/repos?per_page=100&sort=updated&affiliation=owner,collaborator,organization_member",
        now,
    )
    .await?;
    let starred = list_cached(gh, token, db, "starred", "/user/starred?per_page=100", now).await?;
    if owned.is_none() && starred.is_none() {
        return Ok(());
    }
    write(db, move |c| {
        let tx = c.transaction()?;
        for r in owned.iter().flatten() {
            repo(&tx, r, true)?;
        }
        if let Some(starred) = &starred {
            let mut ids = Vec::with_capacity(starred.len());
            for r in starred {
                ids.extend(repo(&tx, r, true)?);
            }
            tx.execute(
                "UPDATE repos SET starred = (id IN (SELECT value FROM json_each(?1)))
                 WHERE starred <> (id IN (SELECT value FROM json_each(?1)))",
                [ids_json(&ids)],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
    .await?;
    emit(&["repos", "sync"]);
    Ok(())
}

// ---- notifications ---------------------------------------------------------

/// Poll the inbox. Honors `If-Modified-Since`; stores `X-Poll-Interval`.
pub async fn notifications(gh: &Github, token: &str, db: &Arc<Db>, now: i64, emit: Emit<'_>) -> Result<()> {
    let old = read(db, |c| Ok(state(c, "notifications")?)).await?;
    let headers: Vec<(&str, &str)> = old.last_modified.as_deref().map(|v| ("If-Modified-Since", v)).into_iter().collect();
    let mut resp = gh.rest(token, Method::GET, "/notifications?all=true&per_page=50", None, &headers).await?;
    let next = State {
        last_modified: resp.header("last-modified").map(str::to_owned),
        poll_interval: resp.header("x-poll-interval").and_then(|v| v.parse().ok()),
        ..touched(now)
    };
    let mut threads = Vec::new();
    if resp.status != 304 {
        for _ in 0..3 {
            threads.extend(resp.body.as_array().cloned().unwrap_or_default());
            let Some(url) = resp.next_link() else { break };
            resp = gh.rest(token, Method::GET, &url, None, &[]).await?;
        }
    }
    let changed = !threads.is_empty();
    write(db, move |c| {
        let tx = c.transaction()?;
        for t in &threads {
            write_notification(&tx, t)?;
        }
        save_state(&tx, "notifications", &next)?;
        tx.commit()?;
        Ok(())
    })
    .await?;
    emit(if changed { &["inbox", "repos", "sync"] } else { &["sync"] });
    Ok(())
}

fn write_notification(tx: &Transaction<'_>, t: &Value) -> Result<()> {
    let Some(id) = t["id"].as_str() else { return Ok(()) };
    let Some(repo_id) = repo(tx, &t["repository"], false)? else { return Ok(()) };
    let subject = &t["subject"];
    let number = subject["url"]
        .as_str()
        .filter(|u| u.contains("/issues/") || u.contains("/pulls/"))
        .and_then(|u| u.rsplit('/').next())
        .and_then(|n| n.parse::<i64>().ok());
    let issue_id: Option<i64> = match number {
        Some(n) => tx
            .prepare_cached("SELECT id FROM issues WHERE repo_id = ?1 AND number = ?2")?
            .query_row([repo_id, n], |r| r.get(0))
            .optional()?,
        None => None,
    };
    // Pending checks use the numeric thread id (see queue::describe).
    let row: i64 = id.parse().unwrap_or(0);
    tx.prepare_cached(
        "INSERT INTO notifications (id, repo_id, subject_type, issue_id, title, reason, unread, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET repo_id = excluded.repo_id, subject_type = excluded.subject_type,
           issue_id = excluded.issue_id, title = excluded.title, reason = excluded.reason,
           unread = CASE WHEN ?9 THEN unread ELSE excluded.unread END,
           done = CASE WHEN ?10 THEN done WHEN excluded.updated_at > updated_at THEN 0 ELSE done END,
           updated_at = excluded.updated_at",
    )?
    .execute(params![
        id,
        repo_id,
        subject["type"].as_str().unwrap_or("Unknown"),
        issue_id,
        subject["title"].as_str().unwrap_or(""),
        t["reason"].as_str().unwrap_or(""),
        t["unread"].as_bool().unwrap_or(true),
        t["updated_at"].as_str().map_or(0, parse_ts),
        is_pending(tx, "notifications", row, "unread")?,
        is_pending(tx, "notifications", row, "done")?,
    ])?;
    Ok(())
}

// ---- deep sync -------------------------------------------------------------

macro_rules! actor {
    () => {
        "fragment A on Actor{login avatarUrl ... on User{databaseId name} ... on Bot{databaseId}}"
    };
}
macro_rules! common {
    () => {
        "id databaseId number title body bodyHTML state createdAt updatedAt closedAt author{...A} milestone{title}
         labels(first:50){nodes{name}} assignees(first:20){nodes{databaseId login avatarUrl name}}
         comments(last:100){totalCount nodes{databaseId body bodyHTML createdAt updatedAt author{...A}}}"
    };
}

pub const ISSUES_QUERY: &str = concat!(
    "query($owner:String!,$name:String!,$after:String,$since:DateTime){repository(owner:$owner,name:$name){",
    "issues(first:50,after:$after,orderBy:{field:UPDATED_AT,direction:DESC},filterBy:{since:$since})",
    "{pageInfo{hasNextPage endCursor} nodes{",
    common!(),
    "}}}}",
    actor!()
);

pub const PULLS_QUERY: &str = concat!(
    "query($owner:String!,$name:String!,$after:String){repository(owner:$owner,name:$name){",
    "pullRequests(first:10,after:$after,orderBy:{field:UPDATED_AT,direction:DESC})",
    "{pageInfo{hasNextPage endCursor} nodes{",
    common!(),
    " isDraft mergeable mergeStateStatus reviewDecision baseRefName headRefName baseRefOid headRefOid
      additions deletions changedFiles
      reviewRequests(first:20){nodes{requestedReviewer{... on User{login} ... on Team{combinedSlug}}}}
      closingIssuesReferences(first:10){nodes{databaseId}}
      reviews(last:50){nodes{databaseId state bodyHTML submittedAt author{...A}}}
      reviewThreads(first:50){totalCount nodes{id isResolved isOutdated path diffSide line startLine originalLine
        comments(first:50){nodes{databaseId body bodyHTML createdAt author{...A} commit{oid} pullRequestReview{databaseId}}}}}
      commits(last:100){nodes{commit{oid messageHeadline authoredDate author{user{login}} statusCheckRollup{state}}}}
      head:commits(last:1){nodes{commit{oid statusCheckRollup{state contexts(first:50){nodes{
        ... on CheckRun{databaseId name status conclusion detailsUrl startedAt completedAt}}}}}}}",
    "}}}}",
    actor!()
);

pub const OVERVIEW_QUERY: &str = "query($owner:String!,$name:String!){repository(owner:$owner,name:$name){
  languages(first:10,orderBy:{field:SIZE,direction:DESC}){edges{size node{name}}}
  releases(first:5,orderBy:{field:CREATED_AT,direction:DESC}){nodes{databaseId tagName name publishedAt}}
  defaultBranchRef{target{... on Commit{history(first:20){nodes{oid messageHeadline authoredDate author{user{login}} statusCheckRollup{state}}}}}}}}";

/// Deep sync one pinned repo: labels, overview, issues, then PRs.
pub async fn deep(gh: &Github, token: &str, db: &Arc<Db>, repo_id: i64, now: i64, emit: Emit<'_>) -> Result<()> {
    let (owner, name): (String, String) =
        read(db, move |c| Ok(c.query_row("SELECT owner, name FROM repos WHERE id = ?1", [repo_id], |r| Ok((r.get(0)?, r.get(1)?)))?))
            .await?;
    let key = format!("repo:{repo_id}");

    // Labels first: issues reference them by name.
    let labels = list_cached(gh, token, db, &format!("{key}:labels"), &format!("/repos/{owner}/{name}/labels?per_page=100"), now).await?;
    let ov = gh.graphql(token, OVERVIEW_QUERY, json!({ "owner": owner, "name": name })).await?;
    write(db, move |c| {
        let tx = c.transaction()?;
        if let Some(labels) = &labels {
            write_labels(&tx, repo_id, labels)?;
        }
        write_overview(&tx, repo_id, &ov["repository"])?;
        tx.commit()?;
        Ok(())
    })
    .await?;
    emit(&["repos"]);

    for is_pr in [false, true] {
        let ckey = format!("{key}:{}", if is_pr { "pulls" } else { "issues" });
        let k = ckey.clone();
        let since = read(db, move |c| Ok(state(c, &k)?.cursor)).await?;
        let max_pages = if since.is_some() { MAX_PAGES } else { FIRST_SYNC_PAGES };
        let mut after: Option<String> = None;
        let mut newest = since.clone();
        for _ in 0..max_pages {
            let (query, conn_name) = if is_pr { (PULLS_QUERY, "pullRequests") } else { (ISSUES_QUERY, "issues") };
            let vars = json!({ "owner": owner, "name": name, "after": after, "since": since });
            let mut data = gh.graphql(token, query, vars).await?;
            let page = data["repository"][conn_name].take();
            let mut items: Vec<Value> = nodes(&page).to_vec();
            // PRs have no `since` filter: stop at the first item we already have.
            let mut reached_old = false;
            if is_pr && let Some(s) = &since {
                let before = items.len();
                items.retain(|n| n["updatedAt"].as_str().is_some_and(|u| u > s.as_str()));
                reached_old = items.len() < before;
            }
            for n in &items {
                if let Some(u) = n["updatedAt"].as_str()
                    && newest.as_deref().is_none_or(|cur| u > cur)
                {
                    newest = Some(u.to_owned());
                }
            }
            write(db, move |c| {
                let tx = c.transaction()?;
                for n in &items {
                    write_issue(&tx, repo_id, is_pr, n)?;
                }
                tx.commit()?;
                Ok(())
            })
            .await?;
            emit(&["issues", "pr", "search"]);
            let more = page["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false);
            after = page["pageInfo"]["endCursor"].as_str().map(str::to_owned);
            if !more || reached_old || after.is_none() {
                break;
            }
        }
        let s = State { cursor: newest, ..touched(now) };
        write(db, move |c| Ok(save_state(c, &ckey, &s)?)).await?;
    }
    write(db, move |c| Ok(save_state(c, &key, &touched(now))?)).await?;
    emit(&["sync"]);
    Ok(())
}

fn write_labels(tx: &Transaction<'_>, repo_id: i64, labels: &[Value]) -> rusqlite::Result<()> {
    let mut ids = Vec::with_capacity(labels.len());
    let mut stmt = tx.prepare_cached(
        "INSERT INTO labels (id, repo_id, name, color, description) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, color = excluded.color, description = excluded.description",
    )?;
    for l in labels {
        let (Some(id), Some(name)) = (l["id"].as_i64(), l["name"].as_str()) else { continue };
        stmt.execute(params![id, repo_id, name, l["color"].as_str().unwrap_or("888888"), l["description"].as_str()])?;
        ids.push(id);
    }
    tx.execute(
        "DELETE FROM labels WHERE repo_id = ?1 AND id NOT IN (SELECT value FROM json_each(?2))",
        params![repo_id, ids_json(&ids)],
    )?;
    Ok(())
}

fn write_overview(tx: &Transaction<'_>, repo_id: i64, r: &Value) -> rusqlite::Result<()> {
    let langs: Vec<Value> = r["languages"]["edges"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|e| json!({ "name": e["node"]["name"], "bytes": e["size"] }))
        .collect();
    tx.execute("UPDATE repos SET languages_json = ?1 WHERE id = ?2", params![json!(langs).to_string(), repo_id])?;
    for rel in nodes(&r["releases"]) {
        let (Some(id), Some(tag)) = (rel["databaseId"].as_i64(), rel["tagName"].as_str()) else { continue };
        tx.prepare_cached(
            "INSERT INTO releases (id, repo_id, tag, name, published_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET tag = excluded.tag, name = excluded.name, published_at = excluded.published_at",
        )?
        .execute(params![id, repo_id, tag, rel["name"].as_str(), rel["publishedAt"].as_str().map(parse_ts)])?;
    }
    for c in nodes(&r["defaultBranchRef"]["target"]["history"]) {
        write_commit(tx, repo_id, c, true)?;
    }
    Ok(())
}

fn write_commit(tx: &Transaction<'_>, repo_id: i64, c: &Value, default_branch: bool) -> rusqlite::Result<Option<String>> {
    let Some(oid) = c["oid"].as_str() else { return Ok(None) };
    tx.prepare_cached(
        "INSERT INTO commits (repo_id, oid, headline, author_login, authored_at, check_state, on_default_branch)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(repo_id, oid) DO UPDATE SET check_state = excluded.check_state,
           on_default_branch = max(on_default_branch, excluded.on_default_branch)",
    )?
    .execute(params![
        repo_id,
        oid,
        c["messageHeadline"].as_str().unwrap_or(""),
        c["author"]["user"]["login"].as_str(),
        c["authoredDate"].as_str().map_or(0, parse_ts),
        c["statusCheckRollup"]["state"].as_str(),
        default_branch,
    ])?;
    Ok(Some(oid.to_owned()))
}

fn state_of(s: &str) -> &'static str {
    match s {
        "MERGED" => "merged",
        "CLOSED" => "closed",
        _ => "open",
    }
}

/// Upsert one issue or PR node with its comments (and PR details), then
/// re-index it for search.
fn write_issue(tx: &Transaction<'_>, repo_id: i64, is_pr: bool, n: &Value) -> Result<()> {
    let (Some(id), Some(number)) = (n["databaseId"].as_i64(), n["number"].as_i64()) else { return Ok(()) };
    let author = user(tx, &n["author"])?;
    tx.prepare_cached(
        "INSERT INTO issues (id, node_id, repo_id, number, is_pr, state, title, body_html, body_md, author_id,
           milestone, comments_count, created_at, updated_at, closed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO UPDATE SET node_id = excluded.node_id,
           state = CASE WHEN ?16 THEN state ELSE excluded.state END,
           title = excluded.title, body_html = excluded.body_html, body_md = excluded.body_md,
           author_id = excluded.author_id, milestone = excluded.milestone,
           comments_count = excluded.comments_count, updated_at = excluded.updated_at,
           closed_at = excluded.closed_at",
    )?
    .execute(params![
        id,
        n["id"].as_str().unwrap_or(""),
        repo_id,
        number,
        is_pr,
        state_of(n["state"].as_str().unwrap_or("OPEN")),
        n["title"].as_str().unwrap_or(""),
        n["bodyHTML"].as_str(),
        n["body"].as_str(),
        author,
        n["milestone"]["title"].as_str(),
        n["comments"]["totalCount"].as_i64().unwrap_or(0),
        n["createdAt"].as_str().map_or(0, parse_ts),
        n["updatedAt"].as_str().map_or(0, parse_ts),
        n["closedAt"].as_str().map(parse_ts),
        is_pending(tx, "issues", id, "state")?,
    ])?;

    if !is_pending(tx, "issues", id, "labels")? {
        let mut find = tx.prepare_cached("SELECT id FROM labels WHERE repo_id = ?1 AND name = ?2")?;
        let mut ids = Vec::new();
        for l in nodes(&n["labels"]) {
            if let Some(lid) = find.query_row(params![repo_id, l["name"].as_str()], |r| r.get(0)).optional()? {
                ids.push(lid);
            }
        }
        set_links(tx, "issue_labels", "label_id", id, &ids)?;
    }
    if !is_pending(tx, "issues", id, "assignees")? {
        let mut ids = Vec::new();
        for a in nodes(&n["assignees"]) {
            ids.extend(user(tx, a)?);
        }
        set_links(tx, "issue_assignees", "user_id", id, &ids)?;
    }

    let comments = nodes(&n["comments"]);
    let mut seen = Vec::with_capacity(comments.len());
    for c in comments {
        let Some(cid) = c["databaseId"].as_i64() else { continue };
        let author = user(tx, &c["author"])?;
        tx.prepare_cached(
            "INSERT INTO comments (id, issue_id, author_id, body_html, body_md, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET author_id = excluded.author_id, body_html = excluded.body_html,
               body_md = excluded.body_md, updated_at = excluded.updated_at",
        )?
        .execute(params![
            cid,
            id,
            author,
            c["bodyHTML"].as_str(),
            c["body"].as_str(),
            c["createdAt"].as_str().map_or(0, parse_ts),
            c["updatedAt"].as_str().map_or(0, parse_ts),
        ])?;
        seen.push(cid);
    }
    // Only a complete list proves a comment was deleted.
    if n["comments"]["totalCount"].as_u64().is_some_and(|t| t as usize <= comments.len()) {
        tx.prepare_cached("DELETE FROM comments WHERE issue_id = ?1 AND id > 0 AND id NOT IN (SELECT value FROM json_each(?2))")?
            .execute(params![id, ids_json(&seen)])?;
    }
    if is_pr {
        write_pr(tx, repo_id, id, n)?;
    }
    fl_search::index_issue(tx, id)?;
    Ok(())
}

/// SQL: local optimistic rows of `table` that no action owns any more.
fn confirmed_local(table: &str) -> String {
    format!("id < 0 AND id NOT IN (SELECT row_id FROM pending_fields WHERE table_name = '{table}' AND field = 'row')")
}

fn write_pr(tx: &Transaction<'_>, repo_id: i64, id: i64, n: &Value) -> Result<()> {
    let head = nodes(&n["head"]).first().map(|h| &h["commit"]);
    tx.prepare_cached(
        "INSERT INTO pr_details (issue_id, base_ref, head_ref, base_oid, head_oid, is_draft, mergeable, merge_state,
           review_decision, check_state, additions, deletions, changed_files)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(issue_id) DO UPDATE SET base_ref = excluded.base_ref, head_ref = excluded.head_ref,
           base_oid = excluded.base_oid, head_oid = excluded.head_oid, is_draft = excluded.is_draft,
           mergeable = excluded.mergeable, merge_state = excluded.merge_state,
           review_decision = excluded.review_decision, check_state = excluded.check_state,
           additions = excluded.additions, deletions = excluded.deletions, changed_files = excluded.changed_files",
    )?
    .execute(params![
        id,
        n["baseRefName"].as_str().unwrap_or(""),
        n["headRefName"].as_str().unwrap_or(""),
        n["baseRefOid"].as_str().unwrap_or(""),
        n["headRefOid"].as_str().unwrap_or(""),
        n["isDraft"].as_bool().unwrap_or(false),
        n["mergeable"].as_str(),
        n["mergeStateStatus"].as_str(),
        n["reviewDecision"].as_str(),
        head.and_then(|h| h["statusCheckRollup"]["state"].as_str()),
        n["additions"].as_i64().unwrap_or(0),
        n["deletions"].as_i64().unwrap_or(0),
        n["changedFiles"].as_i64().unwrap_or(0),
    ])?;

    tx.prepare_cached("DELETE FROM pr_review_requests WHERE pr_id = ?1")?.execute([id])?;
    for r in nodes(&n["reviewRequests"]) {
        let rr = &r["requestedReviewer"];
        if let Some(login) = rr["login"].as_str().or(rr["combinedSlug"].as_str()) {
            tx.prepare_cached("INSERT OR IGNORE INTO pr_review_requests (pr_id, reviewer_login) VALUES (?1, ?2)")?
                .execute(params![id, login])?;
        }
    }

    tx.prepare_cached("DELETE FROM issue_links WHERE pr_id = ?1")?.execute([id])?;
    for i in nodes(&n["closingIssuesReferences"]) {
        tx.prepare_cached("INSERT OR IGNORE INTO issue_links (issue_id, pr_id) SELECT id, ?2 FROM issues WHERE id = ?1")?
            .execute(params![i["databaseId"].as_i64(), id])?;
    }

    for r in nodes(&n["reviews"]) {
        let Some(rid) = r["databaseId"].as_i64() else { continue };
        let author = user(tx, &r["author"])?;
        tx.prepare_cached(
            "INSERT INTO reviews (id, pr_id, author_id, state, body_html, submitted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET state = excluded.state, body_html = excluded.body_html,
               submitted_at = excluded.submitted_at",
        )?
        .execute(params![
            rid,
            id,
            author,
            r["state"].as_str().unwrap_or("COMMENTED"),
            r["bodyHTML"].as_str().filter(|b| !b.is_empty()),
            r["submittedAt"].as_str().map(parse_ts),
        ])?;
    }
    tx.execute(&format!("DELETE FROM reviews WHERE pr_id = ?1 AND {}", confirmed_local("reviews")), [id])?;

    let mut thread_ids = Vec::new();
    for t in nodes(&n["reviewThreads"]) {
        let comments = nodes(&t["comments"]);
        let Some(tid) = comments.first().and_then(|c| c["databaseId"].as_i64()) else { continue };
        tx.prepare_cached(
            "INSERT INTO review_threads (id, node_id, pr_id, path, side, line, start_line, original_line, commit_oid,
               is_resolved, is_outdated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET node_id = excluded.node_id, path = excluded.path, side = excluded.side,
               line = excluded.line, start_line = excluded.start_line, original_line = excluded.original_line,
               commit_oid = excluded.commit_oid,
               is_resolved = CASE WHEN ?12 THEN is_resolved ELSE excluded.is_resolved END,
               is_outdated = excluded.is_outdated",
        )?
        .execute(params![
            tid,
            t["id"].as_str().unwrap_or(""),
            id,
            t["path"].as_str().unwrap_or(""),
            if t["diffSide"] == "LEFT" { "LEFT" } else { "RIGHT" },
            t["line"].as_i64(),
            t["startLine"].as_i64(),
            t["originalLine"].as_i64(),
            comments[0]["commit"]["oid"].as_str(),
            t["isResolved"].as_bool().unwrap_or(false),
            t["isOutdated"].as_bool().unwrap_or(false),
            is_pending(tx, "review_threads", tid, "is_resolved")?,
        ])?;
        thread_ids.push(tid);
        for c in comments {
            let Some(cid) = c["databaseId"].as_i64() else { continue };
            let author = user(tx, &c["author"])?;
            tx.prepare_cached(
                "INSERT INTO review_comments (id, thread_id, review_id, author_id, body_html, body_md, created_at)
                 VALUES (?1, ?2, (SELECT id FROM reviews WHERE id = ?3), ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET review_id = excluded.review_id, body_html = excluded.body_html,
                   body_md = excluded.body_md",
            )?
            .execute(params![
                cid,
                tid,
                c["pullRequestReview"]["databaseId"].as_i64(),
                author,
                c["bodyHTML"].as_str(),
                c["body"].as_str(),
                c["createdAt"].as_str().map_or(0, parse_ts),
            ])?;
        }
    }
    // Server threads replace confirmed local ones; a partial list deletes nothing real.
    let complete = n["reviewThreads"]["totalCount"].as_u64().is_some_and(|t| t as usize <= thread_ids.len());
    tx.execute(
        &format!(
            "DELETE FROM review_threads WHERE pr_id = ?1 AND (({}) OR (?3 AND id > 0 AND id NOT IN (SELECT value FROM json_each(?2))))",
            confirmed_local("review_threads")
        ),
        params![id, ids_json(&thread_ids), complete],
    )?;
    tx.execute(
        &format!(
            "DELETE FROM review_comments WHERE thread_id IN (SELECT id FROM review_threads WHERE pr_id = ?1) AND {}",
            confirmed_local("review_comments")
        ),
        [id],
    )?;

    tx.prepare_cached("DELETE FROM pr_commits WHERE pr_id = ?1")?.execute([id])?;
    for (pos, c) in nodes(&n["commits"]).iter().enumerate() {
        if let Some(oid) = write_commit(tx, repo_id, &c["commit"], false)? {
            tx.prepare_cached("INSERT OR IGNORE INTO pr_commits (pr_id, oid, position) VALUES (?1, ?2, ?3)")?
                .execute(params![id, oid, pos as i64])?;
        }
    }

    if let Some(head) = head
        && let Some(oid) = head["oid"].as_str()
    {
        tx.prepare_cached("DELETE FROM checks WHERE repo_id = ?1 AND commit_oid = ?2")?.execute(params![repo_id, oid])?;
        for c in nodes(&head["statusCheckRollup"]["contexts"]) {
            let Some(cid) = c["databaseId"].as_i64() else { continue };
            tx.prepare_cached(
                "INSERT OR REPLACE INTO checks (id, repo_id, commit_oid, name, status, conclusion, url, started_at, completed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?
            .execute(params![
                cid,
                repo_id,
                oid,
                c["name"].as_str().unwrap_or(""),
                c["status"].as_str().unwrap_or("QUEUED"),
                c["conclusion"].as_str(),
                c["detailsUrl"].as_str(),
                c["startedAt"].as_str().map(parse_ts),
                c["completedAt"].as_str().map(parse_ts),
            ])?;
        }
    }
    Ok(())
}
