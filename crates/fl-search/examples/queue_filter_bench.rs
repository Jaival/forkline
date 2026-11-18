//! `cargo run -p fl-search --release --example queue_filter_bench -- fake-data/forkline.db`
//! Times the `/` filter of each queue (first page + group counts) over the
//! `search_bench` query set. Prints p50/p95/max per queue kind, for the first
//! page (what the queue shows first) and for page + group counts.

use std::time::Instant;

use fl_db::queries::queue;
use fl_types::{PAGE_MAX, PageReq, QueueKind, QueueQuery};

const QUERIES: &[&str] = &[
    "fixture", "fixture item", "project 42", "gener", "fix*", "PR #7", "is:pr fixture",
    "is:issue is:open fixture", "-is:pr fixture", "author:octocat fixture", "repo:forkline-demo/project-007",
    "repo:forkline-demo/project-007 is:pr item", "is:pr", "author:octocat is:open", "label:bug fixture",
    "assignee:octocat", "fixture -generated", "\"local data\" project", "NEAR( AND \" * -", "zzzznomatch",
];

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "fake-data/forkline.db".into());
    let conn = fl_db::open_conn(std::path::Path::new(&path))?;
    for kind in QueueKind::ALL {
        let q = QueueQuery { kind, repo_id: None };
        let mut ms = Vec::new();
        let mut with_counts = Vec::new();
        for round in 0..11 {
            for text in QUERIES {
                let t = Instant::now();
                let cond = fl_search::queue_filter(&conn, text)?;
                let page = queue::page(&conn, &q, cond.as_ref(), PageReq { offset: 0, limit: PAGE_MAX })?;
                let first = t.elapsed().as_secs_f64() * 1000.0;
                let counts = queue::counts(&conn, &q, cond.as_ref())?;
                std::hint::black_box((page, counts));
                if round > 0 {
                    ms.push((first, *text));
                    with_counts.push(t.elapsed().as_secs_f64() * 1000.0);
                }
            }
        }
        with_counts.sort_by(f64::total_cmp);
        println!("{kind:?} with counts: p95 {:.2} ms", with_counts[with_counts.len() * 95 / 100]);
        if std::env::var_os("VERBOSE").is_some() {
            for text in QUERIES {
                let mut v: Vec<f64> = ms.iter().filter(|m| m.1 == *text).map(|m| m.0).collect();
                v.sort_by(f64::total_cmp);
                println!("  {:7.2} ms  {text}", v[v.len() / 2]);
            }
        }
        ms.sort_by(|a, b| a.0.total_cmp(&b.0));
        let p = |f: f64| ms[((ms.len() as f64 * f) as usize).min(ms.len() - 1)].0;
        let max = ms.last().unwrap();
        println!("{kind:?}: p95 {:.2} ms (p50 {:.2}, max {:.2} on {:?})", p(0.95), p(0.5), max.0, max.1);
    }
    Ok(())
}
