//! PR state for a working directory, cached on disk.
//!
//! Fetching costs ~900ms, far too slow to render a prompt with, so the cache is
//! the source of truth and refreshing is someone else's problem: `--pr-refresh`
//! fills it, the prompt renders whatever is already there. The session switcher
//! reads the same cache through `--pr-summary`, so both surfaces reduce a check
//! rollup the same way.

use std::{
    env, fs,
    fs::{File, FileTimes},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, SystemTime},
};

/// A cache entry older than this is refetched. Short enough that a PR you just
/// opened, or a check that just flipped, shows up while you are still watching
/// for it. The prompt can only ever refresh the one directory you are in, so a
/// short window costs a bounded ~240 GraphQL points/hour against a 5000 budget.
const TTL: Duration = Duration::from_secs(30);

/// `isDraft` because a draft PR still reports `state: OPEN`.
const FIELDS: &str = "number,state,statusCheckRollup,isDraft";

pub enum State {
    Open,
    Draft,
    Merged,
    Closed,
}

pub enum Checks {
    None,
    Ok,
    Fail,
    Pending,
}

pub struct Summary {
    pub number: u64,
    pub state: State,
    pub checks: Checks,
}

impl Summary {
    /// `847\tOPEN\tok` — fields rather than a rendering, so each caller keeps
    /// its own. A prompt and a list row want different glyphs and different
    /// colour escapes, but there is only one right answer about the PR itself.
    pub fn fields(&self) -> String {
        let state = match self.state {
            State::Open => "OPEN",
            State::Draft => "DRAFT",
            State::Merged => "MERGED",
            State::Closed => "CLOSED",
        };
        let checks = match self.checks {
            Checks::None => "none",
            Checks::Ok => "ok",
            Checks::Fail => "fail",
            Checks::Pending => "pending",
        };
        format!("{}\t{}\t{}", self.number, state, checks)
    }
}

/// Where the fetched JSON for `dir`'s branch lives, and the branch itself.
struct Target {
    path: PathBuf,
    branch: String,
}

fn cache_dir() -> Option<PathBuf> {
    let base = match env::var_os("XDG_CACHE_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => PathBuf::from(env::var_os("HOME")?).join(".cache"),
    };
    Some(base.join("pronto/pr"))
}

/// (per-worktree git dir, shared common dir). A linked worktree's `.git` is a
/// file naming the former, whose `commondir` points back at the latter — which
/// is where `config`, and so the remote, lives.
fn git_dirs(start: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut dir = start.to_path_buf();
    let dot_git = loop {
        let candidate = dir.join(".git");
        if candidate.exists() {
            break candidate;
        }
        if !dir.pop() {
            return None;
        }
    };

    if dot_git.is_dir() {
        return Some((dot_git.clone(), dot_git));
    }

    let git_dir = PathBuf::from(
        fs::read_to_string(&dot_git)
            .ok()?
            .trim()
            .strip_prefix("gitdir: ")?,
    );
    let common = match fs::read_to_string(git_dir.join("commondir")) {
        Ok(relative) => git_dir.join(relative.trim()),
        Err(_) => git_dir.clone(),
    };
    Some((git_dir, common))
}

/// Read rather than shell out: `git config --get remote.origin.url` costs 2.4ms,
/// this costs 21µs, and the prompt pays it on every render. The tradeoff is that
/// `insteadOf` and `include` directives are ignored; disagreeing with git there
/// yields a duplicate cache entry, never wrong state.
fn remote_origin_url(common: &Path) -> Option<String> {
    let config = fs::read_to_string(common.join("config")).ok()?;
    let mut in_origin = false;

    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_origin = line == "[remote \"origin\"]";
        } else if in_origin
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "url"
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn branch(git_dir: &Path) -> Option<String> {
    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    Some(head.trim().strip_prefix("ref: refs/heads/")?.to_string())
}

/// `git@github.com:etrobert/setup.git` and `https://github.com/etrobert/setup`
/// both reduce to `etrobert/setup`, so the key does not depend on which form a
/// given checkout happens to use.
fn repo_slug(url: &str) -> String {
    let url = url.strip_suffix(".git").unwrap_or(url);
    let parts: Vec<&str> = url.split(['/', ':']).filter(|p| !p.is_empty()).collect();

    match parts.as_slice() {
        [.., owner, repo] => format!("{owner}/{repo}"),
        _ => url.to_string(),
    }
}

/// Anything outside the set collapses to `%`, which flattens the key to one
/// filename component — so no separator can escape the cache directory.
fn encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
            _ => '%',
        })
        .collect()
}

/// `None` whenever there is nothing to look up: no repository, a detached HEAD,
/// or a remote that is not there. Those cost nothing and never spawn `gh`.
fn target(dir: &Path) -> Option<Target> {
    let (git_dir, common) = git_dirs(dir)?;
    let branch = branch(&git_dir)?;
    let slug = repo_slug(&remote_origin_url(&common)?);
    let name = format!("{}%{}.json", encode(&slug), encode(&branch));

    Some(Target {
        path: cache_dir()?.join(name),
        branch,
    })
}

