//! A small command-line parser that behaves like Commander.js 12, which the
//! original `mdg` CLI used. Agents and scripts may depend on its exact help
//! text, error messages and argument handling, so this reproduces:
//!
//! - option parsing anywhere on the command line (`--opt value`,
//!   `--opt=value`, `-abc` short-flag groups, `--` terminator, variadic
//!   options that consume following operands);
//! - subcommand dispatch, the implicit `help [command]` command and
//!   `-h, --help` / `-V, --version`;
//! - help layout and word wrapping (80 columns unless writing to a terminal);
//! - error messages, "Did you mean …?" suggestions and the order in which
//!   problems are detected.

use std::collections::HashMap;

use mdg_core::js;

/// What happens to an option's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptKind {
    /// `--flag`: sets `true`.
    Bool,
    /// `--no-flag`: defaults to `true`, sets `false`.
    Negate,
    /// `--opt <value>`.
    Required,
}

/// A parsed option value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Str(String),
    Int(u64),
    List(Vec<String>),
}

pub type ParseFn = fn(&str) -> Result<Value, String>;

#[derive(Debug, Clone)]
pub struct Opt {
    pub flags: &'static str,
    pub description: &'static str,
    short: Option<String>,
    long: Option<String>,
    kind: OptKind,
    variadic: bool,
    mandatory: bool,
    default: Option<Value>,
    /// `JSON.stringify` of the default, shown in help.
    default_text: Option<String>,
    choices: Option<Vec<&'static str>>,
    parser: Option<ParseFn>,
}

impl Opt {
    pub fn new(flags: &'static str, description: &'static str) -> Self {
        // Same loose splitting as Commander: "-V, --version", "--depth <n>".
        let parts: Vec<&str> = flags.split([' ', '|', ',']).filter(|s| !s.is_empty()).collect();
        let mut rest = parts.as_slice();
        let mut short = None;
        if rest.len() > 1 && !rest[1].starts_with(['[', '<']) {
            short = Some(rest[0].to_string());
            rest = &rest[1..];
        }
        let mut long = rest.first().map(|s| s.to_string());
        if short.is_none()
            && long.as_deref().is_some_and(|l| l.len() == 2 && l.starts_with('-') && !l.starts_with("--"))
        {
            short = long.take();
        }
        let required = flags.contains('<');
        let variadic = flags.contains("...>");
        let negate = long.as_deref().is_some_and(|l| l.starts_with("--no-"));
        let kind = if required {
            OptKind::Required
        } else if negate {
            OptKind::Negate
        } else {
            OptKind::Bool
        };
        Opt {
            flags,
            description,
            short,
            long,
            kind,
            variadic,
            mandatory: false,
            default: None,
            default_text: None,
            choices: None,
            parser: None,
        }
    }

    pub fn mandatory(mut self) -> Self {
        self.mandatory = true;
        self
    }

    pub fn parser(mut self, parser: ParseFn) -> Self {
        self.parser = Some(parser);
        self
    }

    pub fn default_int(mut self, value: u64) -> Self {
        self.default = Some(Value::Int(value));
        self.default_text = Some(value.to_string());
        self
    }

    pub fn default_str(mut self, value: &str) -> Self {
        self.default = Some(Value::Str(value.to_string()));
        self.default_text = Some(js::quote(value));
        self
    }

