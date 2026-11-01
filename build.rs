// Generates the exe icon from the same artwork the tray uses and embeds it as resource 1,
// together with version info and a manifest (visual styles, per-monitor DPI).
#[path = "src/art.rs"]
mod art;

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=src/art.rs");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let authors = env::var("CARGO_PKG_AUTHORS").unwrap();
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    let date = build_date();
    println!("cargo:rustc-env=KEEPAWAKE_COMMIT={commit}");
    println!("cargo:rustc-env=KEEPAWAKE_BUILD_DATE={date}");

    let ico = out.join("app.ico");
    fs::write(&ico, art::ico(&[16, 20, 24, 32, 40, 48, 64, 96], art::Look::Awake)).unwrap();

    let mut nums: Vec<u16> = version.split(['.', '-', '+']).take(3).map(|p| p.parse().unwrap_or(0)).collect();
    nums.resize(4, 0);
    let dotted = format!("{}.{}.{}.{}", nums[0], nums[1], nums[2], nums[3]);
    let commas = dotted.replace('.', ",");

    let manifest = out.join("app.manifest");
    fs::write(&manifest, format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="KeepAwake" version="{dotted}"/>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
        processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application><supportedOS Id="{{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}}"/></application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#)).unwrap();

    let path = |p: &PathBuf| p.display().to_string().replace('\\', "/");
    let rc = out.join("app.rc");
    fs::write(&rc, format!(r#"1 ICON "{ico}"
1 24 "{manifest}"
1 VERSIONINFO
FILEVERSION {commas}
PRODUCTVERSION {commas}
FILEOS 0x40004
FILETYPE 1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "{authors}"
      VALUE "FileDescription", "KeepAwake"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "keepawake"
      VALUE "LegalCopyright", "Copyright (c) 2026 {authors}. MIT License."
      VALUE "OriginalFilename", "keepawake.exe"
      VALUE "ProductName", "KeepAwake"
      VALUE "ProductVersion", "{version} ({commit})"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#, ico = path(&ico), manifest = path(&manifest))).unwrap();
    embed_resource::compile(&rc, embed_resource::NONE).manifest_optional().unwrap();
}

/// Today's UTC date as YYYY-MM-DD.
fn build_date() -> String {
    let days = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0) as i64;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}
