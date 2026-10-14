//! The `mdg` command tree: commands, arguments, options and help texts.

use crate::commander::{Command, Opt};
use crate::shared::{examples, non_negative_int, positive_int};

pub const VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Info,
    Tree,
    Describe,
    Inputs,
    Outputs,
    Connections,
    Trace,
    Validate,
    Export,
    New,
    ModuleAdd,
    ModuleRemove,
    ModuleRename,
    ModuleMove,
    ModuleSetDescription,
    PortAdd,
    PortRemove,
    PortRename,
    PortSetDescription,
    Connect,
    Disconnect,
}

type Cmd = Command<Action>;

fn json(description: &'static str) -> Opt {
    Opt::new("--json", description)
}

fn with_edit_options(command: Cmd) -> Cmd {
    command
        .option(json("machine-readable result"))
        .option(Opt::new("--dry-run", "validate and report the change without writing the file"))
}

fn with_description_options(command: Cmd) -> Cmd {
    command
        .option(Opt::new("--description <markdown>", "Markdown description"))
        .option(Opt::new("--description-file <path>", "read the Markdown description from a file (`-` for stdin)"))
}

fn inspect_commands(program: Cmd) -> Cmd {
    let mut program = program
        .command(
            Cmd::new("info", "Show document summary: title, versions, root, counts and file size.")
                .argument("<file>", ".mdg file")
                .option(json("machine-readable output"))
                .help_after(examples(&["mdg info architecture.mdg", "mdg info architecture.mdg --json"]))
                .action(Action::Info),
        )
        .command(
            Cmd::new("tree", "Show the module hierarchy.")
                .argument("<file>", ".mdg file")
                .option(Opt::new("--ids", "show module IDs"))
                .option(json("nested JSON output"))
                .option(Opt::new("--depth <n>", "levels below the root to show").parser(non_negative_int))
                .help_after(examples(&[
                    "mdg tree architecture.mdg --ids",
                    "mdg tree architecture.mdg --depth 1 --json",
                ]))
                .action(Action::Tree),
        )
        .command(
            Cmd::new(
                "describe",
                "Describe a module as Markdown. Depth 0 shows the public contract only; depth 1 adds children \
                 and internal connections; depth N recurses N levels. Only the needed Markdown is read.",
            )
            .argument("<file>", ".mdg file")
            .argument("<module-id>", "module ID (or unique prefix)")
            .option(Opt::new("--depth <n>", "levels of internals to include").parser(non_negative_int).default_int(0))
            .option(json("structured JSON output"))
            .option(Opt::new("--output <file>", "write to a file instead of stdout"))
            .help_after(examples(&[
                "mdg describe architecture.mdg auth-001",
                "mdg describe architecture.mdg auth-001 --depth 1",
                "mdg describe architecture.mdg auth-001 --depth 2 --json --output auth.json",
            ]))
            .action(Action::Describe),
        );
    for (name, action) in [("inputs", Action::Inputs), ("outputs", Action::Outputs)] {
        program = program.command(
            Cmd::new(name, format!("List a module's {name} with their Markdown contracts."))
                .argument("<file>", ".mdg file")
                .argument("<module-id>", "module ID (or unique prefix)")
                .option(json("machine-readable output"))
                .help_after(examples(&[
                    &format!("mdg {name} architecture.mdg auth-001"),
                    &format!("mdg {name} architecture.mdg auth-001 --json"),
                ]))
                .action(action),
        );
    }
    program
        .command(
            Cmd::new("connections", "List the connections inside a module's internal graph.")
                .argument("<file>", ".mdg file")
                .argument("<module-id>", "module ID (or unique prefix)")
                .option(json("machine-readable output"))
                .help_after(examples(&[
                    "mdg connections architecture.mdg api-service",
                    "mdg connections architecture.mdg api-service --json",
                ]))
                .action(Action::Connections),
        )
        .command(
            Cmd::new(
                "trace",
                "Trace possible structural message routes from a port across parent boundaries. \
                 Routes do not imply runtime execution order.",
            )
            .argument("<file>", ".mdg file")
            .argument("<module-id>", "module ID (or unique prefix)")
            .argument("<port-id>", "port ID (or unique prefix)")
            .option(Opt::new("--reverse", "trace backwards (where can messages arriving here come from?)"))
            .option(Opt::new("--max-depth <n>", "maximum connections per route").parser(positive_int).default_int(64))
            .option(
                Opt::new("--max-routes <n>", "maximum number of routes to report")
                    .parser(positive_int)
                    .default_int(1000),
            )
            .option(json("machine-readable output"))
            .help_after(examples(&[
                "mdg trace architecture.mdg protected-endpoint token-validation-request",
                "mdg trace architecture.mdg protected-endpoint token-validation-result --reverse --json",
            ]))
            .action(Action::Trace),
        )
        .command(
            Cmd::new(
                "validate",
                "Check structural validity. Exits 1 when errors exist; warnings do not fail by default.",
            )
            .argument("<file>", ".mdg file")
            .option(json("machine-readable output"))
            .option(Opt::new("--strict", "also exit 1 when there are warnings"))
            .option(Opt::new("--no-warnings", "do not compute or show warnings"))
            .help_after(examples(&["mdg validate architecture.mdg", "mdg validate architecture.mdg --json --strict"]))
            .action(Action::Validate),
        )
        .command(
            Cmd::new("export", "Export documentation for the entire hierarchy.")
                .argument("<file>", ".mdg file")
                .option(Opt::new("--format <format>", "output format").choices(&["markdown"]).default_str("markdown"))
                .option(Opt::new("--output <file>", "write a single Markdown file instead of stdout"))
                .option(Opt::new(
                    "--split <dir>",
                    "write one Markdown file per module (plus index.md) into a directory",
                ))
                .help_after(examples(&[
                    "mdg export architecture.mdg --format markdown > ARCHITECTURE.md",
                    "mdg export architecture.mdg --format markdown --split docs/modules",
                ]))
                .action(Action::Export),
        )
}

