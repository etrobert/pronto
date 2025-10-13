use std::{env, path::PathBuf, process::Command, sync::LazyLock};

struct Colors {
    red: &'static str,
    cyan: &'static str,
    reset: &'static str,
    dim: &'static str,
}

// Detect shell and use appropriate color wrappers
// Bash: \x01...\x02 (readline prompt ignore markers)
// Zsh: %{...%} (zsh non-printable markers)
// See: https://stackoverflow.com/questions/24839271/bash-ps1-line-wrap-issue-with-non-printing-characters-from-an-external-command
static COLORS: LazyLock<Colors> = LazyLock::new(|| {
    let is_zsh = env::args().any(|arg| arg == "--zsh");

    if is_zsh {
        Colors {
            red: "%{\x1b[31m%}",
            cyan: "%{\x1b[36m%}",
            reset: "%{\x1b[0m%}",
            dim: "%{\x1b[2m%}",
        }
    } else {
        Colors {
            red: "\x01\x1b[31m\x02",
            cyan: "\x01\x1b[36m\x02",
            reset: "\x01\x1b[0m\x02",
            dim: "\x01\x1b[2m\x02",
        }
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

    let session_name = tmux_session_path
        .file_name()
        .expect("Can't extract directory name from TMUX_SESSION_PATH")
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

    format!("{}{}{}", COLORS.cyan, path, COLORS.reset)
}

fn parse_git_ab(ab: &str) -> &str {
    let parts: Vec<&str> = ab.split_whitespace().collect();
    match parts.as_slice() {
        [ahead, behind] => {
            let ahead_number = ahead.strip_prefix("+").expect("ahead has wrong formatting");
            let behind_number = behind
                .strip_prefix("-")
                .expect("behind has wrong formatting");

            match (ahead_number, behind_number) {
                ("0", "0") => "",
                ("0", _) => " ⬇︎",
                (_, "0") => " ⬆︎",
                (_, _) => " ⬆︎⬇︎",
            }
        }
        _ => panic!("Unexpected ab format: {}", ab),
    }
}

fn get_git_status() -> Option<String> {
    let result = Command::new("git")
        .args(["status", "--porcelain=v2", "--branch"])
        .output()
        .expect("error calling git status");

    if !result.status.success() {
        return None;
    }

    let out_str = String::from_utf8_lossy(&result.stdout);

    let mut branch: Option<&str> = None;
    let mut ab: Option<&str> = None;

    for line in out_str.lines() {
        if let Some(out_branch) = line.strip_prefix("# branch.head ") {
            branch = Some(out_branch);
        } else if let Some(out_ab) = line.strip_prefix("# branch.ab ") {
            ab = Some(parse_git_ab(out_ab));
        }
    }

    format!(" ({}{})", branch.unwrap_or("???"), ab.unwrap_or_default()).into()
}

fn get_exit_code() -> Option<String> {
    let exit_code = env::args().nth(1).expect("Previous exit code missing");

    match exit_code.as_str() {
        "0" => None,
        _ => Some(format!(
            " {}{}{}",
            COLORS.red, exit_code, COLORS.reset
        )),
    }
}

const MIN: i32 = 60000;
const HOUR: i32 = 3600000;

fn get_timing() -> Option<String> {
    let last_cmd_time_str = env::var("LAST_CMD_TIME").ok()?;

    let time: i32 = last_cmd_time_str
        .parse()
        .expect("LAST_CMD_TIME is not a valid i32");

    match time {
        time if time < 100 => Some(format!("{:02}ms", time)),
        time if time < 1000 => Some(format!(".{}s", time / 10)),
        time if time < MIN => Some(format!("{}.{}s", time / 1000, time % 1000 / 100)),
        time if time < HOUR => Some(format!("{}m{}s", time / MIN, time % MIN / 1000)),
        time => Some(format!("{}h{}m", time / HOUR, time % HOUR / MIN)),
    }
}

fn get_left_prompt() -> String {
    let path = get_path();

    let git_status = get_git_status().unwrap_or_default();

    format!("{}{}$ ", path, git_status)
}

fn get_right_prompt() -> String {
    let exit_code = get_exit_code();
    let timing = get_timing();

    match (exit_code, timing) {
        (None, None) => "".to_string(),
        (None, Some(timing)) => format!("{}{}{}", COLORS.dim, timing, COLORS.reset),
        (Some(exit_code), None) => exit_code,
        (Some(exit_code), Some(timing)) => format!(
            "{} {}in {}{}",
            exit_code, COLORS.dim, timing, COLORS.reset
        ),
    }
}

fn main() {
    if env::args()
        .into_iter()
        .skip(2)
        .any(|arg| arg == "--rprompt")
    {
        print!("{}", get_right_prompt());
    } else {
        print!("{}", get_left_prompt());
    }
}