    pub fn choices(mut self, choices: &[&'static str]) -> Self {
        self.choices = Some(choices.to_vec());
        self
    }

    /// The option name without dashes, e.g. `dry-run` or `no-reuse`.
    fn name(&self) -> String {
        match (&self.long, &self.short) {
            (Some(long), _) => long.trim_start_matches("--").to_string(),
            (None, Some(short)) => short.trim_start_matches('-').to_string(),
            (None, None) => String::new(),
        }
    }

    /// Key in the parsed values: camel case, without a `no-` prefix.
    pub fn attribute(&self) -> String {
        let name = self.name();
        let name = name.strip_prefix("no-").unwrap_or(&name);
        let mut out = String::new();
        let mut upper = false;
        for c in name.chars() {
            if c == '-' {
                upper = true;
            } else if upper {
                out.extend(c.to_uppercase());
                upper = false;
            } else {
                out.push(c);
            }
        }
        out
    }

    fn is(&self, arg: &str) -> bool {
        self.short.as_deref() == Some(arg) || self.long.as_deref() == Some(arg)
    }

    fn help_description(&self) -> String {
        let mut extra = Vec::new();
        if let Some(choices) = &self.choices {
            let quoted: Vec<String> = choices.iter().map(|c| js::quote(c)).collect();
            extra.push(format!("choices: {}", quoted.join(", ")));
        }
        if let (Some(text), OptKind::Required) = (&self.default_text, self.kind) {
            extra.push(format!("default: {text}"));
        }
        if extra.is_empty() {
            self.description.to_string()
        } else {
            format!("{} ({})", self.description, extra.join(", "))
        }
    }
}

#[derive(Debug, Clone)]
pub struct Arg {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
}

impl Arg {
    fn human(&self) -> String {
        if self.required {
            format!("<{}>", self.name)
        } else {
            format!("[{}]", self.name)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Command<A: Copy> {
    pub name: &'static str,
    pub description: String,
    pub args: Vec<Arg>,
    pub options: Vec<Opt>,
    pub commands: Vec<Command<A>>,
    pub help_after: Option<String>,
    pub action: Option<A>,
    pub version: Option<&'static str>,
}

impl<A: Copy> Command<A> {
    pub fn new(name: &'static str, description: impl Into<String>) -> Self {
        Command {
            name,
            description: description.into(),
            args: vec![],
            options: vec![],
            commands: vec![],
            help_after: None,
            action: None,
            version: None,
        }
    }

    pub fn version(mut self, version: &'static str) -> Self {
        self.version = Some(version);
        self.options.push(Opt::new("-V, --version", "output the version number"));
        self
    }

    pub fn argument(mut self, spec: &'static str, description: &'static str) -> Self {
        let required = spec.starts_with('<');
        self.args.push(Arg { name: spec.trim_matches(['<', '>', '[', ']']), description, required });
        self
    }

    pub fn option(mut self, option: Opt) -> Self {
        self.options.push(option);
        self
    }

    pub fn command(mut self, command: Command<A>) -> Self {
        self.commands.push(command);
        self
    }

    pub fn help_after(mut self, text: impl Into<String>) -> Self {
        self.help_after = Some(text.into());
        self
    }

    pub fn action(mut self, action: A) -> Self {
        self.action = Some(action);
        self
    }

    fn find_command(&self, name: &str) -> Option<&Command<A>> {
        self.commands.iter().find(|c| c.name == name)
    }

    fn find_option(&self, arg: &str) -> Option<&Opt> {
        self.options.iter().find(|o| o.is(arg))
    }

    fn has_help_command(&self) -> bool {
        !self.commands.is_empty() && self.action.is_none() && self.find_command("help").is_none()
    }
}

/// Where help and errors go, and how wide the terminal is.
pub trait Output {
    fn out(&mut self, text: &str);
    fn err(&mut self, text: &str);
    fn out_width(&self) -> Option<usize> {
        None
    }
    fn err_width(&self) -> Option<usize> {
        None
    }
}

/// The result of a successful parse.
#[derive(Debug, Clone)]
pub struct Parsed<A> {
    pub action: A,
    pub args: Vec<String>,
    pub values: HashMap<String, Value>,
}

impl<A> Parsed<A> {
    pub fn flag(&self, name: &str) -> bool {
        matches!(self.values.get(name), Some(Value::Bool(true)))
    }

    pub fn string(&self, name: &str) -> Option<String> {
        match self.values.get(name) {
            Some(Value::Str(s)) => Some(s.clone()),
            _ => None,
        }
    }

    pub fn int(&self, name: &str) -> Option<u64> {
        match self.values.get(name) {
            Some(Value::Int(n)) => Some(*n),
            _ => None,
        }
    }

    pub fn list(&self, name: &str) -> Option<Vec<String>> {
        match self.values.get(name) {
            Some(Value::List(l)) => Some(l.clone()),
            _ => None,
        }
    }
}

/// Parsing stopped: help or version was shown, or a usage error was
/// reported. `code` is Commander's exit code (0 or 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit {
    pub code: i32,
}

const HELP_FLAGS: &str = "-h, --help";
const HELP_DESCRIPTION: &str = "display help for command";
const SHOW_HELP_AFTER_ERROR: &str = "(run with --help for usage)";

fn maybe_option(arg: &str) -> bool {
    arg.len() > 1 && arg.starts_with('-')
}

struct Parser<'a, A: Copy> {
    out: &'a mut dyn Output,
    /// The command and its ancestors, root first.
    chain: Vec<&'a Command<A>>,
}

struct OptionsParse {
    operands: Vec<String>,
    unknown: Vec<String>,
}

impl<'a, A: Copy> Parser<'a, A> {
    fn cmd(&self) -> &'a Command<A> {
        self.chain.last().expect("chain is never empty")
    }