fn is_stale(path: &Path) -> bool {
    let fresh_since = || -> Option<bool> {
        let modified = fs::metadata(path).ok()?.modified().ok()?;
        Some(SystemTime::now().duration_since(modified).ok()? < TTL)
    };
    !fresh_since().unwrap_or(false)
}

/// Claim the entry before the fetch, so other panes in this worktree see a fresh
/// mtime and skip. Creates the file when it is missing, which is also how a cold
/// lookup avoids a stampede.
fn claim(path: &Path) -> std::io::Result<()> {
    let file = File::options().create(true).append(true).open(path)?;
    let now = SystemTime::now();
    file.set_times(FileTimes::new().set_modified(now))
}

fn gh_on_path() -> bool {
    env::var_os("PATH")
        .map(|path| env::split_paths(&path).any(|dir| dir.join("gh").is_file()))
        .unwrap_or(false)
}

/// Fetch into the cache unless the entry is still fresh. Idempotent, so the
/// switcher can fan this out over every session on each open without refetching.
pub fn refresh(dir: &Path) {
    let Some(target) = target(dir) else { return };
    if !is_stale(&target.path) {
        return;
    }

    let Some(parent) = target.path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() || claim(&target.path).is_err() {
        return;
    }

    let Ok(output) = Command::new("gh")
        .args(["pr", "view", &target.branch, "--json", FIELDS])
        .current_dir(dir)
        .output()
    else {
        return;
    };

    // A branch with no PR exits non-zero and prints nothing, which lands as an
    // empty entry: renders nothing, retried once the TTL is up, same as a
    // fetch that failed. None of the three need telling apart.
    let temp = target
        .path
        .with_extension(format!("{}.tmp", std::process::id()));
    if fs::write(&temp, &output.stdout).is_ok() {
        let _ = fs::rename(&temp, &target.path);
    }
}

/// A check is only judged by its conclusion once it has finished: a running
/// CheckRun has no conclusion, and reading that as "not a failure" is how a
/// half-finished CI run comes out looking green.
fn health(checks: &[&serde_json::Value]) -> Checks {
    if checks.is_empty() {
        return Checks::None;
    }

    let running = |check: &serde_json::Value| match check["status"].as_str() {
        Some(status) => status != "COMPLETED",
        None => matches!(check["state"].as_str(), Some("PENDING" | "EXPECTED")),
    };
    let failed = |check: &serde_json::Value| {
        let verdict = check["conclusion"].as_str().or(check["state"].as_str());
        matches!(
            verdict,
            Some(
                "FAILURE"
                    | "TIMED_OUT"
                    | "CANCELLED"
                    | "ACTION_REQUIRED"
                    | "STARTUP_FAILURE"
                    | "ERROR"
            )
        )
    };

    if checks.iter().any(|c| failed(c)) {
        Checks::Fail
    } else if checks.iter().any(|c| running(c)) {
        Checks::Pending
    } else {
        Checks::Ok
    }
}

/// Whatever the cache holds. Never fetches, so it is safe on the prompt path.
pub fn summary(dir: &Path) -> Option<Summary> {
    let target = target(dir)?;
    let raw = fs::read_to_string(&target.path).ok()?;
    if raw.trim().is_empty() {
        return None;
    }

    let pr: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let checks: Vec<&serde_json::Value> = pr["statusCheckRollup"]
        .as_array()
        .map(|checks| checks.iter().collect())
        .unwrap_or_default();

    Some(Summary {
        number: pr["number"].as_u64()?,
        state: match (pr["state"].as_str()?, pr["isDraft"].as_bool()) {
            ("MERGED", _) => State::Merged,
            ("CLOSED", _) => State::Closed,
            (_, Some(true)) => State::Draft,
            _ => State::Open,
        },
        checks: health(&checks),
    })
}

/// Render from cache, and kick off a detached refresh when it has gone stale.
/// Never blocks: this render shows the previous answer, the next one shows the
/// new. Spawning ourselves rather than `gh` directly keeps the fetch in one
/// place and lets this process exit immediately.
pub fn field(dir: &Path) -> Option<Summary> {
    let stale = target(dir).is_some_and(|target| is_stale(&target.path));

    if stale
        && gh_on_path()
        && let Ok(exe) = env::current_exe()
    {
        // Detach every stream: the child outlives us, and a prompt calls us
        // through `$(...)`, whose command substitution waits on any process
        // still holding the pipe — inheriting stdout would block the prompt on
        // the very fetch we are forking to avoid.
        let _ = Command::new(exe)
            .arg("--pr-refresh")
            .arg(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }

    summary(dir)
}
