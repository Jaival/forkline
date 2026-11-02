//! `cargo run -p fl-search --release --example search_bench -- fake-data/forkline.db`
//! Prints p50/p95/max over a fixed query set, 20 rounds each.

use std::time::Instant;

use fl_types::PageReq;

const QUERIES: &[&str] = &[
    "fixture",
    "fixture item",
    "project 42",
    "gener",
    "fix*",
    "PR #7",
    "is:pr fixture",
    "is:issue is:open fixture",
    "-is:pr fixture",
    "author:octocat fixture",
    "repo:forkline-demo/project-007",
    "repo:forkline-demo/project-007 is:pr item",
    "is:pr",
    "author:octocat is:open",
    "label:bug fixture",
    "assignee:octocat",
    "fixture -generated",
    "\"local data\" project",
    "NEAR( AND \" * -",
    "zzzznomatch",
];

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "fake-data/forkline.db".into());
    let conn = fl_db::open_conn(std::path::Path::new(&path))?;
    let mut ms = Vec::new();
    for round in 0..21 {
        for q in QUERIES {
            let t = Instant::now();
            let page = fl_search::search(&conn, q, PageReq { offset: 0, limit: 50 })?;
            let e = t.elapsed().as_secs_f64() * 1000.0;
            std::hint::black_box(page);
            if round > 0 {
                ms.push((e, *q));
            }
        }
    }
    if std::env::var_os("VERBOSE").is_some() {
        for q in QUERIES {
            let mut v: Vec<f64> = ms.iter().filter(|m| m.1 == *q).map(|m| m.0).collect();
            v.sort_by(f64::total_cmp);
            println!("{:7.2} ms  {q}", v[v.len() / 2]);
        }
    }
    ms.sort_by(|a, b| a.0.total_cmp(&b.0));
    let p = |f: f64| ms[((ms.len() as f64 * f) as usize).min(ms.len() - 1)].0;
    println!("search_p95_ms: {:.2} (p50 {:.2}, max {:.2} on {:?}, n={})", p(0.95), p(0.5), ms.last().unwrap().0, ms.last().unwrap().1, ms.len());
    Ok(())
}