    fn error(&mut self, message: &str) -> Exit {
        self.out.err(&format!("{message}\n"));
        self.out.err(&format!("{SHOW_HELP_AFTER_ERROR}\n"));
        Exit { code: 1 }
    }

    fn output_help(&mut self, error: bool) {
        let width = if error { self.out.err_width() } else { self.out.out_width() };
        let text = format_help(&self.chain, width.unwrap_or(80));
        let after = self.cmd().help_after.clone();
        let write = |out: &mut dyn Output, t: &str| if error { out.err(t) } else { out.out(t) };
        write(self.out, &text);
        if let Some(after) = after.filter(|a| !a.is_empty()) {
            write(self.out, &format!("{after}\n"));
        }
    }

    fn help(&mut self, error: bool) -> Exit {
        self.output_help(error);
        Exit { code: i32::from(error) }
    }

    /// Splits arguments into operands and unknown options, applying known
    /// options as they are seen (like Commander's `parseOptions`).
    fn parse_options(&mut self, argv: Vec<String>, values: &mut HashMap<String, Value>) -> Result<OptionsParse, Exit> {
        let cmd = self.cmd();
        let mut operands: Vec<String> = Vec::new();
        let mut unknown: Vec<String> = Vec::new();
        let mut to_unknown = false;
        let mut args: std::collections::VecDeque<String> = argv.into();
        let mut active_variadic: Option<&Opt> = None;

        while let Some(arg) = args.pop_front() {
            if arg == "--" {
                let dest = if to_unknown { &mut unknown } else { &mut operands };
                if to_unknown {
                    dest.push(arg);
                }
                dest.extend(args.drain(..));
                break;
            }
            if let Some(option) = active_variadic {
                if !maybe_option(&arg) {
                    self.apply(option, Some(arg), values)?;
                    continue;
                }
            }
            active_variadic = None;

            if maybe_option(&arg) {
                if let Some(option) = cmd.find_option(&arg) {
                    if option.kind == OptKind::Required {
                        let Some(value) = args.pop_front() else {
                            return Err(self.error(&format!("error: option '{}' argument missing", option.flags)));
                        };
                        self.apply(option, Some(value), values)?;
                    } else {
                        self.apply(option, None, values)?;
                    }
                    active_variadic = option.variadic.then_some(option);
                    continue;
                }
            }

            // Short flag groups like -abc: apply the first if known.
            if arg.len() > 2 && arg.starts_with('-') && !arg.starts_with("--") {
                let first: String = arg.chars().take(2).collect();
                if let Some(option) = cmd.find_option(&first) {
                    let rest: String = arg.chars().skip(2).collect();
                    if option.kind == OptKind::Required {
                        self.apply(option, Some(rest), values)?;
                    } else {
                        self.apply(option, None, values)?;
                        args.push_front(format!("-{rest}"));
                    }
                    continue;
                }
            }

            // Long option with an inline value, like --depth=2.
            if arg.starts_with("--") {
                if let Some(eq) = arg.find('=').filter(|&i| i > 2) {
                    if let Some(option) = cmd.find_option(&arg[..eq]) {
                        if option.kind == OptKind::Required {
                            self.apply(option, Some(arg[eq + 1..].to_string()), values)?;
                            continue;
                        }
                    }
                }
            }

            // Unknown options make the rest unknown so subcommands can parse them.
            if maybe_option(&arg) {
                to_unknown = true;
            }
            if to_unknown {
                unknown.push(arg);
            } else {
                operands.push(arg);
            }
        }
        Ok(OptionsParse { operands, unknown })
    }

