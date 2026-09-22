//! Same location as dsvideo_config.py; independent of the bundled Python runtime.
use std::path::{Path, PathBuf};

pub(crate) fn path() -> Result<PathBuf, String> {
    let home = directories::BaseDirs::new().ok_or("无法定位用户目录")?;
    Ok(resolve(
        home.home_dir(),
        std::env::var_os("DSVIDEO_CONFIG_PATH").map(PathBuf::from),
        std::env::var_os("APPDATA").map(PathBuf::from),
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
    ))
}

fn resolve(
    home: &Path,
    override_path: Option<PathBuf>,
    appdata: Option<PathBuf>,
    xdg: Option<PathBuf>,
) -> PathBuf {
    let nonempty = |p: &PathBuf| !p.as_os_str().is_empty();
    let expand = |p: PathBuf| match p.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => p,
    };
    if let Some(path) = override_path.filter(nonempty) {
        return expand(path);
    }
    if let Some(path) = appdata.filter(nonempty) {
        return path.join("dsvideo/providers.json");
    }
    xdg.filter(nonempty)
        .map(expand)
        .unwrap_or_else(|| home.join(".config"))
        .join("dsvideo/providers.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_config_locations_match_python_contract() {
        let home = Path::new("/users/employee");
        assert_eq!(
            resolve(home, None, None, None),
            home.join(".config/dsvideo/providers.json")
        );
        assert_eq!(
            resolve(home, None, Some("roaming".into()), None),
            PathBuf::from("roaming/dsvideo/providers.json")
        );
        assert_eq!(
            resolve(home, None, None, Some("~/config".into())),
            home.join("config/dsvideo/providers.json")
        );
        assert_eq!(
            resolve(
                home,
                Some("~/company.json".into()),
                Some("roaming".into()),
                None
            ),
            home.join("company.json")
        );
    }
}
