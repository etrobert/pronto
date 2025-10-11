use std::{env, fs, path::PathBuf};

const HOME: &str = env!("HOME");

const RED_COLOR: &str = "\x1b[31m";
const CYAN_COLOR: &str = "\x1b[36m";
const RESET_COLOR: &str = "\x1b[0m";

fn get_path() -> String {
    let path = env::current_dir().expect("Could not fetch current directory");

    let home_path = PathBuf::from(HOME);

    let path = match path.strip_prefix(home_path) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
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

fn get_git_status() -> Option<String> {
    let git_root = find_git_root()?;

    let file = fs::read_to_string(git_root.join(".git/HEAD")).expect("No HEAD file in .git dir");

    let branch = file
        .rsplit('/')
        .next()
        .expect("Could not parse file")
        .trim();

    format!(" ({})", branch).into()
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