    /// Applies one option occurrence (Commander's `handleOptionValue`).
    fn apply(&mut self, option: &Opt, value: Option<String>, values: &mut HashMap<String, Value>) -> Result<(), Exit> {
        if self.cmd().version.is_some() && option.flags == "-V, --version" {
            let version = self.cmd().version.unwrap_or_default();
            self.out.out(&format!("{version}\n"));
            return Err(Exit { code: 0 });
        }
        let key = option.attribute();
        let parsed = match (option.kind, value) {
            (OptKind::Bool, _) => Value::Bool(true),
            (OptKind::Negate, _) => Value::Bool(false),
            (OptKind::Required, Some(v)) => {
                if let Some(choices) = &option.choices {
                    if !choices.contains(&v.as_str()) {
                        return Err(self.error(&format!(
                            "error: option '{}' argument '{v}' is invalid. Allowed choices are {}.",
                            option.flags,
                            choices.join(", ")
                        )));
                    }
                    Value::Str(v)
                } else if let Some(parser) = option.parser {
                    match parser(&v) {
                        Ok(parsed) => parsed,
                        Err(message) => {
                            return Err(self.error(&format!(
                                "error: option '{}' argument '{v}' is invalid. {message}",
                                option.flags
                            )))
                        }
                    }
                } else if option.variadic {
                    match values.get(&key) {
                        Some(Value::List(previous)) if option.default.is_none() => {
                            let mut list = previous.clone();
                            list.push(v);
                            Value::List(list)
                        }
                        _ => Value::List(vec![v]),
                    }
                } else {
                    Value::Str(v)
                }
            }
            (OptKind::Required, None) => Value::Str(String::new()),
        };
        values.insert(key, parsed);
        Ok(())
    }

    fn unknown_option(&mut self, flag: &str) -> Exit {
        let mut suggestion = String::new();
        if flag.starts_with("--") {
            let mut candidates: Vec<String> = Vec::new();
            for command in self.chain.iter().rev() {
                candidates.extend(command.options.iter().filter_map(|o| o.long.clone()));
                candidates.push("--help".to_string());
            }
            suggestion = suggest_similar(flag, &candidates);
        }
        self.error(&format!("error: unknown option '{flag}'{suggestion}"))
    }

