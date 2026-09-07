fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bin=dgo=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bin=dgo=/MANIFESTUAC:level='asInvoker' uiAccess='false'");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
