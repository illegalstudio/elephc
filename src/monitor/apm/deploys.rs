//! Purpose:
//! Deploys: the moments a service started serving a build it had not served before. A probe
//! build carries its identity (when it was built, and its version or commit) in every
//! snapshot; the collector notes each new one, so the page can mark it on its charts and
//! compare what came before with what came after.
//!
//! Called from:
//! - The collector (`absorb`), for every snapshot that carries a build.
//! - The page's API (`api::series_json`), for the deploys of a period.
//!
//! Key details:
//! - A deploy is keyed by service, not by source: every process of a pushing program is a
//!   source of its own, and a restart is not a deploy. A polled service is its address; a
//!   pushing program is its name, all its hosts and processes together.
//! - Only a build the service never served before is a deploy. During a rolling deploy the
//!   old and the new build answer in turns, and that is one deploy, not one per answer.
//! - The store keeps them in `deploys.jsonl`, one line each, appended. Pruning keeps the
//!   service's latest deploy whatever its age, so the build serving is still known.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The store file deploys are kept in.
pub(crate) const DEPLOYS_FILE: &str = "deploys.jsonl";

/// One service starting to serve a new build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Deploy {
    /// When the collector first saw the build, Unix milliseconds, by its clock.
    pub t: u64,
    /// The service: the address a collector polls, or a pushing program's name.
    pub service: String,
    /// The build's identity as its snapshots carry it, `<unix seconds> <label>`.
    pub build: String,
}

impl Deploy {
    /// When the build was made, Unix seconds; 0 when the identity does not say.
    pub(crate) fn built_at(&self) -> u64 {
        self.build.split(' ').next().and_then(|seconds| seconds.parse().ok()).unwrap_or(0)
    }

    /// The build's version or commit; empty when it has none.
    pub(crate) fn label(&self) -> &str {
        self.build.split_once(' ').map_or("", |(_, label)| label.trim())
    }
}

/// The service a source belongs to: a pushing program's name, otherwise the source itself.
pub(crate) fn service_of(source: &str) -> &str {
    if super::collector::is_pushed_source(source) {
        source.split_once('@').map_or(source, |(program, _)| program)
    } else {
        source
    }
}

/// The deploys a collector knows, and the builds each service served.
#[derive(Debug, Default)]
pub(crate) struct Deploys {
    list: Vec<Deploy>,
    served: BTreeMap<String, BTreeSet<String>>,
}

impl Deploys {
    /// The deploys a store held, oldest first.
    pub(crate) fn from_list(mut list: Vec<Deploy>) -> Self {
        list.sort_by_key(|deploy| deploy.t);
        let mut deploys = Self::default();
        for deploy in list {
            deploys.served.entry(deploy.service.clone()).or_default().insert(deploy.build.clone());
            deploys.list.push(deploy);
        }
        deploys
    }

    /// Notes that `source` served `build` at `t`: the deploy, when the service never served
    /// that build before. A source without a name (a script's run) has no service.
    pub(crate) fn note(&mut self, source: &str, build: &str, t: u64) -> Option<Deploy> {
        let service = service_of(source);
        if service.is_empty() || build.is_empty() {
            return None;
        }
        let served = self.served.entry(service.to_string()).or_default();
        if !served.insert(build.to_string()) {
            return None;
        }
        let deploy = Deploy { t, service: service.to_string(), build: build.to_string() };
        self.list.push(deploy.clone());
        Some(deploy)
    }

    /// The deploys between `from` and `to`, milliseconds, oldest first.
    pub(crate) fn between(&self, from: u64, to: u64) -> Vec<Deploy> {
        self.list.iter().filter(|deploy| deploy.t >= from && deploy.t <= to).cloned().collect()
    }
}

/// Appends one deploy to the store's file.
pub(crate) fn append(dir: &Path, deploy: &Deploy) -> Result<(), String> {
    let _lock = lock(dir);
    let path = dir.join(DEPLOYS_FILE);
    let mut line = serde_json::to_string(deploy).map_err(|error| error.to_string())?;
    line.push('\n');
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| file.write_all(line.as_bytes()))
        .map_err(|error| format!("{}: {error}", path.display()))
}

