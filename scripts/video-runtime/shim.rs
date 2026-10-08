// PATH entry points for the bundled runtime: npm, npx, python, python3, ffmpeg, ffprobe.
//
// Compiled once by scripts/build-video-runtime.mjs (plain `rustc`, no crates) and copied under
// each name into `video-runtime/shims/tools` (npm, npx, ffmpeg, ffprobe) and
// `video-runtime/shims/python` (python, python3); the file name selects the tool. The App puts
// these directories on PATH after the user's own entries (src-tauri/src/media_runtime/launch.rs),
// so a runtime the user installed keeps priority. src-tauri unit-tests the pure helpers below.
//
// - npm/npx run the bundled npm with the bundled Node. Global installs go to the App-private
//   prefix `~/.kivio/npm-global`, never into the App bundle. Fetching npm commands use the
//   npmmirror registry first and retry once with the official registry, like `dsivio npm`;
//   an explicit registry (argument, environment or .npmrc) is respected.
// - python/python3 run the bundled Python 3.12 isolated from PYTHONHOME/PYTHONPATH/user site,
//   and refuse `pip install` outside a virtual environment so the App bundle stays read-only.
// - A missing bundled runtime exits 127 ("command not found") with an explicit message.
use std::{
    env,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{exit, Command},
};

pub const NPM_MIRROR_REGISTRY: &str = "https://registry.npmmirror.com/";
pub const NPM_OFFICIAL_REGISTRY: &str = "https://registry.npmjs.org/";
pub const PLAYWRIGHT_MIRROR_HOST: &str = "https://cdn.npmmirror.com/binaries/playwright";
pub const MISSING_RUNTIME_EXIT: i32 = 127;
/// npm commands that only download and are safe to repeat against another registry.
pub const FETCH_COMMANDS: &[&str] = &[
    "install", "i", "add", "ci", "clean-install", "update", "up", "upgrade", "exec", "x", "view",
    "info", "show", "outdated",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Npm,
    Npx,
    Python,
    Ffmpeg,
    Ffprobe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Mirror,
    Official,
    AsConfigured,
}

/// The tool a shim file name (without extension) stands for.
pub fn tool_for(stem: &str) -> Option<Tool> {
    match stem.to_ascii_lowercase().as_str() {
        "npm" => Some(Tool::Npm),
        "npx" => Some(Tool::Npx),
        "python" | "python3" => Some(Tool::Python),
        "ffmpeg" => Some(Tool::Ffmpeg),
        "ffprobe" => Some(Tool::Ffprobe),
        _ => None,
    }
}

/// `<root>/shims/<group>/<shim>` -> `<root>`.
pub fn runtime_root(shim: &Path) -> Option<PathBuf> {
    Some(shim.parent()?.parent()?.parent()?.to_path_buf())
}

/// The bundled program and the files it is given before the user's arguments.
pub fn target(root: &Path, tool: Tool, windows: bool) -> (PathBuf, Vec<PathBuf>) {
    let node = root.join(if windows { "node/node.exe" } else { "node/bin/node" });
    let npm_bin = root.join(if windows { "node/node_modules/npm/bin" } else { "node/lib/node_modules/npm/bin" });
    match tool {
        Tool::Npm => (node, vec![npm_bin.join("npm-cli.js")]),
        Tool::Npx => (node, vec![npm_bin.join("npx-cli.js")]),
        Tool::Python => (root.join(if windows { "python/python.exe" } else { "python/bin/python3" }), vec![]),
        Tool::Ffmpeg => (
            root.join(if windows { "analyzer/node_modules/ffmpeg-static/ffmpeg.exe" } else { "analyzer/node_modules/ffmpeg-static/ffmpeg" }),
            vec![],
        ),
        Tool::Ffprobe => (root.join(if windows { "bin/ffprobe.exe" } else { "bin/ffprobe" }), vec![]),
    }
}

fn has_option(args: &[OsString], name: &str) -> bool {
    args.iter().filter_map(|arg| arg.to_str()).any(|arg| arg == name || arg.starts_with(&format!("{name}=")))
}

