//! How KIVO checks coding work before saying it's done (BRAIN-31, BRAIN-32, plan §137): the
//! project's own test or build command, from the workspace's notes when they name one ("tests:
//! cargo nextest run"), else from the project's files.

use std::path::Path;

/// What the request was about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    Tests,
    Build,
}

/// Whether a coding request is about tests or a build (and so has something to check).
pub fn wanted(request: &str) -> Option<Check> {
    let lower = request.to_lowercase();
    let has = |words: &[&str]| {
        lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| words.contains(&w))
    };
    if has(&["test", "tests", "failing", "fails", "spec", "specs"]) {
        Some(Check::Tests)
    } else if has(&["build", "compile", "compiles", "compiling", "builds"]) {
        Some(Check::Build)
    } else if has(&["fix", "bug", "broken", "error", "errors"]) {
        Some(Check::Tests)
    } else {
        None
    }
}

/// A command named in the workspace's notes: a line `tests: …` or `build: …`.
fn from_notes(notes: &str, check: Check) -> Option<String> {
    let keys: &[&str] = match check {
        Check::Tests => &["tests:", "test:", "test command:"],
        Check::Build => &["build:", "build command:"],
    };
    notes.lines().find_map(|line| {
        let l = line.trim().trim_start_matches(['-', '*', ' ']);
        let lower = l.to_lowercase();
        keys.iter().find_map(|k| {
            lower
                .starts_with(k)
                .then(|| l[k.len()..].trim().trim_matches('`').to_owned())
                .filter(|c| !c.is_empty())
        })
    })
}

/// The project's command for `check` in `dir`.
pub fn command(dir: &Path, notes: &str, check: Check) -> Option<String> {
    if let Some(c) = from_notes(notes, check) {
        return Some(c);
    }
    let has = |f: &str| dir.join(f).exists();
    let any_ext = |ext: &str| {
        std::fs::read_dir(dir).ok().is_some_and(|mut it| {
            it.any(|e| {
                e.ok()
                    .is_some_and(|e| e.path().extension().is_some_and(|x| x == ext))
            })
        })
    };
    let node = |script: &str| {
        let body = std::fs::read_to_string(dir.join("package.json")).ok()?;
        let json: serde_json::Value = serde_json::from_str(&body).ok()?;
        json["scripts"][script].as_str()?;
        let runner = if has("pnpm-lock.yaml") {
            "pnpm"
        } else if has("yarn.lock") {
            "yarn"
        } else if has("bun.lockb") || has("bun.lock") {
            "bun"
        } else {
            "npm"
        };
        Some(match (runner, script) {
            ("npm", "test") => "npm test".to_owned(),
            ("npm", s) => format!("npm run {s}"),
            (r, s) => format!("{r} {s}"),
        })
    };
    match check {
        Check::Tests => {
            if has("Cargo.toml") {
                Some("cargo test".into())
            } else if has("package.json") {
                node("test")
            } else if has("pyproject.toml") || has("pytest.ini") || has("setup.cfg") {
                Some("python -m pytest".into())
            } else if has("go.mod") {
                Some("go test ./...".into())
            } else if any_ext("sln") || any_ext("csproj") {
                Some("dotnet test".into())
            } else if has("pom.xml") {
                Some("mvn test".into())
            } else if has("build.gradle") || has("build.gradle.kts") {
                Some("gradle test".into())
            } else {
                None
            }
        }
        Check::Build => {
            if has("Cargo.toml") {
                Some("cargo build".into())
            } else if has("package.json") {
                node("build")
            } else if has("go.mod") {
                Some("go build ./...".into())
            } else if any_ext("sln") || any_ext("csproj") {
                Some("dotnet build".into())
            } else if has("pom.xml") {
                Some("mvn package".into())
            } else if has("build.gradle") || has("build.gradle.kts") {
                Some("gradle build".into())
            } else {
                None
            }
        }
    }
}

/// The last few lines of a command's output, for saying what still fails.
pub fn tail(stdout: &str, stderr: &str, lines: usize) -> String {
    let all = format!("{stdout}\n{stderr}");
    let kept: Vec<&str> = all
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    kept[kept.len().saturating_sub(lines)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_about_tests_and_builds_have_a_check() {
        assert_eq!(
            wanted("fix the failing tests in K.I.V.O"),
            Some(Check::Tests)
        );
        assert_eq!(wanted("why doesn't it compile?"), Some(Check::Build));
        assert_eq!(wanted("fix this bug"), Some(Check::Tests));
        assert_eq!(wanted("explain the architecture"), None);
        assert_eq!(wanted("attest the latest"), None, "whole words only");
    }

    #[test]
    fn the_projects_own_commands_are_found() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        assert_eq!(command(d, "", Check::Tests), None);
        std::fs::write(
            d.join("package.json"),
            r#"{"scripts":{"test":"vitest","build":"vite build"}}"#,
        )
        .unwrap();
        assert_eq!(command(d, "", Check::Tests).as_deref(), Some("npm test"));
        std::fs::write(d.join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(command(d, "", Check::Build).as_deref(), Some("pnpm build"));
        std::fs::write(d.join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(command(d, "", Check::Tests).as_deref(), Some("cargo test"));
        let notes = "Some notes\n- Tests: `cargo nextest run`\n";
        assert_eq!(
            command(d, notes, Check::Tests).as_deref(),
            Some("cargo nextest run"),
            "the workspace's notes win"
        );
        assert_eq!(tail("a\nb\n\nc", "d", 2), "c\nd");
    }
}
