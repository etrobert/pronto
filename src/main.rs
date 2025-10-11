use std::{env, path::PathBuf};

fn main() {
    let path = env::current_dir().expect("Could not fetch current directory");

    let home = env::var_os("HOME").expect("HOME environment variable is not defined");

    let home_path = PathBuf::from(&home);

    match path.strip_prefix(home_path) {
        Ok(rest) if rest.as_os_str().is_empty() => {
            println!("~");
        }
        Ok(rest) => {
            println!("~/{}", rest.display());
        }
        _ => {
            println!("{}", path.display());
        }
    }
}