fn module_commands() -> Cmd {
    Cmd::new("module", "Add, remove, rename, move or describe modules.")
        .command(
            with_edit_options(with_description_options(
                Cmd::new("add", "Add a module under a parent.")
                    .argument("<file>", ".mdg file")
                    .option(Opt::new("--parent <module-id>", "parent module ID").mandatory())
                    .option(Opt::new("--name <name>", "module name").mandatory())
                    .option(Opt::new("--id <id>", "explicit module ID (generated when omitted)"))
                    .option(Opt::new("--index <n>", "position among the parent's children").parser(non_negative_int)),
            ))
            .help_after(examples(&[
                "mdg module add architecture.mdg --parent api-service --name \"Rate Limiter\" --description \"Limits requests per client.\"",
                "mdg module add architecture.mdg --parent root-001 --name Billing --id billing --description-file billing.md --json",
            ]))
            .action(Action::ModuleAdd),
        )
        .command(
            with_edit_options(
                Cmd::new("remove", "Remove a module. Fails if it has children or connections unless --cascade is given.")
                    .argument("<file>", ".mdg file")
                    .argument("<module-id>", "module ID")
                    .option(Opt::new("--cascade", "also remove descendants and connections referencing the module")),
            )
            .help_after(examples(&[
                "mdg module remove architecture.mdg rate-limiter",
                "mdg module remove architecture.mdg billing --cascade --dry-run",
            ]))
            .action(Action::ModuleRemove),
        )
        .command(
            with_edit_options(
                Cmd::new("rename", "Rename a module. Its ID never changes.")
                    .argument("<file>", ".mdg file")
                    .argument("<module-id>", "module ID")
                    .argument("<name>", "new name"),
            )
            .help_after(examples(&["mdg module rename architecture.mdg auth-001 \"Identity Service\""]))
            .action(Action::ModuleRename),
        )
        .command(
            with_edit_options(
                Cmd::new(
                    "move",
                    "Move a module (and its subtree) under a new parent. IDs are preserved; moves that would invalidate connections are rejected.",
                )
                .argument("<file>", ".mdg file")
                .argument("<module-id>", "module ID")
                .option(Opt::new("--to <parent-id>", "new parent module ID").mandatory())
                .option(Opt::new("--index <n>", "position among the new parent's children").parser(non_negative_int)),
            )
            .help_after(examples(&["mdg module move architecture.mdg jwt-validator --to api-service --dry-run"]))
            .action(Action::ModuleMove),
        )
        .command(
            with_edit_options(with_description_options(
                Cmd::new("set-description", "Replace a module's Markdown description.")
                    .argument("<file>", ".mdg file")
                    .argument("<module-id>", "module ID"),
            ))
            .help_after(examples(&[
                "mdg module set-description architecture.mdg auth-001 --description \"Handles authentication requests.\"",
                "cat auth.md | mdg module set-description architecture.mdg auth-001 --description-file -",
            ]))
            .action(Action::ModuleSetDescription),
        )
}

