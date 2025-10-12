use std::{env, fs, path::PathBuf, process::Command};

const HOME: &str = env!("HOME");

// Colors wrapped with \x01 and \x02 (readline prompt ignore markers) to fix line wrapping
// See: https://stackoverflow.com/questions/24839271/bash-ps1-line-wrap-issue-with-non-printing-characters-from-an-external-command
const RED_COLOR: &str = "\x01\x1b[31m\x02";
const CYAN_COLOR: &str = "\x01\x1b[36m\x02";
const RESET_COLOR: &str = "\x01\x1b[0m\x02";

fn home_substitution(path: PathBuf) -> String {
    let home_path = PathBuf::from(HOME);

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

    format!("{}{}{}", CYAN_COLOR, path, RESET_COLOR)
}

fn find_git_root() -> Option<PathBuf> {
    let mut path = env::current_dir().ok()?;

    loop {
        if path.join(".git").is_dir() {
            return Some(path);
        }
        path = path.parent()?.to_path_buf()
    }
}

fn get_git_branch(git_root: PathBuf) -> String {
    match fs::read_to_string(git_root.join(".git/HEAD")) {
        Ok(file) => file
            .rsplit('/')
            .next()
            .expect("Could not parse file")
            .trim()
            .to_string(),
        Err(_) => "CORRUPT".to_string(),
    }
}

fn get_git_upstream() -> &'static str {
    let result = Command::new("git")
        .args(["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
        .output()
        .expect("Error calling git rev-list");

    // Branch probably has no upstream
    if !result.status.success() {
        return "";
    }

    let out_str = String::from_utf8_lossy(&result.stdout);

    let parts: Vec<&str> = out_str.split_whitespace().collect();

    let [ahead, behind] = parts.as_slice() else {
        panic!("git rev-list output does not match expected shape")
    };

    match (ahead, behind) {
        (&"0", &"0") => "",
        (&"0", _) => " ⬇︎",
        (_, &"0") => " ⬆︎",
        (_, _) => " ⬆︎⬇︎",
    }
}

fn get_git_status() -> Option<String> {
    let git_root = find_git_root()?;

    let branch = get_git_branch(git_root);

    let upstream = get_git_upstream();

    format!(" ({}{})", branch, upstream).into()
}

fn get_exit_code() -> Option<String> {
    let exit_code = env::args().nth(1).expect("Previous exit code missing");

    match exit_code.as_str() {
        "0" => None,
        _ => Some(format!(" {}[{}]{}", RED_COLOR, exit_code, RESET_COLOR)),
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
        time if time < 100 => Some(format!(" [{:02}ms]", time)),
        time if time < 1000 => Some(format!(" [.{}s]", time / 10)),
        time if time < MIN => Some(format!(" [{}.{}s]", time / 1000, time % 1000 / 100)),
        time if time < HOUR => Some(format!(" [{}m{}s]", time / MIN, time % MIN / 1000)),
        time => Some(format!(" [{}h{}m]", time / HOUR, time % HOUR / MIN)),
    }
}

fn main() {
    let path = get_path();

    let git_status = get_git_status().unwrap_or_default();

    let exit_code = get_exit_code().unwrap_or_default();

    let timing = get_timing().unwrap_or_default();

    println!("{}{}{}{}$ ", path, git_status, exit_code, timing);
}
