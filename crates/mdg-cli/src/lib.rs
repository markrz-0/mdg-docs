//! `mdg` — the command-line interface for MDG documents. The primary
//! interface for AI agents and scripts: it never prompts, supports `--json`
//! everywhere, and returns stable exit codes (see docs/cli.md).
//!
//! The CLI is a drop-in replacement for the original TypeScript `mdg`:
//! commands, help, error messages, JSON and exit codes are identical.

pub mod commander;
mod edit;
mod inspect;
pub mod program;
mod resolve;
pub mod shared;

use mdg_core::{js, ErrorCode, MdgError, PortDirection};
use serde_json::json;

use crate::program::Action;
pub use crate::shared::CliIo;
use crate::shared::{exit, Context};

fn exit_for(code: ErrorCode) -> i32 {
    match code {
        ErrorCode::NotFound => exit::NOT_FOUND,
        ErrorCode::InvalidArgument => exit::USAGE,
        ErrorCode::InvalidOperation | ErrorCode::ValidationFailed => exit::FAILURE,
        ErrorCode::FormatError | ErrorCode::UnsupportedVersion | ErrorCode::IoError => exit::FILE,
    }
}

fn report_error(error: &MdgError, io: &mut dyn CliIo, json: bool) -> i32 {
    if json {
        let mut body = json!({ "code": error.code.as_str(), "message": error.message });
        if let Some(issues) = &error.issues {
            body["issues"] = js::to_value(issues);
        }
        io.err(&format!("{}\n", js::stringify_pretty(&json!({ "error": body }))));
    } else {
        io.err(&format!("error [{}]: {}\n", error.code, error.message));
        for issue in error.issues.iter().flatten() {
            io.err(&format!("{}\n", inspect::format_issue(issue)));
        }
    }
    exit_for(error.code)
}

/// Runs the CLI with the given arguments (without the program name) and
/// returns the process exit code.
pub fn run_cli(argv: &[String], io: &mut dyn CliIo) -> i32 {
    let program = program::program();
    let parsed = match commander::parse(&program, argv, io) {
        Ok(parsed) => parsed,
        Err(stop) => return if stop.code == 0 { exit::OK } else { exit::USAGE },
    };
    let mut ctx = Context { io, exit_code: exit::OK };
    let result = match parsed.action {
        Action::Info => inspect::info(&mut ctx, &parsed),
        Action::Tree => inspect::tree(&mut ctx, &parsed),
        Action::Describe => inspect::describe(&mut ctx, &parsed),
        Action::Inputs => inspect::ports(&mut ctx, &parsed, PortDirection::Input),
        Action::Outputs => inspect::ports(&mut ctx, &parsed, PortDirection::Output),
        Action::Connections => inspect::connections(&mut ctx, &parsed),
        Action::Trace => inspect::trace_command(&mut ctx, &parsed),
        Action::Validate => inspect::validate(&mut ctx, &parsed),
        Action::Export => inspect::export(&mut ctx, &parsed),
        Action::New => edit::new(&mut ctx, &parsed),
        Action::ModuleAdd => edit::module_add(&mut ctx, &parsed),
        Action::ModuleRemove => edit::module_remove(&mut ctx, &parsed),
        Action::ModuleRename => edit::module_rename(&mut ctx, &parsed),
        Action::ModuleMove => edit::module_move(&mut ctx, &parsed),
        Action::ModuleSetDescription => edit::module_set_description(&mut ctx, &parsed),
        Action::PortAdd => edit::port_add(&mut ctx, &parsed),
        Action::PortRemove => edit::port_remove(&mut ctx, &parsed),
        Action::PortRename => edit::port_rename(&mut ctx, &parsed),
        Action::PortSetDescription => edit::port_set_description(&mut ctx, &parsed),
        Action::PortSetResponse => edit::port_set_response(&mut ctx, &parsed),
        Action::PortSetKind => edit::port_set_kind(&mut ctx, &parsed),
        Action::Connect => edit::connect(&mut ctx, &parsed),
        Action::Disconnect => edit::disconnect(&mut ctx, &parsed),
    };
    match result {
        Ok(()) => ctx.exit_code,
        Err(error) => report_error(&error, ctx.io, argv.iter().any(|a| a == "--json")),
    }
}
