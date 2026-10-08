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

pub(crate) const NPM_MIRROR_REGISTRY: &str = "https://registry.npmmirror.com/";
pub(crate) const NPM_OFFICIAL_REGISTRY: &str = "https://registry.npmjs.org/";
/// npmmirror's copy of the Playwright browser CDN (`builds/cft/...`, `builds/chromium/...`).
pub(crate) const PLAYWRIGHT_MIRROR_HOST: &str = "https://cdn.npmmirror.com/binaries/playwright";
/// "Command not found": the bundled runtime is absent, use the Skill's documented fallback.
pub const MISSING_RUNTIME_EXIT: u8 = 127;

/// npm commands that only download and are safe to repeat against another registry.
const FETCH_COMMANDS: &[&str] = &[
    "install", "i", "add", "ci", "clean-install", "update", "up", "upgrade", "exec", "x", "view",
    "info", "show", "outdated",
];

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

/// One npm attempt: which registry and Playwright download host to impose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NpmSource {
    /// npmmirror registry + npmmirror Playwright CDN.
    Mirror,
    /// registry.npmjs.org + Playwright's own CDN.
    Official,
    /// The user configured a registry (or the command does not fetch): change nothing.
    AsConfigured,
}

impl NpmSource {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Mirror => "npmmirror 镜像",
            Self::Official => "官方源",
            Self::AsConfigured => "已配置的源",
        }
    }
}

fn npm_command_name(args: &[OsString]) -> Option<&str> {
    args.iter().filter_map(|arg| arg.to_str()).find(|arg| !arg.starts_with('-'))
}

/// Attempts for one `npm` invocation, in order. An explicit `--registry` argument or a
/// `npm_config_registry` environment variable is respected as is, and commands that do not
/// fetch (run, publish, config, ...) never get a registry imposed.
pub(crate) fn npm_sources(
    args: &[OsString],
    env: impl Fn(&str) -> Option<OsString>,
) -> Vec<NpmSource> {
    let configured = args
        .iter()
        .any(|arg| arg.to_str().is_some_and(|arg| arg == "--registry" || arg.starts_with("--registry=")))
        || ["npm_config_registry", "NPM_CONFIG_REGISTRY"]
            .iter()
            .any(|key| env(key).is_some_and(|value| !value.is_empty()));
    let fetches = npm_command_name(args).is_some_and(|command| FETCH_COMMANDS.contains(&command));
    if configured || !fetches {
        vec![NpmSource::AsConfigured]
    } else {
        vec![NpmSource::Mirror, NpmSource::Official]
    }
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
    let explicit_prefix = args.iter().any(|arg| arg.to_str().is_some_and(|arg| arg == "--prefix" || arg.starts_with("--prefix=")));
    if let (Some(prefix), false) = (prefix, explicit_prefix) {
        // Unix environments are case-sensitive and npm reads both spellings.
        #[cfg(not(windows))]
        command.env_remove("NPM_CONFIG_PREFIX");
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

/// Make the App-private npm executables and the bundled Node findable by the App's own
/// children (agent shell, MCP servers, computer-control installers). Both go to the END of
/// PATH, so a runtime the user installed keeps priority; on a clean machine `node`,
/// `playwright-cli` and `ziniao-cli` (whose npm shims need `node`) still resolve.
pub(crate) fn extend_process_path(root: &Path) {
    let mut extra = Vec::new();
    if let Ok(node) = tool(root, "node", Tool::Node) {
        if let Some(dir) = node.parent() {
            extra.push(dir.to_path_buf());
        }
    }
    if let Some(prefix) = npm_prefix() {
        extra.push(npm_bin_dir(&prefix));
    }
    let key = if cfg!(windows) { "Path" } else { "PATH" };
    let current = std::env::var_os(key).unwrap_or_default();
    if let Some(joined) = appended_path(&current, &extra) {
        std::env::set_var(key, joined);
    }
}

fn appended_path(current: &OsStr, extra: &[PathBuf]) -> Option<OsString> {
    let existing: Vec<PathBuf> = std::env::split_paths(current).collect();
    let missing: Vec<&PathBuf> = extra.iter().filter(|dir| !existing.contains(dir)).collect();
    if missing.is_empty() {
        return None;
    }
    std::env::join_paths(existing.iter().chain(missing)).ok()
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
            &["exec", "--", "playwright", "install", "chromium"],
            &["view", "@playwright/cli", "version"],
        ] {
            assert_eq!(npm_sources(&os(args), none), vec![NpmSource::Mirror, NpmSource::Official], "{args:?}");
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

    #[test]
    fn app_path_appends_once_and_keeps_user_runtimes_first() {
        let user = std::env::join_paths([PathBuf::from("/usr/local/bin"), PathBuf::from("/usr/bin")]).unwrap();
        let extra = [PathBuf::from("/app/video-runtime/node/bin"), PathBuf::from("/home/u/.kivio/npm-global/bin")];
        let joined = appended_path(&user, &extra).unwrap();
        let entries: Vec<PathBuf> = std::env::split_paths(&joined).collect();
        assert_eq!(entries[0], PathBuf::from("/usr/local/bin"));
        assert_eq!(&entries[2..], &extra[..]);
        assert!(appended_path(&joined, &extra).is_none(), "idempotent");
    }
}
