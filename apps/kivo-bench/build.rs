//! sherpa-onnx's static library asks for the static C runtime (LIBCMT) while everything else
//! links the DLL one; left alone, MSVC warns (LNK4098) that the two conflict. One runtime is
//! used: the DLL one, like the rest of KIVO.

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.ends_with("-windows-msvc") {
        println!("cargo:rustc-link-arg=/NODEFAULTLIB:libcmt.lib");
    }
}
