// Relocatable console entry points. Python-generated launchers embed the build
// machine's absolute interpreter path, so they cannot ship in an installer.
use std::{
    env,
    process::{exit, Command},
};

fn main() {
    let executable = env::current_exe().expect("locate bundled launcher");
    let root = executable.parent().unwrap().parent().unwrap();
    let name = executable.file_stem().unwrap().to_str().unwrap();
    let distribution = match name {
        "comfy" => "comfy-cli",
        "yt-dlp" => "yt-dlp",
        _ => panic!("unknown entry point"),
    };
    let python = root.join(if cfg!(windows) {
        "python/python.exe"
    } else {
        "python/bin/python3"
    });
    let mut command = Command::new(python);
    command.args(["-s", "-B", "-c", "import sys; from importlib.metadata import distribution; d,n=sys.argv[1:3]; sys.argv=sys.argv[2:]; sys.exit(next(e for e in distribution(d).entry_points if e.group=='console_scripts' and e.name==n).load()())", distribution, name]);
    command.args(env::args_os().skip(1));
    command.env("PYTHONPATH", root.join("python-packages"));
    command.env_remove("PYTHONHOME");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        eprintln!("Bundled Python failed: {error}");
        exit(1);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        exit(
            command
                .status()
                .expect("start bundled Python")
                .code()
                .unwrap_or(1),
        );
    }
}
