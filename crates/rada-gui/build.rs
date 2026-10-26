// Windows resource insertion for the GUI exe: icon + version info.
// Same approach as crates/rada-cli/build.rs — generate resource.rc, compile
// with windres (w64devkit toolchain), pass the resulting loose .o via
// rustc-link-arg-bins (winres' static archive is dropped by GNU ld).
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rerun-if-changed=../../assets/icon/rada-client.ico");

    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let ico = Path::new(&manifest).join("../../assets/icon/rada-client.ico");
    assert!(
        ico.exists() && fs::metadata(&ico).unwrap().len() > 0,
        "missing icon file"
    );
    let ico = ico.canonicalize().unwrap();
    let ico = ico.to_str().unwrap().replace('\\', "/");

    let version = env::var("CARGO_PKG_VERSION").unwrap();

    let mut parts = [0u16; 4];
    for (i, seg) in version.split('.').take(3).enumerate() {
        parts[i] = seg.parse().expect("numeric version");
    }
    let (a, b, c, d) = (parts[0], parts[1], parts[2], parts[3]);

    let rc = format!(
        "#pragma code_page(65001)\n\
         1 VERSIONINFO\n\
         FILEVERSION {a}, {b}, {c}, {d}\n\
         PRODUCTVERSION {a}, {b}, {c}, {d}\n\
         FILEOS 0x40004\n\
         FILETYPE 0x1\n\
         {{\n\
         BLOCK \"StringFileInfo\"\n\
         {{\n\
         BLOCK \"000004b0\"\n\
         {{\n\
         VALUE \"FileVersion\", \"{version}\"\n\
         VALUE \"ProductVersion\", \"{version}\"\n\
         VALUE \"FileDescription\", \"Ukrainian Legislation Offline Client (GUI)\"\n\
         VALUE \"ProductName\", \"rada-client-gui\"\n\
         }}\n\
         }}\n\
         BLOCK \"VarFileInfo\"\n\
         {{\n\
         VALUE \"Translation\", 0x0, 0x04b0\n\
         }}\n\
         }}\n\
         1 ICON \"{ico}\"\n"
    );

    let out_dir = env::var("OUT_DIR").unwrap();
    let rc_path = Path::new(&out_dir).join("resource.rc");
    let obj_path = Path::new(&out_dir).join("resource.o");
    fs::write(&rc_path, rc).unwrap();

    let status = Command::new("windres")
        .arg("-c") // encoding for interpreting the input .rc
        .arg("65001")
        .arg("-O") // output format = coff object
        .arg("coff")
        .arg("-i")
        .arg(&rc_path)
        .arg("-o")
        .arg(&obj_path)
        .status()
        .expect("failed to run windres — is the w64devkit toolchain on PATH?");
    assert!(
        status.success(),
        "windres failed to compile {}",
        rc_path.display()
    );

    println!("cargo:rustc-link-arg-bins={}", obj_path.display());
}
