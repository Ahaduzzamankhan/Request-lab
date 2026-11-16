//! Windows resources: the taskbar icon and version metadata for requestlab.exe.
//!
//! The check is on the *target* operating system, not the host, so a Windows
//! cross-build embeds the icon exactly like a native one. `manifest_optional`
//! treats a machine without a resource compiler (`rc.exe`) as "nothing to do",
//! while a compiler that runs and fails fails the build instead of silently
//! shipping an executable with the wrong icon.

fn main() {
    println!("cargo:rerun-if-changed=res/requestlab.rc");
    println!("cargo:rerun-if-changed=res/requestlab.ico");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        embed_resource::compile("res/requestlab.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("RequestLab's Windows icon and version resource must embed");
    }
}
