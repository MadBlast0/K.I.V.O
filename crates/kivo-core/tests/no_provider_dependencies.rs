//! ARCH-38 (invariant 1, "the runtime stays provider-independent"): `kivo-core` holds KIVO's
//! vocabulary only. It must never depend on a brain, an HTTP client, a platform crate or anything
//! else that ties the core to a provider or an OS; this test fails the build if it does.

/// The only dependencies `kivo-core` may have.
const ALLOWED: &[&str] = &[
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
