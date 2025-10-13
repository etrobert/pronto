use std::{env, path::PathBuf, process::Command};

// Colors wrapped with \x01 and \x02 (readline prompt ignore markers) to fix line wrapping
// See: https://stackoverflow.com/questions/24839271/bash-ps1-line-wrap-issue-with-non-printing-characters-from-an-external-command
const RED_COLOR: &str = "\x01\x1b[31m\x02";
const CYAN_COLOR: &str = "\x01\x1b[36m\x02";
const RESET_COLOR: &str = "\x01\x1b[0m\x02";

// Colors wrapped with %{...%} for zsh prompts
const RED_COLOR_ZSH: &str = "%{\x1b[31m%}";
const CYAN_COLOR_ZSH: &str = "%{\x1b[36m%}";
const RESET_COLOR_ZSH: &str = "%{\x1b[0m%}";
const DIM_COLOR_ZSH: &str = "%{\x1b[2m%}";

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

    format!("{}{}{}", CYAN_COLOR_ZSH, path, RESET_COLOR_ZSH)
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
            " {}[{}]{}",
            RED_COLOR_ZSH, exit_code, RESET_COLOR_ZSH
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

    let exit_code = get_exit_code().unwrap_or_default();

    format!("{}{}{}$ ", path, git_status, exit_code)
}

fn get_right_prompt() -> String {
    match get_timing() {
        Some(timing) => format!("{}{}{}", DIM_COLOR_ZSH, timing, RESET_COLOR_ZSH),
        None => String::new(),
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
