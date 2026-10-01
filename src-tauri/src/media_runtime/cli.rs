//! `dsivio tools [--json]`: discover bundled programs without a running App.
use super::runtime;
use std::{collections::BTreeMap, ffi::OsString, io::Write, path::PathBuf, process::ExitCode};

pub const SUBCOMMAND: &str = "tools";

fn print_tools(
    tools: &BTreeMap<&str, PathBuf>,
    json: bool,
    output: &mut impl Write,
) -> Result<(), String> {
    if json {
        serde_json::to_writer(&mut *output, tools).map_err(|error| error.to_string())?;
        writeln!(output).map_err(|error| error.to_string())?;
    } else {
        for (name, path) in tools {
            writeln!(output, "{name}\t{}", path.display()).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// Runs before App initialization; it never contacts the media generation server.
pub fn run(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    let json = match args.next() {
        None => false,
        Some(arg) if arg == "--json" && args.next().is_none() => true,
        _ => {
            eprintln!("Usage: dsivio tools [--json]");
            return ExitCode::from(2);
        }
    };
    let result = runtime::tools_resource_directory()
        .and_then(|resources| runtime::tools_at(&resources.join("video-runtime")))
        .and_then(|tools| print_tools(&tools, json, &mut std::io::stdout().lock()));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dsivio tools: {error}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_tools_print_absolute_paths_and_omit_missing_programs() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("relocated app with spaces/video-runtime");
        let ffmpeg = root.join(if cfg!(windows) {
            "analyzer/node_modules/ffmpeg-static/ffmpeg.exe"
        } else {
            "analyzer/node_modules/ffmpeg-static/ffmpeg"
        });
        let node = root.join(if cfg!(windows) { "node/node.exe" } else { "node/bin/node" });
        for program in [&ffmpeg, &node] {
            std::fs::create_dir_all(program.parent().unwrap()).unwrap();
            std::fs::write(program, "fixture").unwrap();
        }
        // A directory named like a program is not a discovered executable file.
        std::fs::create_dir_all(root.join(if cfg!(windows) { "bin/ffprobe.exe" } else { "bin/ffprobe" })).unwrap();
        let tools = runtime::tools_at(&root).unwrap();
        let expected = serde_json::json!({"ffmpeg": ffmpeg, "node": node});
        let mut json = Vec::new();
        print_tools(&tools, true, &mut json).unwrap();
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&json).unwrap(), expected);
        assert_eq!(json.last(), Some(&b'\n'));
        assert!(tools.values().all(|path| path.is_absolute() && path.is_file()));

        let mut human = Vec::new();
        print_tools(&tools, false, &mut human).unwrap();
        assert_eq!(
            String::from_utf8(human).unwrap(),
            format!("ffmpeg\t{}\nnode\t{}\n", ffmpeg.display(), node.display())
        );
    }

    #[test]
    fn missing_runtime_prints_empty_json_and_no_human_rows() {
        let temp = tempfile::tempdir().unwrap();
        let tools = runtime::tools_at(&temp.path().join("absent")).unwrap();
        let mut json = Vec::new();
        print_tools(&tools, true, &mut json).unwrap();
        assert_eq!(json, b"{}\n");
        let mut human = Vec::new();
        print_tools(&tools, false, &mut human).unwrap();
        assert!(human.is_empty());
    }
}
