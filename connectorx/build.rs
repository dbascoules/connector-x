use std::env;
use std::path::PathBuf;

fn emit_link_search(path: PathBuf) {
    if path.is_dir() {
        println!("cargo:rustc-link-search=native={}", path.display());
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_SRC_INFORMIX");
    println!("cargo:rerun-if-env-changed=INFORMIX_LIB_DIR");
    println!("cargo:rerun-if-env-changed=IFX_LIB_DIR");
    println!("cargo:rerun-if-env-changed=INFORMIXDIR");

    if env::var_os("CARGO_FEATURE_SRC_INFORMIX").is_none() {
        return;
    }

    // The src_informix feature now uses ibm_informix_bridge (libdb2 / DRDA) instead of
    // libifcli (SQLI).  Link-search / lib directives are handled by the bridge crate's
    // own build.rs, so nothing is needed here anymore.
    // The env-var checks below are kept only for projects that still build against a
    // local Informix CSDK for other purposes.
    if let Some(lib_dir) = env::var_os("INFORMIX_LIB_DIR").or_else(|| env::var_os("IFX_LIB_DIR")) {
        emit_link_search(PathBuf::from(lib_dir));
        return;
    }

    if let Some(informix_dir) = env::var_os("INFORMIXDIR") {
        let informix_dir = PathBuf::from(informix_dir);
        emit_link_search(informix_dir.join("lib/cli"));
        emit_link_search(informix_dir.join("lib/esql"));
        emit_link_search(informix_dir.join("lib"));
    }
}