    fn parse_command(&mut self, mut operands: Vec<String>, unknown: Vec<String>) -> Result<Parsed<A>, Exit> {
        let cmd = self.cmd();
        let mut values: HashMap<String, Value> = HashMap::new();
        for option in &cmd.options {
            if option.kind == OptKind::Negate {
                values.insert(option.attribute(), Value::Bool(true));
            } else if let Some(default) = &option.default {
                values.insert(option.attribute(), default.clone());
            }
        }
        let parsed = self.parse_options(unknown, &mut values)?;
        operands.extend(parsed.operands);
        let unknown = parsed.unknown;
        let all_args: Vec<String> = operands.iter().chain(unknown.iter()).cloned().collect();

        if let Some(sub) = operands.first().and_then(|name| cmd.find_command(name)) {
            self.chain.push(sub);
            return self.parse_command(operands[1..].to_vec(), unknown);
        }
        if cmd.has_help_command() && operands.first().map(String::as_str) == Some("help") {
            return Err(self.dispatch_help(operands.get(1).cloned()));
        }
        if !cmd.commands.is_empty() && all_args.is_empty() && cmd.action.is_none() {
            return Err(self.help(true));
        }
        if unknown.iter().any(|a| a == "-h" || a == "--help") {
            self.output_help(false);
            return Err(Exit { code: 0 });
        }
        for command in self.chain.clone().into_iter().rev() {
            for option in &command.options {
                if option.mandatory && !values.contains_key(&option.attribute()) {
                    return Err(self.error(&format!("error: required option '{}' not specified", option.flags)));
                }
            }
        }

        let Some(action) = cmd.action else {
            if let Some(name) = operands.first() {
                let mut names: Vec<String> = cmd.commands.iter().map(|c| c.name.to_string()).collect();
                if cmd.has_help_command() {
                    names.push("help".to_string());
                }
                let suggestion = suggest_similar(name, &names);
                return Err(self.error(&format!("error: unknown command '{name}'{suggestion}")));
            }
            if let Some(flag) = unknown.first() {
                return Err(self.unknown_option(flag));
            }
            return Err(self.help(true));
        };

        if let Some(flag) = unknown.first() {
            return Err(self.unknown_option(flag));
        }
        for (i, arg) in cmd.args.iter().enumerate() {
            if arg.required && i >= all_args.len() {
                return Err(self.error(&format!("error: missing required argument '{}'", arg.name)));
            }
        }
        // Commander 12 allows excess arguments by default; they are ignored.
        Ok(Parsed { action, args: all_args, values })
    }

