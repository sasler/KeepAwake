// Generates the exe icon from the same artwork the tray uses and embeds it as resource 1.
#[path = "src/art.rs"]
mod art;

use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=src/art.rs");
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ico = out.join("app.ico");
    fs::write(&ico, art::ico(&[16, 20, 24, 32, 40, 48, 64, 96], art::Look::Awake)).unwrap();
    let rc = out.join("app.rc");
    fs::write(&rc, format!("1 ICON \"{}\"\n", ico.display().to_string().replace('\\', "/"))).unwrap();
    embed_resource::compile(&rc, embed_resource::NONE).manifest_optional().unwrap();
}
