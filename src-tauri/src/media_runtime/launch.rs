//! `dsivio python|node|npm`: run the App's bundled Python 3.12 / Node 22 / npm for Skills, so a
//! clean Windows or macOS machine needs no system runtime. No PATH lookup: when the bundled
//! runtime is missing the command exits with [`MISSING_RUNTIME_EXIT`] and an explicit message,
//! and Skills fall back to their documented host-runtime path.
//!
//! `dsivio npm` installs `-g` packages into the App-private prefix [`npm_prefix`]
//! (`~/.kivio/npm-global`), never into a system Node. Fetching commands try the npmmirror
//! registry (and its Playwright browser CDN) first and retry once with the official sources.
use super::runtime;
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

pub const PYTHON_SUBCOMMAND: &str = "python";
pub const NODE_SUBCOMMAND: &str = "node";
pub const NPM_SUBCOMMAND: &str = "npm";

#[path = "../../../scripts/video-runtime/npm_policy.rs"]
mod npm_policy;
pub(crate) use npm_policy::{Source as NpmSource, NPM_MIRROR_REGISTRY, NPM_OFFICIAL_REGISTRY, PLAYWRIGHT_MIRROR_HOST};
/// "Command not found": the bundled runtime is absent, use the Skill's documented fallback.
pub const MISSING_RUNTIME_EXIT: u8 = 127;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Python,
    Node,
    Npm,
}

impl Tool {
    pub fn from_subcommand(first: &OsStr) -> Option<Self> {
        match first.to_str()? {
            PYTHON_SUBCOMMAND => Some(Self::Python),
            NODE_SUBCOMMAND => Some(Self::Node),
            NPM_SUBCOMMAND => Some(Self::Npm),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Node => "node",
            Self::Npm => "npm",
        }
    }

    fn missing_message(self, root: &Path) -> String {
        let fallback = match self {
            Self::Python => "临时可改用系统 Python 3.12+（Skill 的环境脚本会在私有 venv 中安装依赖）",
            Self::Node | Self::Npm => "临时可改用系统 Node.js 22+ 与 npm",
        };
        format!(
            "Dsivio 内置 {} 缺失（{}）。请重新安装或修复 Dsivio；{fallback}。",
            match self {
                Self::Python => "Python 3.12",
                Self::Node => "Node.js",
                Self::Npm => "npm",
            },
            root.display()
        )
    }
}

/// App-private prefix for `dsivio npm install -g` (and the Playwright CLI installer).
pub(crate) fn npm_prefix() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".kivio").join("npm-global"))
}

/// Where npm puts executables for a global prefix: the prefix itself on Windows, `bin` elsewhere.
pub(crate) fn npm_bin_dir(prefix: &Path) -> PathBuf {
    if cfg!(windows) {
        prefix.to_path_buf()
    } else {
        prefix.join("bin")
    }
}

fn npm_configuration(args: &[OsString], env: &dyn Fn(&str) -> Option<OsString>) -> npm_policy::Configuration {
    npm_policy::configuration(args, env, std::env::current_dir().ok().as_deref(),
        directories::BaseDirs::new().as_ref().map(|dirs| dirs.home_dir()))
}

pub(crate) fn npm_sources(args: &[OsString], env: impl Fn(&str) -> Option<OsString>) -> Vec<NpmSource> {
    npm_policy::sources(false, args, &npm_configuration(args, &env))
}

fn prepend_path(command: &mut Command, front: &[PathBuf], back: &[PathBuf]) -> Result<(), String> {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let existing: Vec<PathBuf> = std::env::split_paths(&existing)
        .map(crate::utils::strip_windows_verbatim_prefix)
        .filter(|entry| !front.contains(entry) && !back.contains(entry))
        .collect();
    let joined = std::env::join_paths(front.iter().cloned().chain(existing).chain(back.iter().cloned()))
        .map_err(|error| error.to_string())?;
    command.env("PATH", joined);
    Ok(())
}

