//! Windows resources: the taskbar icon and version metadata for requestlab.exe.
//!
//! The resource compiler is only required on Windows; a host without one keeps
//! building and gets a warning instead of a broken build.

fn main() {
    println!("cargo:rerun-if-changed=res/requestlab.rc");
    println!("cargo:rerun-if-changed=res/requestlab.ico");

    #[cfg(windows)]
    if let Err(error) = embed_resource::compile("res/requestlab.rc", embed_resource::NONE) {
        println!("cargo:warning=RequestLab icon was not embedded: {error}");
    }
}