fn env_set(env: &dyn Fn(&str) -> Option<OsString>, keys: &[&str]) -> bool {
    keys.iter().any(|key| env(key).is_some_and(|value| !value.is_empty()))
}

/// True when an npmrc text sets the default registry (scoped `@x:registry` lines do not count).
pub fn npmrc_sets_registry(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start();
        line.split_once('=').is_some_and(|(key, _)| key.trim() == "registry")
    })
}

/// Attempts for one invocation, in order.
pub fn sources(tool: Tool, args: &[OsString], env: &dyn Fn(&str) -> Option<OsString>, npmrc_registry: bool) -> Vec<Source> {
    let configured = has_option(args, "--registry") || env_set(env, &["npm_config_registry", "NPM_CONFIG_REGISTRY"]) || npmrc_registry;
    match tool {
        Tool::Npm if !configured => {
            let command = args.iter().filter_map(|arg| arg.to_str()).find(|arg| !arg.starts_with('-'));
            if command.is_some_and(|command| FETCH_COMMANDS.contains(&command)) {
                vec![Source::Mirror, Source::Official]
            } else {
                vec![Source::AsConfigured]
            }
        }
        // npx runs the package after fetching it; repeating it could run the tool twice.
        Tool::Npx if !configured => vec![Source::Mirror],
        _ => vec![Source::AsConfigured],
    }
}

/// Environment changes for one attempt: (set, remove).
pub fn environment(
    tool: Tool,
    source: Source,
    args: &[OsString],
    env: &dyn Fn(&str) -> Option<OsString>,
    home: Option<&Path>,
) -> (Vec<(&'static str, OsString)>, Vec<&'static str>) {
    let mut set: Vec<(&'static str, OsString)> = Vec::new();
    let mut remove = Vec::new();
    match tool {
        Tool::Npm | Tool::Npx => {
            set.push(("npm_config_update_notifier", "false".into()));
            // The bundled npm's default global prefix is inside the App bundle.
            if !has_option(args, "--prefix") && !env_set(env, &["npm_config_prefix", "NPM_CONFIG_PREFIX"]) {
                if let Some(home) = home {
                    set.push(("npm_config_prefix", home.join(".kivio").join("npm-global").into_os_string()));
                }
            }
            match source {
                Source::Mirror => {
                    set.push(("npm_config_registry", NPM_MIRROR_REGISTRY.into()));
                    if !env_set(env, &["PLAYWRIGHT_DOWNLOAD_HOST"]) {
                        set.push(("PLAYWRIGHT_DOWNLOAD_HOST", PLAYWRIGHT_MIRROR_HOST.into()));
                    }
                }
                Source::Official => set.push(("npm_config_registry", NPM_OFFICIAL_REGISTRY.into())),
                Source::AsConfigured => {}
            }
        }
        Tool::Python => {
            remove.extend(["PYTHONHOME", "PYTHONPATH"]);
            set.push(("PYTHONNOUSERSITE", "1".into()));
            set.push(("PYTHONDONTWRITEBYTECODE", "1".into()));
            set.push(("PYTHONUTF8", "1".into()));
            set.push(("PYTHONIOENCODING", "utf-8".into()));
            if env("PIP_REQUIRE_VIRTUALENV").is_none() {
                set.push(("PIP_REQUIRE_VIRTUALENV", "1".into()));
            }
        }
        Tool::Ffmpeg | Tool::Ffprobe => {}
    }
    (set, remove)
}

fn label(tool: Tool) -> &'static str {
    match tool {
        Tool::Npm => "npm",
        Tool::Npx => "npx",
        Tool::Python => "Python 3.12",
        Tool::Ffmpeg => "ffmpeg",
        Tool::Ffprobe => "ffprobe",
    }
}

fn home() -> Option<PathBuf> {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    env::var_os(key).filter(|value| !value.is_empty()).map(PathBuf::from)
}

fn npmrc_registry(home: Option<&Path>) -> bool {
    let user = env::var_os("npm_config_userconfig")
        .or_else(|| env::var_os("NPM_CONFIG_USERCONFIG"))
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".npmrc")));
    let project = env::current_dir().ok().map(|dir| dir.join(".npmrc"));
    [user, project]
        .into_iter()
        .flatten()
        .any(|file| std::fs::read_to_string(file).is_ok_and(|text| npmrc_sets_registry(&text)))
}