fn tool(root: &Path, name: &str, owner: Tool) -> Result<PathBuf, String> {
    runtime::tools_at(root)?
        .remove(name)
        .ok_or_else(|| owner.missing_message(root))
}

/// The bundled Python with its shipped packages (openpyxl, Pillow, lxml, ...), isolated from the
/// user's PYTHONPATH/PYTHONHOME and user site. Bundled Node comes first on the child's PATH, so
/// report scripts that call `node` get Node 22 even on a machine without Node.
pub(crate) fn python_command(root: &Path, args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let python = tool(root, "python", Tool::Python)?;
    let mut command = Command::new(&python);
    command
        .args(args)
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .env("PYTHONNOUSERSITE", "1")
        // The shipped packages live inside the App bundle; never write bytecode there.
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8");
    // `pip install` would write into the App bundle; packages belong in a venv.
    if std::env::var_os("PIP_REQUIRE_VIRTUALENV").is_none() {
        command.env("PIP_REQUIRE_VIRTUALENV", "1");
    }
    let mut front = vec![python.parent().ok_or("无效的 Python 路径")?.to_path_buf()];
    if let Ok(node) = tool(root, "node", Tool::Node) {
        front.push(node.parent().ok_or("无效的 Node 路径")?.to_path_buf());
    }
    prepend_path(&mut command, &front, &npm_prefix().map(|p| vec![npm_bin_dir(&p)]).unwrap_or_default())?;
    Ok(command)
}

pub(crate) fn node_command(root: &Path, args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let node = tool(root, "node", Tool::Node)?;
    let mut command = Command::new(&node);
    command.args(args);
    prepend_path(
        &mut command,
        &[node.parent().ok_or("无效的 Node 路径")?.to_path_buf()],
        &npm_prefix().map(|p| vec![npm_bin_dir(&p)]).unwrap_or_default(),
    )?;
    Ok(command)
}

/// `<bundled node> <npm-cli.js> args` with the private global prefix and the given source.
pub(crate) fn npm_command(
    root: &Path,
    args: &[OsString],
    source: NpmSource,
    prefix: Option<&Path>,
) -> Result<Command, String> {
    let npm = tool(root, "npm", Tool::Npm)?;
    let mut command = node_command(root, std::iter::once(npm.into_os_string()).chain(args.iter().cloned()))?;
    // A global prefix inside the App bundle is read-only (and replaced on update).
    let configured = npm_configuration(args, &|key| std::env::var_os(key));
    if let (Some(prefix), false) = (prefix, configured.prefix) {
        command.env("npm_config_prefix", prefix);
    }
    command.env("npm_config_update_notifier", "false");
    match source {
        NpmSource::Mirror => {
            command.env("npm_config_registry", NPM_MIRROR_REGISTRY);
            if std::env::var_os("PLAYWRIGHT_DOWNLOAD_HOST").is_none() {
                command.env("PLAYWRIGHT_DOWNLOAD_HOST", PLAYWRIGHT_MIRROR_HOST);
            }
        }
        NpmSource::Official => {
            command.env("npm_config_registry", NPM_OFFICIAL_REGISTRY);
        }
        NpmSource::AsConfigured => {}
    }
    Ok(command)
}

fn exit_code(status: std::process::ExitStatus) -> ExitCode {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitCode::from(status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(1)).clamp(0, 255) as u8)
    }
    #[cfg(not(unix))]
    ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8)
}

fn inherit(command: &mut Command) -> &mut Command {
    command.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit())
}