fn port_commands() -> Cmd {
    Cmd::new("port", "Add, remove, rename or describe ports.")
        .command(
            with_edit_options(with_description_options(
                Cmd::new("add", "Add an input or output port to a module (not the root).")
                    .argument("<file>", ".mdg file")
                    .argument("<module-id>", "module ID")
                    .option(Opt::new("--direction <direction>", "port direction").choices(&["input", "output"]).mandatory())
                    .option(Opt::new("--name <name>", "port name").mandatory())
                    .option(Opt::new("--id <id>", "explicit port ID, unique within the module (generated when omitted)")),
            ))
            .help_after(examples(&[
                "mdg port add architecture.mdg auth-001 --direction input --name \"Login Request\" --id login-request --description \"Receives POST /login.\"",
                "mdg port add architecture.mdg auth-001 --direction output --name \"Token Issued\" --json",
            ]))
            .action(Action::PortAdd),
        )
        .command(
            with_edit_options(
                Cmd::new("remove", "Remove a port. Fails if it is connected unless --cascade is given.")
                    .argument("<file>", ".mdg file")
                    .argument("<module-id>", "module ID")
                    .argument("<port-id>", "port ID")
                    .option(Opt::new("--cascade", "also remove connections that use the port")),
            )
            .help_after(examples(&["mdg port remove architecture.mdg auth-001 login-request --cascade"]))
            .action(Action::PortRemove),
        )
        .command(
            with_edit_options(
                Cmd::new("rename", "Rename a port. Its ID never changes.")
                    .argument("<file>", ".mdg file")
                    .argument("<module-id>", "module ID")
                    .argument("<port-id>", "port ID")
                    .argument("<name>", "new name"),
            )
            .help_after(examples(&["mdg port rename architecture.mdg auth-001 login-request \"Sign-in Request\""]))
            .action(Action::PortRename),
        )
        .command(
            with_edit_options(with_description_options(
                Cmd::new("set-description", "Replace a port's Markdown contract.")
                    .argument("<file>", ".mdg file")
                    .argument("<module-id>", "module ID")
                    .argument("<port-id>", "port ID"),
            ))
            .help_after(examples(&[
                "mdg port set-description architecture.mdg auth-001 token-issued --description \"Emits a signed JWT.\"",
            ]))
            .action(Action::PortSetDescription),
        )
}

fn edit_commands(program: Cmd) -> Cmd {
    program
        .command(
            with_description_options(
                Cmd::new("new", "Create a new document containing only a root module.")
                    .argument("<file>", ".mdg file to create")
                    .option(Opt::new("--title <title>", "document title").mandatory())
                    .option(Opt::new("--root-name <name>", "root module name (defaults to the title)"))
                    .option(Opt::new("--root-id <id>", "explicit root module ID"))
                    .option(Opt::new("--force", "overwrite an existing file"))
                    .option(json("machine-readable result")),
            )
            .help_after(examples(&["mdg new architecture.mdg --title \"Payments Platform\""]))
            .action(Action::New),
        )
        .command(module_commands())
        .command(port_commands())
        .command(
            with_edit_options(
                Cmd::new(
                    "connect",
                    "Connect two ports (directed message flow). Endpoints are <module-id>:<port-id>. Without --route both \
                     ports must be on the same hierarchy level. With --route, boundary ports and local connections are \
                     created at every intermediate level.",
                )
                .argument("<file>", ".mdg file")
                .argument("<from>", "source endpoint <module-id>:<port-id>")
                .argument("<to>", "destination endpoint <module-id>:<port-id>")
                .option(Opt::new("--id <id>", "explicit connection ID (local connections only)"))
                .option(Opt::new(
                    "--container <module-id>",
                    "module whose internal graph stores the connection (inferred by default)",
                ))
                .option(Opt::new("--route", "route across hierarchy levels through generated boundary ports"))
                .option(Opt::new("--via <module:port...>", "with --route: use this existing boundary port at that module"))
                .option(Opt::new("--no-reuse", "with --route: never reuse existing boundary ports"))
                .option(Opt::new("--output-name <name>", "with --route: name for generated output ports"))
                .option(Opt::new("--input-name <name>", "with --route: name for generated input ports")),
            )
            .help_after(examples(&[
                "mdg connect architecture.mdg frontend:login-submitted api-service:login-request",
                "mdg connect architecture.mdg protected-endpoint:token-check jwt-validator:validate-token --route --dry-run --json",
            ]))
            .action(Action::Connect),
        )
        .command(
            with_edit_options(
                Cmd::new("disconnect", "Remove a connection by ID.")
                    .argument("<file>", ".mdg file")
                    .argument("<connection-id>", "connection ID (see `mdg connections`)"),
            )
            .help_after(examples(&["mdg disconnect architecture.mdg conn-1234 --dry-run"]))
            .action(Action::Disconnect),
        )
}

pub fn program() -> Cmd {
    let program = Cmd::new(
        "mdg",
        "MDG (Markdown Graph): inspect, edit and validate recursive message-flow architecture documents (.mdg).",
    )
    .version(VERSION)
    .help_after(
        [
            "",
            "Exit codes: 0 ok, 1 validation errors / rejected edit, 2 usage error,",
            "            3 file or format error, 4 module/port/connection not found.",
            "",
            "Examples:",
            "  $ mdg tree architecture.mdg --ids",
            "  $ mdg describe architecture.mdg api-service --depth 1",
            "  $ mdg connect architecture.mdg a:out b:in --route --dry-run",
            "  $ mdg validate architecture.mdg --json",
        ]
        .join("\n"),
    );
    edit_commands(inspect_commands(program))
}
