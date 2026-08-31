use std::process::ExitCode;

use line::ssh::askpass_entrypoint;

mod runtime;

fn main() -> ExitCode {
    // OpenSSH invokes this same executable as its password helper. This check
    // must happen before config loading or terminal initialization.
    if let Some(code) = askpass_entrypoint() {
        return ExitCode::from(code as u8);
    }

    match runtime::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("line: {error}");
            ExitCode::FAILURE
        }
    }
}
