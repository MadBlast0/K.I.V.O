//! `kivo-bench`: measures KIVO's engines, latency and resource budgets on this machine
//! (docs/architecture/BENCHMARKS.md). Engine defaults are chosen from its numbers.
//!
//!   kivo-bench machine                 print this machine's fingerprint
//!   kivo-bench list                    list the suites
//!   kivo-bench <suite>... [options]    run suites and save the results
//!
//! Options: --runs N (default 5, at least 5 for saved results) · --warmup N (default 1) ·
//! --out DIR (default: the current folder; results go to bench-results/ and docs/benchmarks/) ·
//! --no-save (print only) · --tier low (emulate the low tier, BENCHMARKS §2: this process runs
//! on 4 physical cores and its results are saved under a separate, "emulated" machine id).

mod harness;
mod machine;
mod report;
#[cfg(windows)]
mod speech;
mod stats;
mod suites;
#[cfg(windows)]
mod win;

use harness::Plan;
use machine::Machine;
use report::Output;
use std::path::PathBuf;
use std::process::ExitCode;

/// BENCHMARKS §2: the low tier is a 4-core laptop CPU.
#[cfg(windows)]
const LOW_TIER_CORES: usize = 4;

struct Options {
    suites: Vec<String>,
    plan: Plan,
    out: PathBuf,
    save: bool,
    /// Emulate the low reference tier on this machine.
    low_tier: bool,
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        suites: Vec::new(),
        plan: Plan { runs: 5, warmup: 1 },
        out: PathBuf::from("."),
        save: true,
        low_tier: false,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--runs" => {
                options.plan.runs = value("--runs")?
                    .parse()
                    .map_err(|_| "--runs takes a number")?;
            }
            "--warmup" => {
                options.plan.warmup = value("--warmup")?
                    .parse()
                    .map_err(|_| "--warmup takes a number")?;
            }
            "--out" => options.out = PathBuf::from(value("--out")?),
            "--no-save" => options.save = false,
            "--tier" => match value("--tier")?.as_str() {
                "low" => options.low_tier = true,
                other => return Err(format!("unknown tier {other} (only \"low\" is emulated)")),
            },
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            suite => options.suites.push(suite.to_owned()),
        }
    }
    if options.plan.runs == 0 {
        return Err("--runs must be at least 1".into());
    }
    // Saved numbers feed decisions, so they follow the repeatability rule.
    if options.save && options.plan.runs < 5 {
        return Err("saved results need at least 5 runs (use --no-save for a quick look)".into());
    }
    Ok(options)
}

fn main() -> ExitCode {
    #[cfg(windows)]
    win::dpi_aware();
    let options = match parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("kivo-bench: {e}");
            return ExitCode::from(2);
        }
    };
    match options.suites.first().map(String::as_str) {
        None | Some("help" | "--help") => {
            println!(
                "usage: kivo-bench machine | list | <suite>... [--runs N] [--warmup N] [--out DIR] [--no-save]"
            );
            ExitCode::SUCCESS
        }
        Some("list") => {
            for (name, about) in suites::ALL {
                println!("{name:10} {about}");
            }
            ExitCode::SUCCESS
        }
        Some("machine") => match Machine::detect() {
            Ok(m) => {
                println!("{}\n{}", m.id, m.describe());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("kivo-bench: {e}");
                ExitCode::FAILURE
            }
        },
        Some(_) => match run(&options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("kivo-bench: {e}");
                ExitCode::FAILURE
            }
        },
    }
}

fn run(options: &Options) -> Result<(), String> {
    let mut machine = Machine::detect()?;
    if options.low_tier {
        #[cfg(windows)]
        {
            let cores = win::limit_to_physical_cores(LOW_TIER_CORES)?;
            machine = machine.emulating_low_tier(cores);
        }
        #[cfg(not(windows))]
        return Err("the low tier is emulated on Windows only".into());
    }
    println!("{}", machine.describe());
    let db = if options.save {
        let paths = kivo_platform::Paths::user().ok_or("couldn't find the AppData folders")?;
        Some(kivo_store::Database::open(&paths.database()).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let output = Output {
        root: options.out.clone(),
    };
    for name in &options.suites {
        let mut suite = suites::create(name, options.plan)?;
        println!(
            "\n{name}: {} runs after {} warmup…",
            options.plan.runs, options.plan.warmup
        );
        let result = harness::execute(suite.as_mut(), options.plan)?;
        for m in &result.metrics {
            println!(
                "  {:<40} p50 {:>10.2}  p95 {:>10.2}  {}",
                m.name, m.p50, m.p95, m.unit
            );
        }
        if let Some(db) = &db {
            let json = serde_json::to_string(&result).map_err(|e| e.to_string())?;
            db.record_benchmark(&result.suite, &machine.id, result.started_at, &json)
                .map_err(|e| e.to_string())?;
            let (json, md) = output.save(&machine, &result)?;
            println!("  saved {} and {}", json.display(), md.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Options, String> {
        parse(list.iter().map(ToString::to_string))
    }

    #[test]
    fn options_are_parsed() {
        let o = args(&["ipc", "idle", "--runs", "7", "--warmup", "2", "--out", "x"]).unwrap();
        assert_eq!(o.suites, ["ipc", "idle"]);
        assert_eq!((o.plan.runs, o.plan.warmup), (7, 2));
        assert_eq!(o.out, PathBuf::from("x"));
        assert!(o.save);
    }

    #[test]
    fn saved_results_need_five_runs() {
        assert!(args(&["ipc", "--runs", "3"]).is_err());
        assert!(args(&["ipc", "--runs", "3", "--no-save"]).is_ok());
        assert!(args(&["ipc", "--bogus"]).is_err());
        assert!(args(&["ipc", "--runs"]).is_err());
        assert!(args(&["ipc", "--tier", "low"]).unwrap().low_tier);
        assert!(args(&["ipc", "--tier", "high"]).is_err());
    }
}
