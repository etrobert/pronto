use std::{
    env,
    fs::{self},
    path::PathBuf,
    process::Command,
};

fn get_path() -> String {
    let path = env::current_dir().expect("Could not fetch current directory");

    let home = env::var_os("HOME").expect("HOME environment variable is not defined");

    let home_path = PathBuf::from(&home);

    match path.strip_prefix(home_path) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
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

#[allow(dead_code, reason = "Keep for reference")]
fn get_git_status() -> String {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .expect("git branch failed");

    format!("({})", String::from_utf8_lossy(&output.stdout).trim())
}

fn get_git_status_file() -> Option<String> {
    let git_root = find_git_root()?;

    let file = fs::read_to_string(git_root.join(".git/HEAD")).expect("No HEAD file in .git dir");

    let branch = file
        .rsplit('/')
        .next()
        .expect("Could not parse file")
        .trim();

    format!(" ({})", branch).into()
}

fn main() {
    let path = get_path();

    let git_status = get_git_status_file().unwrap_or_default();

    const CYAN_COLOR: &str = "\x1b[36m";
    const RESET_COLOR: &str = "\x1b[0m";

    println!("{}{}{}{}", CYAN_COLOR, path, RESET_COLOR, git_status);
}
