mod pr;

use gethostname::gethostname;
use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
    sync::LazyLock,
};

struct Colors {
    red: &'static str,
    green: &'static str,
    cyan: &'static str,
    reset: &'static str,
    dim: &'static str,
    /// Catppuccin Macchiato mauve, #c6a0f6. Truecolor because GitHub renders a
    /// merged PR purple and no ANSI slot holds one -- magenta is pink here.
    mauve: &'static str,
}

// Detect shell and use appropriate color wrappers
// Bash: \x01...\x02 (readline prompt ignore markers)
// Zsh: %{...%} (zsh non-printable markers)
// See: https://stackoverflow.com/questions/24839271/bash-ps1-line-wrap-issue-with-non-printing-characters-from-an-external-command
static COLORS: LazyLock<Colors> = LazyLock::new(|| {
    let (open, close) = if env::args().any(|arg| arg == "--zsh") {
        ("%{", "%}")
    } else {
        ("\x01", "\x02")
    };
    // String::leak: built once per process, and the fields stay &'static str.
    let wrap = |code: &str| -> &'static str { format!("{open}{code}{close}").leak() };

    Colors {
        red: wrap("\x1b[31m"),
        green: wrap("\x1b[32m"),
        cyan: wrap("\x1b[36m"),
        reset: wrap("\x1b[0m"),
        dim: wrap("\x1b[2m"),
        mauve: wrap("\x1b[38;2;198;160;246m"),
    }
});

fn home_substitution(path: PathBuf) -> String {
    let home_path = PathBuf::from(env::var("HOME").expect("HOME environment variable not defined"));

    match path.strip_prefix(home_path) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

fn tmux_substitution(path: &PathBuf) -> Option<String> {
    let tmux_session_path = PathBuf::from(env::var("TMUX_SESSION_PATH").ok()?);

    // The name tmux-sessionizer gave the session: the path tail under ~/work,
    // <repo>/<branch>. A session elsewhere -- doc -- keeps its directory name.
    let work = PathBuf::from(env::var("HOME").expect("HOME environment variable not defined"))
        .join("work");

    let session_name = match tmux_session_path.strip_prefix(work) {
        // ~/work itself strips to nothing, and would leave the prompt nameless.
        Ok(rest) if !rest.as_os_str().is_empty() => rest,
        _ => Path::new(
            tmux_session_path
                .file_name()
                .expect("Can't extract directory name from TMUX_SESSION_PATH"),
        ),
    }
    .to_str()
    .expect("Directory name in TMUX_SESSION_PATH is invalid");

    match path.strip_prefix(&tmux_session_path) {
        Ok(rest) if rest.as_os_str().is_empty() => Some(session_name.to_string()),
        Ok(rest) => Some(format!("{}/{}", session_name, rest.display())),
        _ => None,
    }
}

fn get_path() -> String {
    let path = match env::current_dir() {
        Ok(path) => tmux_substitution(&path).unwrap_or(home_substitution(path)),
        Err(_) => "???".to_string(),
    };

    format!("{}", path)
}

fn get_hostname() -> String {
    let hostname = gethostname().to_string_lossy().into_owned();
    hostname.split('.').next().unwrap_or(&hostname).to_owned()
}

fn parse_git_ab(ab: &str) -> String {
    let parts: Vec<&str> = ab.split_whitespace().collect();
    match parts.as_slice() {
        [ahead, behind] => {
            let ahead_number = ahead.strip_prefix("+").expect("ahead has wrong formatting");
            let behind_number = behind
                .strip_prefix("-")
                .expect("behind has wrong formatting");

            let ahead_str = match ahead_number {
                "0" => String::new(),
                "1" => " ↑".to_string(),
                n => format!(" ↑{}", n),
            };
            let behind_str = match behind_number {
                "0" => String::new(),
                "1" => " ↓".to_string(),
                n => format!(" ↓{}", n),
            };
            format!("{}{}", ahead_str, behind_str)
        }
        _ => panic!("Unexpected ab format: {}", ab),
    }
}

/// Returns the branch and everything the prompt shows after it. `dir` is None
/// for the prompt, which runs where the shell already is, and Some for the
/// session switcher, which asks about worktrees it is not sitting in.
fn get_git_status(dir: Option<&Path>) -> Option<(String, String)> {
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.arg("-C").arg(dir);
    }

    let result = command
        .args(["status", "--porcelain=v2", "--branch"])
        .output()
        .expect("error calling git status");

    if !result.status.success() {
        return None;
    }

    let out_str = String::from_utf8_lossy(&result.stdout);

    let mut branch: Option<&str> = None;
    let mut ab: Option<String> = None;
    let mut dirty_marker = "";

    for line in out_str.lines() {
        if let Some(out_branch) = line.strip_prefix("# branch.head ") {
            branch = Some(out_branch);
        } else if let Some(out_ab) = line.strip_prefix("# branch.ab ") {
            ab = Some(parse_git_ab(out_ab));
        } else if line.starts_with('1') // ordinary changed entries
                || line.starts_with('2') // renamed or copied entries
                || line.starts_with('u') // unmerged entries (conflicts)
                // untracked entries
                || line.starts_with('?')
        {
            dirty_marker = "*";
        }
    }

    Some((
        branch.unwrap_or("???").to_string(),
        format!("{}{}", ab.unwrap_or_default(), dirty_marker),
    ))
}

