use std::io::{IsTerminal, Read, Write};

use mdg_cli::commander::Output;
use mdg_cli::{run_cli, CliIo};

/// Process stdio. Help is wrapped to the terminal width when writing to a
/// terminal, and to 80 columns otherwise.
struct ProcessIo;

fn width(is_terminal: bool) -> Option<usize> {
    if !is_terminal {
        return None;
    }
    terminal_size::terminal_size().map(|(w, _)| usize::from(w.0))
}

impl Output for ProcessIo {
    fn out(&mut self, text: &str) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(text.as_bytes());
        let _ = out.flush();
    }

    fn err(&mut self, text: &str) {
        let _ = std::io::stderr().lock().write_all(text.as_bytes());
    }

    fn out_width(&self) -> Option<usize> {
        width(std::io::stdout().is_terminal())
    }

    fn err_width(&self) -> Option<usize> {
        width(std::io::stderr().is_terminal())
    }
}

impl CliIo for ProcessIo {
    fn read_stdin(&mut self) -> String {
        let mut bytes = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

fn main() {
    let argv: Vec<String> = std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
    // Hierarchies have no depth limit; give deep documents room to serialize.
    let code = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || run_cli(&argv, &mut ProcessIo))
        .expect("spawn main thread")
        .join()
        .unwrap_or(1);
    std::process::exit(code);
}
