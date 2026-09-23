//! Makes `cargo test --workspace` build the dummy app, which the UIA journey and the event tests
//! in `kivo-platform-windows` start (TOOLS_AND_CONTROL §10). Cargo builds a package's binaries
//! for its integration tests, and all builds finish before any test runs.

#[test]
fn the_dummy_app_is_built() {
    let exe = std::path::Path::new(env!("CARGO_BIN_EXE_kivo-test-app"));
    assert!(exe.is_file(), "{} is missing", exe.display());
}