    /// `help [command]`: shows that command's help, or this command's.
    fn dispatch_help(&mut self, name: Option<String>) -> Exit {
        let Some(name) = name else { return self.help(false) };
        match self.cmd().find_command(&name) {
            Some(sub) => {
                self.chain.push(sub);
                self.help(false)
            }
            None => self.help(true),
        }
    }
}

/// Parses `argv` (without the program name) against `program`.
pub fn parse<A: Copy>(program: &Command<A>, argv: &[String], out: &mut dyn Output) -> Result<Parsed<A>, Exit> {
    let mut parser = Parser { out, chain: vec![program] };
    parser.parse_command(vec![], argv.to_vec())
}

// ---------------------------------------------------------------- help

fn command_usage<A: Copy>(chain: &[&Command<A>]) -> String {
    let cmd = chain.last().expect("chain is never empty");
    let mut parts = vec!["[options]".to_string()];
    if !cmd.commands.is_empty() {
        parts.push("[command]".to_string());
    }
    parts.extend(cmd.args.iter().map(Arg::human));
    let names: Vec<&str> = chain.iter().map(|c| c.name).collect();
    format!("{} {}", names.join(" "), parts.join(" "))
}

fn subcommand_term<A: Copy>(cmd: &Command<A>) -> String {
    let mut term = cmd.name.to_string();
    if !cmd.options.is_empty() {
        term.push_str(" [options]");
    }
    let args: Vec<String> = cmd.args.iter().map(Arg::human).collect();
    if !args.is_empty() {
        term.push(' ');
        term.push_str(&args.join(" "));
    }
    term
}

fn format_help<A: Copy>(chain: &[&Command<A>], help_width: usize) -> String {
    let cmd = chain.last().expect("chain is never empty");
    let mut options: Vec<(String, String)> =
        cmd.options.iter().map(|o| (o.flags.to_string(), o.help_description())).collect();
    options.push((HELP_FLAGS.to_string(), HELP_DESCRIPTION.to_string()));
    let mut commands: Vec<(String, String)> =
        cmd.commands.iter().map(|c| (subcommand_term(c), c.description.clone())).collect();
    if cmd.has_help_command() {
        commands.push(("help [command]".to_string(), HELP_DESCRIPTION.to_string()));
    }
    let arguments: Vec<(String, String)> = if cmd.args.iter().any(|a| !a.description.is_empty()) {
        cmd.args.iter().map(|a| (a.name.to_string(), a.description.to_string())).collect()
    } else {
        vec![]
    };

    let term_width =
        options.iter().chain(commands.iter()).chain(arguments.iter()).map(|(t, _)| js::utf16_len(t)).max().unwrap_or(0);
    let item_indent = 2;
    let separator = 2;
    let format_item = |term: &str, description: &str| -> String {
        if description.is_empty() {
            return term.to_string();
        }
        let padding = (term_width + separator).saturating_sub(js::utf16_len(term));
        let full = format!("{term}{}{description}", " ".repeat(padding));
        wrap(&full, help_width.saturating_sub(item_indent), term_width + separator)
    };
    let format_list = |items: &[(String, String)]| -> String {
        let text: Vec<String> = items.iter().map(|(t, d)| format_item(t, d)).collect();
        text.join("\n")
            .split('\n')
            .map(|line| format!("{}{line}", " ".repeat(item_indent)))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let mut output = vec![format!("Usage: {}", command_usage(chain)), String::new()];
    if !cmd.description.is_empty() {
        output.push(wrap(&cmd.description, help_width, 0));
        output.push(String::new());
    }
    if !arguments.is_empty() {
        output.extend(["Arguments:".to_string(), format_list(&arguments), String::new()]);
    }
    output.extend(["Options:".to_string(), format_list(&options), String::new()]);
    if !commands.is_empty() {
        output.extend(["Commands:".to_string(), format_list(&commands), String::new()]);
    }
    output.join("\n")
}

fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `\s` plus the zero-width space, as in Commander's wrap pattern.
fn is_break(c: char) -> bool {
    js::is_js_whitespace(c) || c == '\u{200b}'
}

/// Commander's `Help.wrap`: wraps `text` to `width` columns, indenting
/// continuation lines by `indent`. Text with manual indentation, or too
/// narrow a column, is returned unchanged.
pub fn wrap(text: &str, width: usize, indent: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let manual_indent = chars.windows(2).any(|w| {
        w[0] == '\n'
            && (matches!(
                w[1],
                ' ' | '\u{c}'
                    | '\t'
                    | '\u{b}'
                    | '\u{a0}'
                    | '\u{1680}'
                    | '\u{202f}'
                    | '\u{205f}'
                    | '\u{3000}'
                    | '\u{feff}'
            ) || ('\u{2000}'..='\u{200a}').contains(&w[1]))
    });
    if manual_indent {
        return text.to_string();
    }
    let column = width as isize - indent as isize;
    if column < 40 {
        return text.to_string();
    }
    let column = column as usize;
    let leading: String = chars.iter().take(indent).collect();
    let column_text: String = chars.iter().skip(indent).collect::<String>().replacen("\r\n", "\n", 1);
    let text: Vec<char> = column_text.chars().collect();

    // Emulates the global match of
    // /\n|.{1,column-1}([\s​]|$)|[^\s​]+?([\s​]|$)/g
    let mut lines: Vec<String> = Vec::new();
    let mut pos = 0;
    while pos < text.len() {
        if text[pos] == '\n' {
            lines.push("\n".to_string());
            pos += 1;
            continue;
        }
        let mut run = 0;
        while run < column - 1 && pos + run < text.len() && !is_line_terminator(text[pos + run]) {
            run += 1;
        }
        let mut end = None;
        for k in (1..=run).rev() {
            let next = pos + k;
            if next == text.len() {
                end = Some(next);
                break;
            }
            if is_break(text[next]) {
                end = Some(next + 1);
                break;
            }
        }
        if end.is_none() && !is_break(text[pos]) {
            let mut e = pos;
            while e < text.len() && !is_break(text[e]) {
                e += 1;
            }
            end = Some(if e < text.len() { e + 1 } else { e });
        }
        match end {
            Some(e) => {
                lines.push(text[pos..e].iter().collect());
                pos = e;
            }
            None => pos += 1,
        }
    }
    let indent_string = " ".repeat(indent);
    let body: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if line == "\n" {
                String::new()
            } else {
                format!("{}{}", if i > 0 { indent_string.as_str() } else { "" }, js::trim_end(line))
            }
        })
        .collect();
    format!("{leading}{}", body.join("\n"))
}