fn get_exit_code() -> Option<String> {
    let exit_code = env::args().nth(1).expect("Previous exit code missing");

    match exit_code.as_str() {
        "0" => None,
        _ => Some(format!(" {}{}{}", COLORS.red, exit_code, COLORS.reset)),
    }
}

const MIN: i32 = 60000;
const HOUR: i32 = 3600000;

fn get_timing() -> Option<String> {
    let time: i32 = env::args()
        .find_map(|arg| arg.strip_prefix("--cmd-duration=").map(str::to_owned))?
        .parse()
        .expect("--cmd-duration is not a valid i32");

    match time {
        time if time < 100 => Some(format!("{:02}ms", time)),
        time if time < 1000 => Some(format!(".{}s", time / 10)),
        time if time < MIN => Some(format!("{}.{}s", time / 1000, time % 1000 / 100)),
        time if time < HOUR => Some(format!("{}m{}s", time / MIN, time % MIN / 1000)),
        time => Some(format!("{}h{}m", time / HOUR, time % HOUR / MIN)),
    }
}

fn color(s: String, color: &str) -> String {
    format!("{}{}{}", color, s, COLORS.reset)
}

fn get_left_prompt() -> String {
    let exit_code = env::args().nth(1).expect("Previous exit code missing");
    let chevron_color = if exit_code == "0" {
        COLORS.reset
    } else {
        COLORS.red
    };
    let hostname = get_hostname();
    let path = get_path();

    let git_status = match get_git_status(None) {
        Some((branch, state)) => format!(" {}{}{}{}", COLORS.green, branch, COLORS.reset, state),
        None => String::new(),
    };

    format!(
        "{}{}{} {}{}{} {}»{} ",
        COLORS.dim,
        hostname,
        // reset seems necessary on darwin, otherwise everything is dim moving forward
        COLORS.reset,
        COLORS.cyan,
        path,
        git_status,
        chevron_color,
        COLORS.reset
    )
}

/// Only the check mark is coloured: it is the part you glance for, and the state
/// icons already differ by shape.
// GitHub's own icons, written as escapes so the private-use codepoints survive
// any editor. A wrong glyph still renders, so it would fail silently.
const PR_OPEN: &str = "\u{f407}"; // oct-git_pull_request
const PR_DRAFT: &str = "\u{f4dd}"; // oct-git_pull_request_draft
const PR_MERGED: &str = "\u{f419}"; // oct-git_merge
const PR_CLOSED: &str = "\u{f4dc}"; // oct-git_pull_request_closed

/// Colours follow GitHub's own: open green, merged purple, closed red, draft
/// grey. Shape alone is not enough -- four glyphs in one dim grey read as one
/// undifferentiated blob at prompt size.
fn get_pr() -> Option<String> {
    let summary = pr::field(&env::current_dir().ok()?)?;

    let (state, hue) = match summary.state {
        pr::State::Open => (PR_OPEN, COLORS.green),
        pr::State::Draft => (PR_DRAFT, COLORS.dim),
        pr::State::Merged => (PR_MERGED, COLORS.mauve),
        pr::State::Closed => (PR_CLOSED, COLORS.red),
    };
    // The number stays dim: it is a label, the glyph is the signal.
    let head = format!(
        "{} {}",
        color(format!("#{}", summary.number), COLORS.dim),
        color(state.to_string(), hue),
    );

    let mark = match summary.checks {
        pr::Checks::None => return Some(head),
        pr::Checks::Ok => color("\u{2713}".to_string(), COLORS.green),
        pr::Checks::Fail => color("\u{2717}".to_string(), COLORS.red),
        pr::Checks::Pending => color("\u{00b7}".to_string(), COLORS.dim),
    };

    Some(format!("{} {}", head, mark))
}

fn get_right_prompt() -> String {
    let exit_code = get_exit_code();
    let timing = get_timing();

    let status = match (exit_code, timing) {
        (None, None) => "".to_string(),
        (None, Some(timing)) => format!(" {}", color(timing, COLORS.dim)),
        (Some(exit_code), None) => exit_code,
        (Some(exit_code), Some(timing)) => {
            format!("{} {}in {}{}", exit_code, COLORS.dim, timing, COLORS.reset)
        }
    };

    match get_pr() {
        Some(pr) => format!("{}{}", pr, status),
        None => status,
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // The PR subcommands take a directory where the prompt takes an exit code:
    // the session switcher calls them for sessions it is not sitting in.
    match (args.get(1).map(String::as_str), args.get(2)) {
        (Some("--pr-refresh"), Some(dir)) => return pr::refresh(Path::new(dir)),
        (Some("--git-summary"), Some(dir)) => {
            let dir = Path::new(dir);
            // A directory that is not a repository is a normal answer -- the
            // switcher asks about every session. One that is not there at all
            // is the caller being wrong, and silence would read as "clean".
            assert!(dir.is_dir(), "not a directory: {}", dir.display());

            if let Some((_, state)) = get_git_status(Some(dir)) {
                print!("{}", state.trim_start());
            }
            return;
        }
        (Some("--pr-summary"), Some(dir)) => {
            if let Some(summary) = pr::summary(Path::new(dir)) {
                print!("{}", summary.fields());
            }
            return;
        }
        _ => {}
    }

    if args.iter().skip(2).any(|arg| arg == "--rprompt") {
        print!("{}", get_right_prompt());
    } else {
        print!("{}", get_left_prompt());
    }
}
