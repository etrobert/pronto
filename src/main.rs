use std::{env, path::PathBuf, process::Command};

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

fn get_git_status() -> String {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .expect("git status --porcelain failed");

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn main() {
    let path = get_path();

    let git_status = get_git_status();

    println!("{} {}", path, git_status);
}
