// Windows resource insertion: exe icon + version info.
// cargo:rustc-link-arg requires rustc >= 1.50
// Hand-rolled instead of winres: winres emits a static archive
// (libresource.a) that GNU ld never pulls in (resource objects define no
// symbols), so no .rsrc section. A loose .o passed via rustc-link-arg-bins
// is always included. Requires `windres` on PATH only for windows targets.
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
    // relative to the manifest, robust against mixed slashes; forward
    // slashes are valid in .rc for windres
    let ico = Path::new(&manifest).join("../../assets/icon/rada-client.ico");
    assert!(
        ico.exists() && fs::metadata(&ico).unwrap().len() > 0,
        "missing icon file"
    );
    let ico = ico.canonicalize().unwrap();
    let ico = ico.to_str().unwrap().replace('\\', "/");

    let version = env::var("CARGO_PKG_VERSION").unwrap();

    // version components must be all u16s
    let mut parts = [0u16; 4]; // 0.1.0 -> [0, 1, 0, 0]
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
         VALUE \"FileDescription\", \"Ukrainian Legislation Offline Client\"\n\
         VALUE \"ProductName\", \"rada-client\"\n\
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

    // gnu toolchain: windres -> COFF object; msvc: llvm/windres fallback is
    // out of scope here, we build x86_64-pc-windows-gnu only
    let status = Command::new("windres")
        .arg("-c") // encoding for interpreting the input .rc
        .arg("65001")
        .arg("-O") // range format = coff object
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

    // loose object linked only into binaries; always included, never filtered
    println!("cargo:rustc-link-arg-bins={}", obj_path.display());
}
