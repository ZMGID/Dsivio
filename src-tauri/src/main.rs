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
        if first == kivio::app_cli::AI_SUBCOMMAND {
            attach_parent_console();
            return kivio::app_cli::run_ai(args);
        }
        if first == kivio::workbench::commerce::SUBCOMMAND {
            attach_parent_console();
            return kivio::workbench::commerce::run(args);
        }
        if first == kivio::workbench::publish::SUBCOMMAND {
            attach_parent_console();
            return kivio::workbench::publish::run(args);
        }
        if first == kivio::media_runtime::dsvideo::SUBCOMMAND {
            attach_parent_console();
            return kivio::media_runtime::dsvideo::run(args);
        }
        if first == kivio::media_runtime::cli::SUBCOMMAND {
            attach_parent_console();
            return kivio::media_runtime::cli::run(args);
        }
        // `dsivio python|node|npm`: the bundled runtimes for Skills on machines without them.
        if let Some(tool) = kivio::media_runtime::launch::Tool::from_subcommand(&first) {
            attach_parent_console();
            return kivio::media_runtime::launch::run(tool, args);
        }
    }

    kivio::run();
    ExitCode::SUCCESS
}

/// Release builds use the GUI subsystem. Piped stdio from a parent process is inherited as is;
/// an interactive terminal needs its console attached so CLI output is visible.
fn attach_parent_console() {
    #[cfg(all(windows, not(debug_assertions)))]
    unsafe {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}
