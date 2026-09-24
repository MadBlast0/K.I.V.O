//! ARCH-38 (invariant 1, "the runtime stays provider-independent"): `kivo-core` holds KIVO's
//! vocabulary only. It must never depend on a brain, an HTTP client, a platform crate or anything
//! else that ties the core to a provider or an OS; this test fails the build if it does.

/// The only dependencies `kivo-core` may have.
const ALLOWED: &[&str] = &[
    // Dates, times and time zones for routine schedules (ROUT-11): pure computation.
    "jiff",
    "serde",
    "serde_json",
    "thiserror",
    "tokio",
    "tokio-util",
    "tracing",
    "uuid",
    "zeroize",
    "ts-rs",
];

fn dependencies(section: &str) -> Vec<String> {
    let manifest = include_str!("../Cargo.toml");
    let mut in_section = false;
    let mut names = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == format!("[{section}]");
            continue;
        }
        if in_section
            && !line.is_empty()
            && !line.starts_with('#')
            && let Some((name, _)) = line.split_once('=')
        {
            names.push(name.trim().trim_end_matches(".workspace").to_owned());
        }
    }
    names
}

#[test]
fn kivo_core_has_no_provider_or_platform_dependency() {
    let deps = dependencies("dependencies");
    assert!(!deps.is_empty(), "the manifest was read");
    for dep in &deps {
        assert!(
            ALLOWED.contains(&dep.as_str()),
            "kivo-core gained the dependency `{dep}`: providers and platforms belong in their own \
             crates (ARCHITECTURE §8, invariant 1)"
        );
    }
    // Target-specific dependencies are platform code by definition.
    let manifest = include_str!("../Cargo.toml");
    assert!(
        !manifest.contains("[target."),
        "kivo-core has target-specific dependencies"
    );
}

/// Invariant 2, "the UI is not the core runtime" (ARCHITECTURE §8): the app reaches KIVO only over
/// IPC. Its Rust side may use KIVO's vocabulary (`kivo-core`), the IPC crate and the platform
/// traits, never the store, the secrets, the tools, the brains, speech or memory.
#[test]
fn the_app_reaches_kivo_only_through_ipc() {
    let manifest = include_str!("../../../apps/kivo-app/src-tauri/Cargo.toml");
    let kivo: Vec<&str> = manifest
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('=').map(|(name, _)| name.trim()))
        .map(|name| name.trim_end_matches(".workspace"))
        .filter(|name| name.starts_with("kivo-") && *name != "kivo-app")
        .collect();
    assert!(
        kivo.contains(&"kivo-ipc"),
        "the manifest was read: {kivo:?}"
    );
    for name in &kivo {
        assert!(
            ["kivo-core", "kivo-ipc", "kivo-platform"].contains(name),
            "the app depends on `{name}`: the app is a client of the runtime and reaches it only              over IPC (ARCHITECTURE §8, invariant 2)"
        );
    }
}
