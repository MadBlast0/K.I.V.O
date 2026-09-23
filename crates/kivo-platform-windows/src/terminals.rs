//! Visible terminals for CLI agents (CONVERSATION §5.2): Windows Terminal (`wt`) when it is
//! installed, with the folder and a title the session is tracked by; otherwise a console window
//! through `start`. The window is the user's to watch and take over.

use kivo_platform::{PlatformError, PlatformResult, Terminals};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Default)]
pub struct WindowsTerminals;

/// Windows Terminal's command, if it is on this PC (its app alias in `WindowsApps`).
fn windows_terminal() -> Option<PathBuf> {
    crate::environment::current_path()
        .into_iter()
        .map(|dir| dir.join("wt.exe"))
        .find(|p| p.is_file())
}

/// `wt` reads `;` as "a new tab": escape it inside arguments.
fn wt_arg(a: &str) -> String {
    a.replace(';', "\\;")
}

/// The arguments for `wt`: a new window, its title, the folder, then the program.
pub fn wt_args(cwd: &Path, title: &str, program: &str, args: &[String]) -> Vec<String> {
    let mut out = vec![
        "-w".to_owned(),
        "new".to_owned(),
        "--title".to_owned(),
        wt_arg(title),
        "-d".to_owned(),
        cwd.display().to_string(),
        wt_arg(program),
    ];
    out.extend(args.iter().map(|a| wt_arg(a)));
    out
}

/// Quotes one argument for `cmd`.
fn cmd_quote(s: &str) -> String {
    if s.is_empty() || s.contains([' ', '&', '(', ')', '^', '|', '<', '>', '"']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

/// The raw command line after `cmd /d /c` for the console fallback.
pub fn console_line(cwd: &Path, title: &str, program: &str, args: &[String]) -> String {
    let mut line = format!(
        "start {} /D {} cmd /k {}",
        cmd_quote(title),
        cmd_quote(&cwd.display().to_string()),
        cmd_quote(program)
    );
    for a in args {
        line.push(' ');
        line.push_str(&cmd_quote(a));
    }
    line
}

impl Terminals for WindowsTerminals {
    fn open(&self, cwd: &Path, title: &str, program: &str, args: &[String]) -> PlatformResult<()> {
        if !cwd.is_dir() {
            return Err(PlatformError::NotFound(cwd.display().to_string()));
        }
        let spawned = match windows_terminal() {
            Some(wt) => std::process::Command::new(wt)
                .args(wt_args(cwd, title, program, args))
                .spawn(),
            None => std::process::Command::new("cmd")
                .args(["/d", "/c"])
                .raw_arg(console_line(cwd, title, program, args))
                .creation_flags(CREATE_NO_WINDOW)
                .spawn(),
        };
        spawned.map(drop).map_err(|e| PlatformError::Os {
            code: i64::from(e.raw_os_error().unwrap_or(0)),
            message: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines_keep_the_folder_title_and_arguments_whole() {
        let cwd = Path::new(r"C:\work\K.I.V.O");
        let args = vec!["--permission-mode".to_owned(), "plan".to_owned()];
        assert_eq!(
            wt_args(cwd, "Claude · K.I.V.O", "claude", &args),
            [
                "-w",
                "new",
                "--title",
                "Claude · K.I.V.O",
                "-d",
                r"C:\work\K.I.V.O",
                "claude",
                "--permission-mode",
                "plan"
            ]
        );
        assert_eq!(
            wt_args(cwd, "a;b", "x", &["echo;calc".into()])[3],
            "a\\;b",
            "a ; can't start another tab"
        );
        assert_eq!(
            console_line(
                Path::new(r"C:\my work"),
                "Claude",
                "claude",
                &["--resume".into(), "a b".into()]
            ),
            r#"start Claude /D "C:\my work" cmd /k claude --resume "a b""#
        );
        assert_eq!(cmd_quote("x&calc"), "\"x&calc\"");
    }
}