/// Holds the store's deploys file for one append or one rewrite, until dropped. `None`
/// when the lock file cannot be opened: the write goes ahead unguarded, as before.
fn lock(dir: &Path) -> Option<std::fs::File> {
    use fs2::FileExt as _;
    let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(".deploys.lock")).ok()?;
    file.lock_exclusive().ok()?;
    Some(file)
}

/// The deploys a store holds; a line that does not read (a torn last line) is skipped.
pub(crate) fn read(dir: &Path) -> Vec<Deploy> {
    let Ok(text) = std::fs::read_to_string(dir.join(DEPLOYS_FILE)) else {
        return Vec::new();
    };
    text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect()
}

/// Drops the deploys older than `keep_from`, milliseconds, but each service's latest.
pub(crate) fn prune(dir: &Path, keep_from: u64) {
    // Under the lock appends take: a deploy another collector appended between this
    // read and the rename was lost with the rewrite (review).
    let _lock = lock(dir);
    let all = read(dir);
    let mut latest: BTreeMap<&str, u64> = BTreeMap::new();
    for deploy in &all {
        let t = latest.entry(deploy.service.as_str()).or_default();
        *t = (*t).max(deploy.t);
    }
    let kept: Vec<&Deploy> = all
        .iter()
        .filter(|deploy| deploy.t >= keep_from || latest.get(deploy.service.as_str()) == Some(&deploy.t))
        .collect();
    if kept.len() == all.len() {
        return;
    }
    let mut text = String::new();
    for deploy in kept {
        if let Ok(line) = serde_json::to_string(deploy) {
            text.push_str(&line);
            text.push('\n');
        }
    }
    // Written aside and renamed: a crash leaves the old file or the new one, never half.
    let partial = dir.join(format!("{DEPLOYS_FILE}.partial"));
    if std::fs::write(&partial, text).is_ok() {
        let _ = std::fs::rename(&partial, dir.join(DEPLOYS_FILE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new build is a deploy once per service; a restart, a second process or the old
    /// build answering again during a rolling deploy is not.
    #[test]
    fn a_new_build_is_one_deploy_per_service() {
        let mut deploys = Deploys::default();
        let a = "shop@10.0.0.1:5000 pid 41 #0000abcd";
        let b = "shop@10.0.0.2:5000 pid 7 #0000beef";
        assert!(deploys.note(a, "1763540000 v1", 1_000).is_some(), "first build seen");
        assert!(deploys.note(b, "1763540000 v1", 2_000).is_none(), "same service, same build");
        let deploy = deploys.note(a, "1763550000 v2", 3_000).expect("a new build");
        assert_eq!((deploy.service.as_str(), deploy.built_at(), deploy.label()), ("shop", 1_763_550_000, "v2"));
        assert!(deploys.note(b, "1763540000 v1", 4_000).is_none(), "the old build, still answering");
        assert!(deploys.note("10.0.0.9:9411", "1763540000 v1", 5_000).is_some(), "another service");
        assert!(deploys.note("", "1763540000 v1", 6_000).is_none(), "a script's run");
        assert_eq!(deploys.between(2_500, 5_000).len(), 2);
    }

    /// The store keeps deploys across collectors, and pruning keeps each service's latest.
    #[test]
    fn stored_deploys_survive_and_prune_to_the_latest() {
        let dir = std::env::temp_dir().join(format!("elephc-deploys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (t, service, build) in [(1, "shop", "1 v1"), (2, "api", "1 a1"), (3, "shop", "2 v2"), (9, "shop", "3 v3")] {
            append(&dir, &Deploy { t, service: service.into(), build: build.into() }).unwrap();
        }
        let mut known = Deploys::from_list(read(&dir));
        assert!(known.note("shop", "2 v2", 10).is_none(), "a build the store knew");
        prune(&dir, 5);
        let kept: Vec<(u64, String)> = read(&dir).into_iter().map(|d| (d.t, d.service)).collect();
        assert_eq!(kept, vec![(2, "api".to_string()), (9, "shop".to_string())]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
