//! Command-line flags (ARCHITECTURE §1).

/// How the runtime was started, which decides whether and how it launches the app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Args {
    /// Started at sign-in: launch the app in the background (no window).
    pub autostart: bool,
    /// Started by the app, which is already running: supervise it, don't launch it.
    pub from_app: bool,
    /// Never launch or supervise the app (development and tests).
    pub no_app: bool,
}

impl Args {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut parsed = Self::default();
        for arg in args {
            match arg.as_str() {
                "--autostart" => parsed.autostart = true,
                "--from-app" => parsed.from_app = true,
                "--no-app" => parsed.no_app = true,
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, String> {
        Args::parse(args.iter().map(ToString::to_string))
    }

    #[test]
    fn flags_are_parsed_and_unknown_ones_refused() {
        assert_eq!(parse(&[]), Ok(Args::default()));
        let all = parse(&["--autostart", "--from-app", "--no-app"]).unwrap();
        assert!(all.autostart && all.from_app && all.no_app);
        assert_eq!(parse(&["--nope"]), Err("unknown argument: --nope".into()));
    }
}
