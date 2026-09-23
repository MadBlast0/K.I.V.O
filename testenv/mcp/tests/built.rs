//! Makes `cargo test --workspace` build the test MCP server, which the MCP client and hostile
//! server tests start (TOOLS_AND_CONTROL §10, TOOL-41). Cargo builds a package's binaries for its
//! integration tests, and all builds finish before any test runs.

#[test]
fn the_test_mcp_server_is_built() {
    let exe = std::path::Path::new(env!("CARGO_BIN_EXE_kivo-test-mcp"));
    assert!(exe.is_file(), "{} is missing", exe.display());
}