// ---------------------------------------------------------------- suggestions

const MAX_DISTANCE: usize = 3;

/// Damerau–Levenshtein distance (optimal string alignment).
fn edit_distance(a: &[char], b: &[char]) -> usize {
    if a.len().abs_diff(b.len()) > MAX_DISTANCE {
        return a.len().max(b.len());
    }
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for j in 1..=b.len() {
        for i in 1..=a.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// Commander's "(Did you mean …?)" suffix, or an empty string.
pub fn suggest_similar(word: &str, candidates: &[String]) -> String {
    let mut unique: Vec<&str> = Vec::new();
    for c in candidates {
        if !unique.contains(&c.as_str()) {
            unique.push(c);
        }
    }
    let searching_options = word.starts_with("--");
    let strip = |s: &str| if searching_options { s.chars().skip(2).collect::<String>() } else { s.to_string() };
    let word: Vec<char> = strip(word).chars().collect();
    let mut similar: Vec<String> = Vec::new();
    let mut best = MAX_DISTANCE;
    for candidate in unique.iter().map(|c| strip(c)) {
        let chars: Vec<char> = candidate.chars().collect();
        if chars.len() <= 1 {
            continue;
        }
        let distance = edit_distance(&word, &chars);
        let length = word.len().max(chars.len());
        let similarity = (length as f64 - distance as f64) / length as f64;
        if similarity > 0.4 {
            if distance < best {
                best = distance;
                similar = vec![candidate];
            } else if distance == best {
                similar.push(candidate);
            }
        }
    }
    similar.sort_by_key(|s| s.to_lowercase());
    if searching_options {
        similar = similar.into_iter().map(|c| format!("--{c}")).collect();
    }
    match similar.len() {
        0 => String::new(),
        1 => format!("\n(Did you mean {}?)", similar[0]),
        _ => format!("\n(Did you mean one of {}?)", similar.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_like_commander() {
        let text = "  --container <module-id>  module whose internal graph stores the connection (inferred by default)";
        assert_eq!(
            wrap(text, 78, 27),
            "  --container <module-id>  module whose internal graph stores the connection\n                           (inferred by default)"
        );
        assert_eq!(wrap("short", 80, 0), "short");
        assert_eq!(wrap("a\n  manual", 20, 0), "a\n  manual");
    }

    #[test]
    fn suggests_similar_names() {
        let names: Vec<String> = ["info", "tree", "describe", "help"].iter().map(|s| s.to_string()).collect();
        assert_eq!(suggest_similar("inf", &names), "\n(Did you mean info?)");
        assert_eq!(suggest_similar("zzzzzz", &names), "");
        let flags: Vec<String> = ["--json", "--help"].iter().map(|s| s.to_string()).collect();
        assert_eq!(suggest_similar("--jsn", &flags), "\n(Did you mean --json?)");
    }

    #[test]
    fn option_attributes() {
        assert_eq!(Opt::new("--dry-run", "").attribute(), "dryRun");
        assert_eq!(Opt::new("--no-reuse", "").attribute(), "reuse");
        assert_eq!(Opt::new("--via <module:port...>", "").attribute(), "via");
        assert_eq!(Opt::new("-V, --version", "").attribute(), "version");
    }
}
