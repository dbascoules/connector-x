//! build.rs — ibm_informix_bridge
//!
//! Locates `libdb2.so` (IBM Data Server Driver, DRDA) and emits the
//! necessary `cargo:rustc-link-search` directive.
//!
//! Note on protocols
//! -----------------
//! Informix exposes TWO TCP listeners:
//!   port 9088  onsoctcp  →  Informix SQLI native  → requires libifcli (CSDK)
//!   port 9089  drsoctcp  →  DRDA / DB2 wire       → requires libdb2  (this crate)
//!
//! We target port 9089 because libdb2 is freely redistributed inside the
//! Python `ibm_db` wheel (no IBM account required).
//!
//! Discovery order
//! ---------------
//! 1. IBM_DB_HOME         — point to the clidriver root;  lib/ subdir is used
//! 2. INFORMIX_LIB_DIR /
//!    IFX_LIB_DIR         — backward-compat: direct path to the lib directory
//! 3. INFORMIXDIR         — Informix CSDK layout; tries lib/cli, lib/esql, lib
//! 4. Python ibm_db       — `python3 -c "import ibm_db …"` discovers clidriver
//! 5. Panic with actionable message

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Rebuild whenever any of these change
    for var in &[
        "IBM_DB_HOME",
        "INFORMIX_LIB_DIR",
        "IFX_LIB_DIR",
        "INFORMIXDIR",
    ] {
        println!("cargo:rerun-if-env-changed={}", var);
    }

    match find_libdb2() {
        Some(lib_dir) => {
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
            // Also emit the link directive; the #[link(name="db2")] in src/lib.rs
            // will do the same, but being explicit here avoids surprises when the
            // crate is used as a dependency.
            println!("cargo:rustc-link-lib=db2");
        }
        None => {
            panic!(
                "\n\
                ╔══════════════════════════════════════════════════════════════╗\n\
                ║  ibm_informix_bridge: could not locate libdb2.so             ║\n\
                ╠══════════════════════════════════════════════════════════════╣\n\
                ║  Quick fix (embeds libdb2 automatically):                    ║\n\
                ║    pip install ibm_db                                        ║\n\
                ║                                                              ║\n\
                ║  Or point to an existing IBM CLIDriver installation:         ║\n\
                ║    export IBM_DB_HOME=/path/to/clidriver                     ║\n\
                ║                                                              ║\n\
                ║  Reminder: this bridge uses DRDA (port 9089), NOT Informix   ║\n\
                ║  SQLI (port 9088).  No Informix CSDK (libifcli) required.    ║\n\
                ╚══════════════════════════════════════════════════════════════╝\n"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Candidate validation helpers
// ---------------------------------------------------------------------------

fn has_libdb2(dir: &PathBuf) -> bool {
    // Accept libdb2.so, libdb2.so.1, libdb2.so.1.0.x …
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("libdb2") {
                return true;
            }
        }
    }
    false
}

fn check(dir: PathBuf, label: &str) -> Option<PathBuf> {
    if has_libdb2(&dir) {
        println!(
            "cargo:warning=ibm_informix_bridge: libdb2 found via {} → {}",
            label,
            dir.display()
        );
        Some(dir)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Discovery strategies
// ---------------------------------------------------------------------------

fn find_libdb2() -> Option<PathBuf> {
    // --- Strategy 1: IBM_DB_HOME -------------------------------------------
    if let Ok(home) = env::var("IBM_DB_HOME") {
        let root = PathBuf::from(&home);
        // The canonical CLIDriver layout puts libs in <root>/lib/
        if let Some(p) = check(root.join("lib"), "IBM_DB_HOME/lib") { return Some(p); }
        // Some installations keep libs at the root itself
        if let Some(p) = check(root.clone(), "IBM_DB_HOME") { return Some(p); }
    }

    // --- Strategy 2: legacy INFORMIX_LIB_DIR / IFX_LIB_DIR ----------------
    for var in &["INFORMIX_LIB_DIR", "IFX_LIB_DIR"] {
        if let Ok(dir) = env::var(var) {
            if let Some(p) = check(PathBuf::from(dir), var) { return Some(p); }
        }
    }

    // --- Strategy 3: INFORMIXDIR (Informix CSDK layout) --------------------
    if let Ok(dir) = env::var("INFORMIXDIR") {
        let root = PathBuf::from(dir);
        for sub in &["lib/cli", "lib/esql", "lib"] {
            let label = format!("INFORMIXDIR/{}", sub);
            if let Some(p) = check(root.join(sub), &label) { return Some(p); }
        }
    }

    // --- Strategy 4: Python ibm_db package ----------------------------------
    discover_via_python()
}

/// Ask Python where ibm_db's bundled clidriver lives and return the lib/ path.
///
/// The ibm_db wheel ships its own clidriver at:
///   <site-packages>/ibm_db/clidriver/lib/libdb2.so
fn discover_via_python() -> Option<PathBuf> {
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
        if let Some(p) = check(path, &format!("{} ibm_db.clidriver.lib", python)) {
            return Some(p);
        }
    }

    None
}
