//! build.rs — ibm_informix_bridge
//!
//! Selects and locates a client library to reach Informix:
//! - `db2` / `libdb2`   (DRDA, commonly port 9089)
//! - `ifcli` / `libifcli` (SQLI onsoctcp, commonly port 9088)
//!
//! Set `IFX_CLIENT_LIB` to control selection:
//! - `db2`   → force DRDA client
//! - `ifcli` → force SQLI client
//! - unset / `auto` → prefer `db2`, fallback to `ifcli`

use std::env;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Copy)]
enum ClientPref {
    Auto,
    Db2,
    Ifcli,
}

struct LinkTarget {
    lib_dir: PathBuf,
    link_lib: &'static str,
    label: String,
}

fn main() {
    // Rebuild whenever any of these change
    for var in &[
        "IFX_CLIENT_LIB",
        "IBM_DB_HOME",
        "INFORMIX_LIB_DIR",
        "IFX_LIB_DIR",
        "INFORMIXDIR",
    ] {
        println!("cargo:rerun-if-env-changed={}", var);
    }

    match find_client() {
        Some(target) => {
            println!(
                "cargo:warning=ibm_informix_bridge: selected client {} via {}",
                target.link_lib,
                target.label
            );
            println!("cargo:rustc-link-search=native={}", target.lib_dir.display());
            println!("cargo:rustc-link-lib={}", target.link_lib);
        }
        None => {
            panic!(
                "\n\
                ╔══════════════════════════════════════════════════════════════╗\n\
                ║  ibm_informix_bridge: could not locate a client library       ║\n\
                ╠══════════════════════════════════════════════════════════════╣\n\
                ║  IFX_CLIENT_LIB=db2   expects libdb2 (DRDA, usually 9089)    ║\n\
                ║  IFX_CLIENT_LIB=ifcli expects libifcli/iclit09* (9088 SQLI)  ║\n\
                ║                                                              ║\n\
                ║  Useful environment variables:                                ║\n\
                ║    export IFX_CLIENT_LIB=ifcli|db2                            ║\n\
                ║    export INFORMIXDIR=/path/to/informix/csdk                  ║\n\
                ║    export IBM_DB_HOME=/path/to/clidriver                      ║\n\
                ║                                                              ║\n\
                ║  Auto mode prefers db2 then falls back to ifcli.              ║\n\
                ╚══════════════════════════════════════════════════════════════╝\n"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Candidate validation helpers
// ---------------------------------------------------------------------------

fn has_prefix(dir: &PathBuf, prefix: &str) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(prefix) {
                return true;
            }
        }
    }
    false
}

fn check_db2(dir: PathBuf, label: &str) -> Option<LinkTarget> {
    if has_prefix(&dir, "libdb2") {
        return Some(LinkTarget {
            lib_dir: dir,
            link_lib: "db2",
            label: label.to_string(),
        });
    }
    None
}

fn check_ifcli(dir: PathBuf, label: &str) -> Option<LinkTarget> {
    if has_prefix(&dir, "libifcli") {
        return Some(LinkTarget {
            lib_dir: dir,
            link_lib: "ifcli",
            label: label.to_string(),
        });
    }

    // macOS CSDK can expose iclit09b.dylib without a libifcli symlink.
    if has_prefix(&dir, "iclit09") {
        let link_lib = if has_prefix(&dir, "iclit09b") {
            "iclit09b"
        } else {
            "iclit09a"
        };
        return Some(LinkTarget {
            lib_dir: dir,
            link_lib,
            label: label.to_string(),
        });
    }

    None
}

// ---------------------------------------------------------------------------
// Discovery strategies
// ---------------------------------------------------------------------------

fn parse_pref() -> ClientPref {
    match env::var("IFX_CLIENT_LIB") {
        Ok(v) if v.eq_ignore_ascii_case("db2") => ClientPref::Db2,
        Ok(v) if v.eq_ignore_ascii_case("ifcli") => ClientPref::Ifcli,
        _ => ClientPref::Auto,
    }
}

fn discover_db2() -> Option<LinkTarget> {
    if let Ok(home) = env::var("IBM_DB_HOME") {
        let root = PathBuf::from(&home);
        if let Some(p) = check_db2(root.join("lib"), "IBM_DB_HOME/lib") {
            return Some(p);
        }
        if let Some(p) = check_db2(root.clone(), "IBM_DB_HOME") {
            return Some(p);
        }
    }

    for var in &["INFORMIX_LIB_DIR", "IFX_LIB_DIR"] {
        if let Ok(dir) = env::var(var) {
            if let Some(p) = check_db2(PathBuf::from(dir), var) {
                return Some(p);
            }
        }
    }

    if let Ok(dir) = env::var("INFORMIXDIR") {
        let root = PathBuf::from(dir);
        for sub in &["lib/cli", "lib/esql", "lib"] {
            let label = format!("INFORMIXDIR/{}", sub);
            if let Some(p) = check_db2(root.join(sub), &label) {
                return Some(p);
            }
        }
    }

    discover_db2_via_python()
}

fn discover_ifcli() -> Option<LinkTarget> {
    for var in &["INFORMIX_LIB_DIR", "IFX_LIB_DIR"] {
        if let Ok(dir) = env::var(var) {
            if let Some(p) = check_ifcli(PathBuf::from(dir), var) {
                return Some(p);
            }
        }
    }

    if let Ok(dir) = env::var("INFORMIXDIR") {
        let root = PathBuf::from(dir);
        for sub in &["lib/cli", "lib/esql", "lib"] {
            let label = format!("INFORMIXDIR/{}", sub);
            if let Some(p) = check_ifcli(root.join(sub), &label) {
                return Some(p);
            }
        }
    }

    None
}

fn find_client() -> Option<LinkTarget> {
    match parse_pref() {
        ClientPref::Db2 => discover_db2(),
        ClientPref::Ifcli => discover_ifcli(),
        ClientPref::Auto => discover_db2().or_else(discover_ifcli),
    }
}

/// Ask Python where ibm_db's bundled clidriver lives and return the lib/ path.
///
/// The ibm_db wheel ships its own clidriver at:
///   <site-packages>/ibm_db/clidriver/lib/libdb2.so
fn discover_db2_via_python() -> Option<LinkTarget> {
    let script = concat!(
        "import ibm_db, os; ",
        "print(os.path.join(os.path.dirname(ibm_db.__file__), 'clidriver', 'lib'))"
    );

    for python in &["python3", "python"] {
        let Ok(out) = Command::new(python).args(["-c", script]).output() else {
            continue;
        };
        if !out.status.success() {
            continue;
        }
        let path_str = match String::from_utf8(out.stdout) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let path = PathBuf::from(path_str.trim());
        if let Some(p) = check_db2(path, &format!("{} ibm_db.clidriver.lib", python)) {
            return Some(p);
        }
    }

    None
}