/// Runs before App initialization, like `dsivio tools`.
pub fn run(tool: Tool, args: impl Iterator<Item = OsString>) -> ExitCode {
    let args: Vec<OsString> = args.collect();
    let root = match runtime::tools_resource_directory() {
        Ok(resources) => resources.join("video-runtime"),
        Err(error) => {
            eprintln!("dsivio {}: {error}", tool.name());
            return ExitCode::from(MISSING_RUNTIME_EXIT);
        }
    };
    let result = match tool {
        Tool::Python => python_command(&root, args).map(|command| vec![(None, command)]),
        Tool::Node => node_command(&root, args).map(|command| vec![(None, command)]),
        Tool::Npm => {
            let prefix = npm_prefix();
            npm_sources(&args, |key| std::env::var_os(key))
                .into_iter()
                .map(|source| npm_command(&root, &args, source, prefix.as_deref()).map(|command| (Some(source), command)))
                .collect()
        }
    };
    let attempts = match result {
        Ok(attempts) => attempts,
        Err(error) => {
            eprintln!("dsivio {}: {error}", tool.name());
            return ExitCode::from(MISSING_RUNTIME_EXIT);
        }
    };
    let labels: Vec<&str> = attempts.iter().map(|(source, _)| source.map_or("", NpmSource::label)).collect();
    let mut last = ExitCode::from(1);
    for (index, (_, mut command)) in attempts.into_iter().enumerate() {
        match inherit(&mut command).status() {
            Ok(status) if status.success() => return ExitCode::SUCCESS,
            Ok(status) => {
                last = exit_code(status);
                if let Some(next) = labels.get(index + 1) {
                    eprintln!("dsivio npm: 通过{}失败（{status}），改用{next}重试一次…", labels[index]);
                }
            }
            Err(error) => {
                eprintln!("dsivio {}: 无法运行内置 {}：{error}", tool.name(), tool.name());
                return ExitCode::from(1);
            }
        }
    }
    last
}

/// PATH directories the bundled runtime contributes, see `scripts/video-runtime/shim.rs`.
pub(crate) const TOOL_SHIMS_DIR: &str = "shims/tools";
pub(crate) const PYTHON_SHIMS_DIR: &str = "shims/python";

/// Make the bundled runtimes findable by bare name for the App's own children (agent shell,
/// Git Bash, PowerShell, MCP servers, plugin scripts): `shims/tools` (npm, npx, ffmpeg,
/// ffprobe), the bundled Node directory (node), `shims/python` (python, python3) and the
/// App-private npm prefix (`ziniao-cli`, `playwright-cli`, ... installed with `-g`).
///
/// Ordering: all of them go to the END of PATH, so a runtime the user installed keeps priority
/// and its globally installed packages/pip site stay the ones in use. The single exception is an
/// OS placeholder that would otherwise win for `python`/`python3` without running Python:
/// macOS `/usr/bin/python3` without the Command Line Tools (it only opens the CLT installer) and
/// the Windows Store "App execution alias" in `%LOCALAPPDATA%\Microsoft\WindowsApps` (it only
/// opens the Store). Then `shims/python` goes directly before that directory; everything that
/// already precedes it keeps priority, and nothing else is shadowed.
pub(crate) fn extend_process_path(root: &Path) {
    let mut appended = Vec::new();
    let tools = root.join(TOOL_SHIMS_DIR);
    if tools.is_dir() {
        appended.push(tools);
    }
    if let Ok(node) = tool(root, "node", Tool::Node) {
        if let Some(dir) = node.parent() {
            appended.push(dir.to_path_buf());
        }
    }
    let python = Some(root.join(PYTHON_SHIMS_DIR)).filter(|dir| dir.is_dir());
    let npm_bin = npm_prefix().map(|prefix| npm_bin_dir(&prefix));
    let key = if cfg!(windows) { "Path" } else { "PATH" };
    let current = std::env::var_os(key).unwrap_or_default();
    let stub = python_placeholder_dir(&current);
    if let Some(joined) = runtime_path(&current, &appended, python.as_deref(), npm_bin.as_deref(), stub.as_deref()) {
        std::env::set_var(key, joined);
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let normalize = |path: &Path| {
        let text = path.to_string_lossy();
        let text = text.trim_end_matches(['/', '\\']);
        if cfg!(windows) {
            text.replace('/', "\\").to_lowercase()
        } else {
            text.to_string()
        }
    };
    normalize(a) == normalize(b)
}

/// The new PATH (None when unchanged). Our directories are removed from `current` first, so the
/// result is idempotent and a placeholder that appears later still gets `python` placed before it.
fn runtime_path(
    current: &OsStr,
    appended: &[PathBuf],
    python: Option<&Path>,
    npm_bin: Option<&Path>,
    placeholder: Option<&Path>,
) -> Option<OsString> {
    let ours: Vec<&Path> = appended.iter().map(PathBuf::as_path).chain(python).chain(npm_bin).collect();
    let mut entries: Vec<PathBuf> = std::env::split_paths(current)
        .filter(|entry| !ours.iter().any(|dir| same_dir(entry, dir)))
        .collect();
    let before_placeholder = python.and_then(|_| placeholder).and_then(|stub| entries.iter().position(|entry| same_dir(entry, stub)));
    entries.extend(appended.iter().cloned());
    match (python, before_placeholder) {
        (Some(python), Some(index)) => entries.insert(index, python.to_path_buf()),
        (Some(python), None) => entries.push(python.to_path_buf()),
        (None, _) => {}
    }
    entries.extend(npm_bin.map(Path::to_path_buf));
    let joined = std::env::join_paths(entries).ok()?;
    (joined != current).then_some(joined)
}

/// The PATH directory whose `python`/`python3` is only an OS installer placeholder, if any.
#[cfg(target_os = "macos")]
fn python_placeholder_dir(_current: &OsStr) -> Option<PathBuf> {
    let xcode_select = std::fs::read_link("/var/db/xcode_select_link").ok();
    let developer_dirs = std::env::var_os("DEVELOPER_DIR")
        .map(PathBuf::from)
        .into_iter()
        .chain(xcode_select)
        .chain([
            PathBuf::from("/Library/Developer/CommandLineTools"),
            PathBuf::from("/Applications/Xcode.app/Contents/Developer"),
        ]);
    macos_placeholder(Path::new("/usr/bin/python3").exists(), developer_dirs.map(|dir| dir.join("usr/bin/python3").is_file()))
}

#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn macos_placeholder(usr_bin_python3: bool, mut developer_python: impl Iterator<Item = bool>) -> Option<PathBuf> {
    (usr_bin_python3 && !developer_python.any(|present| present)).then(|| PathBuf::from("/usr/bin"))
}