fn shim_path() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    // Windows: keep the plain path; a `\\?\` prefix would leak into sys.executable and venvs.
    if cfg!(windows) {
        Some(exe)
    } else {
        std::fs::canonicalize(&exe).ok().or(Some(exe))
    }
}

#[cfg(windows)]
mod console {
    extern "system" {
        fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(u32) -> i32>, add: i32) -> i32;
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    }
    unsafe extern "system" fn ignore(_: u32) -> i32 {
        1
    }
    /// Ctrl+C reaches the child through the shared console; the shim waits for its exit code.
    /// A handler (unlike the NULL/ignore flag) is not inherited by the child.
    pub fn ignore_ctrl_c() {
        unsafe {
            SetConsoleCtrlHandler(Some(ignore), 1);
        }
    }
    pub fn attached() -> bool {
        let mut id = 0u32;
        unsafe { GetConsoleProcessList(&mut id, 1) != 0 }
    }
}

fn run(mut command: Command, last_attempt: bool) -> Result<std::process::ExitStatus, std::io::Error> {
    #[cfg(unix)]
    if last_attempt {
        use std::os::unix::process::CommandExt;
        return Err(command.exec());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Without a console of our own, do not let the console child open a window.
        if !console::attached() {
            command.creation_flags(0x0800_0000);
        }
    }
    let _ = last_attempt;
    command.status()
}

#[allow(dead_code)]
fn main() {
    let Some(shim) = shim_path() else {
        eprintln!("Dsivio: cannot locate this launcher");
        exit(MISSING_RUNTIME_EXIT);
    };
    let stem = shim.file_stem().and_then(OsStr::to_str).unwrap_or_default().to_string();
    let (Some(tool), Some(root)) = (tool_for(&stem), runtime_root(&shim)) else {
        eprintln!("Dsivio: unknown runtime launcher {}", shim.display());
        exit(MISSING_RUNTIME_EXIT);
    };
    let (program, lead) = target(&root, tool, cfg!(windows));
    if !program.is_file() || lead.iter().any(|file| !file.is_file()) {
        eprintln!("Dsivio 内置 {} 缺失（{}）。请重新安装或修复 Dsivio，或安装系统版本后重试。", label(tool), root.display());
        exit(MISSING_RUNTIME_EXIT);
    }
    #[cfg(windows)]
    console::ignore_ctrl_c();
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let home = home();
    let read_env = |key: &str| env::var_os(key);
    let npm = matches!(tool, Tool::Npm | Tool::Npx);
    let attempts = sources(tool, &args, &read_env, npm && npmrc_registry(home.as_deref()));
    let mut code = 1;
    for (index, source) in attempts.iter().enumerate() {
        let mut command = Command::new(&program);
        command.args(&lead).args(&args);
        let (set, remove) = environment(tool, *source, &args, &read_env, home.as_deref());
        for key in remove {
            command.env_remove(key);
        }
        command.envs(set);
        let last = index + 1 == attempts.len();
        match run(command, last) {
            Ok(status) if status.success() => exit(0),
            Ok(status) => {
                let Some(exit_code) = status.code() else {
                    #[cfg(unix)]
                    {
                        use std::os::unix::process::ExitStatusExt;
                        exit(128 + status.signal().unwrap_or(1));
                    }
                    #[cfg(not(unix))]
                    exit(1);
                };
                code = exit_code;
                if !last {
                    eprintln!("Dsivio {}: 通过 npmmirror 镜像失败（退出码 {exit_code}），改用 npm 官方源重试一次…", label(tool));
                }
            }
            Err(error) => {
                eprintln!("Dsivio: 无法运行内置 {}（{}）：{error}", label(tool), program.display());
                exit(126);
            }
        }
    }
    exit(code);
}
