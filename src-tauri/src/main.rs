#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let _program = args.next();
    if let Some(first) = args.next() {
        if first == kivio::rapidocr::RAPIDOCR_WORKER_ARG {
            return kivio::rapidocr::run_worker_entry(args);
        }
        if first == kivio::media_generation::cli::SUBCOMMAND {
            attach_parent_console();
            return kivio::media_generation::cli::run(args);
        }
    }

    kivio::run();
    ExitCode::SUCCESS
}

/// Release builds use the GUI subsystem. Piped stdio from a parent process is inherited as is;
/// an interactive terminal needs its console attached so `dsivio media` output is visible.
fn attach_parent_console() {
    #[cfg(all(windows, not(debug_assertions)))]
    unsafe {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}