#[cfg(windows)]
fn python_placeholder_dir(_current: &OsStr) -> Option<PathBuf> {
    let apps = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Microsoft").join("WindowsApps");
    // Aliases are reparse points: probe without following them.
    let alias = ["python.exe", "python3.exe"].iter().any(|name| apps.join(name).symlink_metadata().is_ok());
    let names: Vec<String> = std::fs::read_dir(&apps)
        .map(|entries| entries.filter_map(Result::ok).map(|entry| entry.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    windows_placeholder(&apps, alias, &names)
}

/// The aliases belong to the Store installer unless a Python Software Foundation package
/// (a real Store Python) is installed.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn windows_placeholder(apps: &Path, alias: bool, names: &[String]) -> Option<PathBuf> {
    let store_python = names.iter().any(|name| name.starts_with("PythonSoftwareFoundation.Python."));
    (alias && !store_python).then(|| apps.to_path_buf())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn python_placeholder_dir(_current: &OsStr) -> Option<PathBuf> {
    None
}

/// True when `program` resolves (first on PATH) inside `dir`.
pub(crate) fn resolves_inside(program: &str, dir: &Path) -> bool {
    let names: Vec<String> = if cfg!(windows) {
        ["", ".exe", ".cmd", ".bat"].iter().map(|ext| format!("{program}{ext}")).collect()
    } else {
        vec![program.to_string()]
    };
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .find(|entry| names.iter().any(|name| entry.join(name).is_file()))
        .is_some_and(|entry| entry == dir)
}

/// The PATH launchers are built from `scripts/video-runtime/shim.rs` with plain `rustc`;
/// compiled here only so their pure helpers are unit-tested against this module.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../scripts/video-runtime/shim.rs"]
mod shim;

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
        let python = root.join(if cfg!(windows) { "python/python.exe" } else { "python/bin/python3" });
        let node = root.join(if cfg!(windows) { "node/node.exe" } else { "node/bin/node" });
        let npm = root.join(if cfg!(windows) {
            "node/node_modules/npm/bin/npm-cli.js"
        } else {
            "node/lib/node_modules/npm/bin/npm-cli.js"
        });
        for file in [&python, &node, &npm] {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, "fixture").unwrap();
        }
        (python, node, npm)
    }

    fn env_of<'a>(command: &'a Command, key: &str) -> Option<Option<&'a OsStr>> {
        // Windows environment keys are case-insensitive.
        command
            .get_envs()
            .find(|(k, _)| if cfg!(windows) { k.to_string_lossy().eq_ignore_ascii_case(key) } else { *k == OsStr::new(key) })
            .map(|(_, v)| v)
    }

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn subcommands_are_recognised_exactly() {
        assert_eq!(Tool::from_subcommand(OsStr::new("python")), Some(Tool::Python));
        assert_eq!(Tool::from_subcommand(OsStr::new("node")), Some(Tool::Node));
        assert_eq!(Tool::from_subcommand(OsStr::new("npm")), Some(Tool::Npm));
        assert_eq!(Tool::from_subcommand(OsStr::new("python3")), None);
        assert_eq!(Tool::from_subcommand(OsStr::new("media")), None);
    }

    #[test]
    fn relocated_python_is_isolated_and_puts_bundled_runtimes_first() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("moved app with spaces/video-runtime");
        let (python, node, _) = fixture(&root);
        let args = ["scripts/build_report.py", "report dir/report.json", "out%&!.xlsx"];
        let command = python_command(&root, args.iter().map(OsString::from)).unwrap();
        assert_eq!(command.get_program(), python.as_os_str());
        assert_eq!(command.get_args().collect::<Vec<_>>(), args.iter().map(OsStr::new).collect::<Vec<_>>());
        assert!(command.get_current_dir().is_none(), "the Skill directory is inherited");
        assert_eq!(env_of(&command, "PYTHONPATH"), Some(None));
        assert_eq!(env_of(&command, "PYTHONHOME"), Some(None));
        assert_eq!(env_of(&command, "PYTHONNOUSERSITE"), Some(Some(OsStr::new("1"))));
        assert_eq!(env_of(&command, "PYTHONUTF8"), Some(Some(OsStr::new("1"))));
        assert_eq!(env_of(&command, "PYTHONDONTWRITEBYTECODE"), Some(Some(OsStr::new("1"))));
        let path: Vec<PathBuf> = std::env::split_paths(env_of(&command, "PATH").unwrap().unwrap()).collect();
        assert_eq!(path[0], python.parent().unwrap());
        assert_eq!(path[1], node.parent().unwrap());
    }

    #[test]
    fn missing_runtime_is_reported_without_path_fallback() {
        let temp = tempfile::tempdir().unwrap();
        for result in [
            python_command(temp.path(), std::iter::empty()),
            node_command(temp.path(), std::iter::empty()),
            npm_command(temp.path(), &[], NpmSource::Mirror, None),
        ] {
            let error = result.unwrap_err();
            assert!(error.contains("内置") && error.contains("缺失"), "{error}");
        }
    }

    #[test]
    fn fetching_npm_commands_try_mirror_then_official() {
        let none = |_: &str| None;
        for args in [
            &["install", "-g", "@ziniao-open/cli@1.1.2"][..],
            &["ci"],
            &["view", "@playwright/cli", "version"],
        ] {
            assert_eq!(npm_sources(&os(args), none), vec![NpmSource::Mirror, NpmSource::Official], "{args:?}");
        }
    }

    #[test]
    fn executable_npm_commands_never_retry() {
        for command in ["exec", "x"] {
            assert_eq!(npm_sources(&os(&[command, "--", "tool"]), |_| None), vec![NpmSource::Mirror]);
        }
    }

    #[test]
    fn configured_registry_and_non_fetch_commands_are_left_alone() {
        let none = |_: &str| None;
        assert_eq!(npm_sources(&os(&["install", "--registry=https://r.example/"]), none), vec![NpmSource::AsConfigured]);
        assert_eq!(npm_sources(&os(&["install", "--registry", "https://r.example/"]), none), vec![NpmSource::AsConfigured]);
        let configured = |key: &str| (key == "npm_config_registry").then(|| OsString::from("https://r.example/"));
        assert_eq!(npm_sources(&os(&["ci"]), configured), vec![NpmSource::AsConfigured]);
        for args in [&["run", "build"][..], &["publish"], &["config", "list"], &[]] {
            assert_eq!(npm_sources(&os(args), none), vec![NpmSource::AsConfigured], "{args:?}");
        }
    }

    #[test]
    fn npm_uses_bundled_node_private_prefix_and_mirror_hosts() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("video-runtime");
        let (_, node, npm) = fixture(&root);
        let prefix = temp.path().join(".kivio/npm-global");
        let args = os(&["install", "-g", "@playwright/cli@latest"]);
        let mirror = npm_command(&root, &args, NpmSource::Mirror, Some(&prefix)).unwrap();
        assert_eq!(mirror.get_program(), node.as_os_str());
        assert_eq!(mirror.get_args().next().unwrap(), npm.as_os_str());
        assert_eq!(env_of(&mirror, "npm_config_prefix"), Some(Some(prefix.as_os_str())));
        assert_eq!(env_of(&mirror, "npm_config_registry"), Some(Some(OsStr::new(NPM_MIRROR_REGISTRY))));
        if std::env::var_os("PLAYWRIGHT_DOWNLOAD_HOST").is_none() {
            assert_eq!(env_of(&mirror, "PLAYWRIGHT_DOWNLOAD_HOST"), Some(Some(OsStr::new(PLAYWRIGHT_MIRROR_HOST))));
        }
        let official = npm_command(&root, &args, NpmSource::Official, Some(&prefix)).unwrap();
        assert_eq!(env_of(&official, "npm_config_registry"), Some(Some(OsStr::new(NPM_OFFICIAL_REGISTRY))));
        assert_eq!(env_of(&official, "PLAYWRIGHT_DOWNLOAD_HOST"), None);
        let configured = npm_command(&root, &os(&["run", "build"]), NpmSource::AsConfigured, Some(&prefix)).unwrap();
        assert_eq!(env_of(&configured, "npm_config_registry"), None);
        let explicit = npm_command(&root, &os(&["install", "-g", "--prefix=/elsewhere", "x"]), NpmSource::Mirror, Some(&prefix)).unwrap();
        assert_eq!(env_of(&explicit, "npm_config_prefix"), None);
    }

    #[test]
    fn private_bin_dir_matches_npm_global_layout() {
        let prefix = Path::new("home/.kivio/npm-global");
        assert_eq!(npm_bin_dir(prefix), if cfg!(windows) { prefix.to_path_buf() } else { prefix.join("bin") });
    }

    fn paths(entries: &[&str]) -> OsString {
        std::env::join_paths(entries.iter().map(PathBuf::from)).unwrap()
    }

    fn split(value: &OsStr) -> Vec<PathBuf> {
        std::env::split_paths(value).collect()
    }

    #[test]
    fn runtime_dirs_are_appended_after_user_runtimes_and_idempotent() {
        let user = paths(&["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]);
        let appended = [PathBuf::from("/app/video-runtime/shims/tools"), PathBuf::from("/app/video-runtime/node/bin")];
        let python = PathBuf::from("/app/video-runtime/shims/python");
        let npm_bin = PathBuf::from("/home/u/.kivio/npm-global/bin");
        let joined = runtime_path(&user, &appended, Some(&python), Some(&npm_bin), None).unwrap();
        assert_eq!(
            split(&joined),
            [
                "/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/app/video-runtime/shims/tools",
                "/app/video-runtime/node/bin", "/app/video-runtime/shims/python", "/home/u/.kivio/npm-global/bin",
            ]
            .map(PathBuf::from)
        );
        assert!(runtime_path(&joined, &appended, Some(&python), Some(&npm_bin), None).is_none(), "idempotent");
    }

    #[test]
    fn python_goes_only_before_an_os_placeholder() {
        let user = paths(&["/usr/bin", "/bin", "/usr/local/bin"]);
        let appended = [PathBuf::from("/app/shims/tools")];
        let python = PathBuf::from("/app/shims/python");
        let joined = runtime_path(&user, &appended, Some(&python), None, Some(Path::new("/usr/bin/"))).unwrap();
        assert_eq!(split(&joined), ["/app/shims/python", "/usr/bin", "/bin", "/usr/local/bin", "/app/shims/tools"].map(PathBuf::from));
        assert!(runtime_path(&joined, &appended, Some(&python), None, Some(Path::new("/usr/bin"))).is_none(), "idempotent");
        // A placeholder that is not on PATH changes nothing; stale copies of our entries are re-placed.
        let old = paths(&["/usr/local/bin", "/app/shims/python"]);
        let moved = runtime_path(&old, &appended, Some(&python), None, Some(Path::new("/nowhere"))).unwrap();
        assert_eq!(split(&moved), ["/usr/local/bin", "/app/shims/tools", "/app/shims/python"].map(PathBuf::from));
        // Without a bundled Python nothing is inserted before the placeholder.
        let none = runtime_path(&user, &appended, None, None, Some(Path::new("/usr/bin"))).unwrap();
        assert_eq!(split(&none)[0], PathBuf::from("/usr/bin"));
    }

    #[test]
    fn placeholder_detection_rules() {
        assert_eq!(macos_placeholder(true, [false, false].into_iter()), Some(PathBuf::from("/usr/bin")));
        assert_eq!(macos_placeholder(true, [false, true].into_iter()), None, "CLT or Xcode installed");
        assert_eq!(macos_placeholder(false, std::iter::empty()), None);
        let apps = Path::new("C:/Users/u/AppData/Local/Microsoft/WindowsApps");
        let names = |list: &[&str]| list.iter().map(|name| name.to_string()).collect::<Vec<_>>();
        assert_eq!(windows_placeholder(apps, true, &names(&["python.exe", "winget.exe"])), Some(apps.to_path_buf()));
        assert_eq!(windows_placeholder(apps, true, &names(&["python.exe", "PythonSoftwareFoundation.Python.3.12_qbz5n2kfra8p0"])), None);
        assert_eq!(windows_placeholder(apps, false, &names(&[])), None, "aliases turned off");
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_compare_case_and_separator_insensitively() {
        assert!(same_dir(Path::new(r"C:\Users\U\AppData\Local\Microsoft\WindowsApps\"), Path::new("c:/users/u/appdata/local/microsoft/windowsapps")));
    }

    #[test]
    fn shim_names_and_targets_match_the_bundled_layout() {
        for (name, tool) in [("npm", shim::Tool::Npm), ("npx", shim::Tool::Npx), ("python", shim::Tool::Python), ("python3", shim::Tool::Python), ("PYTHON3", shim::Tool::Python), ("ffmpeg", shim::Tool::Ffmpeg), ("ffprobe", shim::Tool::Ffprobe)] {
            assert_eq!(shim::tool_for(name), Some(tool), "{name}");
        }
        assert_eq!(shim::tool_for("node"), None, "node is reached through the bundled Node directory");
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("moved app/video-runtime");
        let (python, node, npm) = fixture(&root);
        let shim_file = root.join(TOOL_SHIMS_DIR).join(if cfg!(windows) { "npm.exe" } else { "npm" });
        assert_eq!(shim::runtime_root(&shim_file).unwrap(), root);
        assert_eq!(shim::runtime_root(&root.join(PYTHON_SHIMS_DIR).join("python3")).unwrap(), root);
        assert_eq!(shim::target(&root, shim::Tool::Npm, cfg!(windows)), (node.clone(), vec![npm.clone()]));
        assert_eq!(shim::target(&root, shim::Tool::Npx, cfg!(windows)).1, vec![npm.with_file_name("npx-cli.js")]);
        assert_eq!(shim::target(&root, shim::Tool::Python, cfg!(windows)).0, python);
        let tools = runtime::tools_at(&root).unwrap();
        assert_eq!(tools.get("npm"), Some(&npm), "same npm entry as `dsivio npm`");
        let ffmpeg = shim::target(&root, shim::Tool::Ffmpeg, cfg!(windows)).0;
        let ffprobe = shim::target(&root, shim::Tool::Ffprobe, cfg!(windows)).0;
        for file in [&ffmpeg, &ffprobe] {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, "fixture").unwrap();
        }
        let tools = runtime::tools_at(&root).unwrap();
        assert_eq!(tools.get("ffmpeg"), Some(&ffmpeg));
        assert_eq!(tools.get("ffprobe"), Some(&ffprobe));
    }

    #[test]
    fn shim_exec_and_npx_are_never_retried() {
        let none = |_: &str| None;
        for command in ["exec", "x"] {
            assert_eq!(shim::sources(shim::Tool::Npm, &os(&[command, "--", "tool"]), &none, None), vec![shim::Source::Mirror]);
        }
        assert_eq!(shim::sources(shim::Tool::Npx, &os(&["tool"]), &none, None), vec![shim::Source::Mirror]);
    }

    #[test]
    fn shim_environment_keeps_the_bundle_read_only() {
        let none = |_: &str| None;
        let home = Path::new("/home/u");
        let (set, remove) = shim::environment(shim::Tool::Npm, shim::Source::Mirror, &os(&["install", "-g", "x"]), &none, Some(home));
        let value = |key: &str| set.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone());
        assert_eq!(value("npm_config_prefix"), Some(home.join(".kivio").join("npm-global").into_os_string()));
        assert_eq!(value("npm_config_registry"), Some(OsString::from(NPM_MIRROR_REGISTRY)));
        assert!(remove.is_empty());
        let user_prefix = |key: &str| (key == "npm_config_prefix").then(|| OsString::from("/opt/npm"));
        let (set, _) = shim::environment(shim::Tool::Npm, shim::Source::Official, &os(&["i", "-g", "x"]), &user_prefix, Some(home));
        assert!(set.iter().all(|(key, _)| *key != "npm_config_prefix"), "a configured prefix is kept");
        assert!(set.iter().any(|(key, value)| *key == "npm_config_registry" && value == NPM_OFFICIAL_REGISTRY));
        let (set, _) = shim::environment(shim::Tool::Npx, shim::Source::AsConfigured, &os(&["--prefix=/p", "x"]), &none, Some(home));
        assert!(set.iter().all(|(key, _)| *key != "npm_config_prefix" && *key != "npm_config_registry"));
        let (set, remove) = shim::environment(shim::Tool::Python, shim::Source::AsConfigured, &[], &none, Some(home));
        assert_eq!(remove, ["PYTHONHOME", "PYTHONPATH"]);
        for key in ["PYTHONNOUSERSITE", "PYTHONUTF8", "PIP_REQUIRE_VIRTUALENV"] {
            assert!(set.iter().any(|(k, v)| *k == key && v == "1"), "{key}");
        }
        let opted_out = |key: &str| (key == "PIP_REQUIRE_VIRTUALENV").then(|| OsString::from("0"));
        let (set, _) = shim::environment(shim::Tool::Python, shim::Source::AsConfigured, &[], &opted_out, Some(home));
        assert!(set.iter().all(|(key, _)| *key != "PIP_REQUIRE_VIRTUALENV"));
        let (set, remove) = shim::environment(shim::Tool::Ffmpeg, shim::Source::AsConfigured, &[], &none, Some(home));
        assert!(set.is_empty() && remove.is_empty());
    }
}
