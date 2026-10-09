//! Shared npm configuration and retry policy for the App CLI and standalone PATH launchers.
//! std-only so the build can compile the standalone launchers without Cargo dependencies.
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

pub const NPM_MIRROR_REGISTRY: &str = "https://registry.npmmirror.com/";
pub const NPM_OFFICIAL_REGISTRY: &str = "https://registry.npmjs.org/";
pub const PLAYWRIGHT_MIRROR_HOST: &str = "https://cdn.npmmirror.com/binaries/playwright";
const FETCH_COMMANDS: &[&str] = &[
    "install",
    "i",
    "add",
    "ci",
    "clean-install",
    "update",
    "up",
    "upgrade",
    "view",
    "info",
    "show",
    "outdated",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Mirror,
    Official,
    AsConfigured,
}
impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mirror => "npmmirror 镜像",
            Self::Official => "官方源",
            Self::AsConfigured => "已配置的源",
        }
    }
}

#[derive(Default)]
pub struct Configuration {
    pub registry: bool,
    pub prefix: bool,
}

// Only npm's options count; options after `--` belong to the executed program.
pub fn has_option(args: &[OsString], name: &str) -> bool {
    args.iter()
        .take_while(|arg| *arg != OsStr::new("--"))
        .filter_map(|arg| arg.to_str())
        .any(|arg| arg == name || arg.starts_with(&format!("{name}=")))
}
fn option(args: &[OsString], name: &str) -> Option<OsString> {
    let mut args = args.iter().take_while(|arg| *arg != OsStr::new("--"));
    let mut found = None;
    while let Some(arg) = args.next() {
        if arg == name {
            found = args.next().cloned();
        } else if let Some(value) = arg
            .to_str()
            .and_then(|arg| arg.strip_prefix(&format!("{name}=")))
        {
            found = Some(value.into());
        }
    }
    found
}
pub fn env_value(env: &dyn Fn(&str) -> Option<OsString>, key: &str) -> Option<OsString> {
    env(key)
        .or_else(|| env(&key.to_ascii_uppercase()))
        .filter(|value| !value.is_empty())
}
fn npmrc_sets(text: &str, key: &str) -> bool {
    text.lines().any(|line| {
        line.trim_start()
            .split_once('=')
            .is_some_and(|(name, value)| name.trim() == key && !value.trim().is_empty())
    })
}
fn read_config(config: &mut Configuration, path: Option<PathBuf>) {
    if let Some(text) = path.and_then(|path| std::fs::read_to_string(path).ok()) {
        config.registry |= npmrc_sets(&text, "registry");
        config.prefix |= npmrc_sets(&text, "prefix");
    }
}

/// Read only the relevant keys; npm itself still resolves values and precedence.
/// A project npmrc belongs to npm's local prefix (nearest package.json/node_modules),
/// and is ignored for global commands, matching npm's normal configuration scope.
pub fn configuration(
    args: &[OsString],
    env: &dyn Fn(&str) -> Option<OsString>,
    cwd: Option<&Path>,
    home: Option<&Path>,
) -> Configuration {
    let mut config = Configuration {
        registry: has_option(args, "--registry") || env_value(env, "npm_config_registry").is_some(),
        prefix: has_option(args, "--prefix") || env_value(env, "npm_config_prefix").is_some(),
    };
    let user = option(args, "--userconfig")
        .or_else(|| env_value(env, "npm_config_userconfig"))
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".npmrc")));
    read_config(&mut config, user);
    read_config(
        &mut config,
        option(args, "--globalconfig")
            .or_else(|| env_value(env, "npm_config_globalconfig"))
            .map(PathBuf::from),
    );
    let global = has_option(args, "-g")
        || has_option(args, "--global")
        || env_value(env, "npm_config_global").is_some_and(|value| value == "true" || value == "1");
    if !global {
        let explicit = option(args, "--prefix")
            .or_else(|| env_value(env, "npm_config_prefix"))
            .map(PathBuf::from);
        let project = explicit.as_deref().or(cwd).map(|dir| {
            dir.ancestors()
                .find(|dir| dir.join("package.json").is_file() || dir.join("node_modules").is_dir())
                .unwrap_or(dir)
                .join(".npmrc")
        });
        read_config(&mut config, project);
    }
    config
}

pub fn sources(npx: bool, args: &[OsString], config: &Configuration) -> Vec<Source> {
    if config.registry {
        return vec![Source::AsConfigured];
    }
    // Only classify an unambiguous command. Option values may themselves be named
    // `view` or `install`; treating those as commands could rerun a later `exec`.
    let command = args.iter().filter_map(|arg| arg.to_str()).find_map(|arg| {
        if arg.starts_with('-') {
            None
        } else {
            Some(arg)
        }
    });
    let command = match args.first().and_then(|arg| arg.to_str()) {
        Some(arg) if !arg.starts_with('-') => command,
        _ => None,
    };
    // exec/x and npx can perform arbitrary work. A nonzero exit is not proof of a
    // download failure, so never rerun the user's command against another registry.
    if npx || matches!(command, Some("exec" | "x")) {
        return vec![Source::Mirror];
    }
    if command.is_some_and(|command| FETCH_COMMANDS.contains(&command)) {
        vec![Source::Mirror, Source::Official]
    } else {
        vec![Source::AsConfigured]
    }
}
