// Relocatable entry point: always run the pinned CLI with Dsivio's bundled Node.
use std::{
    env,
    process::{exit, Command},
};
fn main() {
    let exe = env::current_exe().expect("Cannot locate Playwright launcher");
    let root = exe.parent().unwrap().parent().unwrap();
    let node = root.parent().unwrap().join(if cfg!(windows) {
        "video-runtime/node/node.exe"
    } else {
        "video-runtime/node/bin/node"
    });
    let cli = root.join("playwright/node_modules/@playwright/cli/playwright-cli.js");
    let mut command = Command::new(&node);
    command.arg(cli).args(env::args_os().skip(1));
    let path = env::var_os("PATH").unwrap_or_default();
    if let Ok(path) = env::join_paths(
        std::iter::once(node.parent().unwrap().to_path_buf()).chain(env::split_paths(&path)),
    ) {
        command.env("PATH", path);
    }
    match command.status() {
        Ok(status) => exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("Cannot start bundled Playwright: {error}");
            exit(127)
        }
    }
}
