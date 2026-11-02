//! The repository's own scripts, run as a contributor runs them: a call without what the script
//! needs says how to call it, exits 2 and changes nothing. `models_sync.py` replaces the model
//! catalog's `data/` with what it copies, so it runs here as a copy of itself, beside a catalog
//! crate of its own: the repository's `data/` is never at stake, even when the script is wrong.

use std::path::PathBuf;

use super::root;
use crate::common::home::{Home, output, text};

/// The copy's catalog in `home`: the providers its `src/lib.rs` lists first, and a file of
/// `data/` that a run which copies would remove.
fn catalog_beside(home: &Home) -> PathBuf {
    let scripts = home.path().join("scripts");
    let models = home.path().join("packages/providers/models");
    for dir in [&scripts, &models.join("src"), &models.join("data")] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::copy(
        root().join("scripts/models_sync.py"),
        scripts.join("models_sync.py"),
    )
    .unwrap();
    std::fs::write(
        models.join("src/lib.rs"),
        "pub const POPULAR: [&str; 1] = [\"demo\"];\n",
    )
    .unwrap();
    let kept = models.join("data/kept.toml");
    std::fs::write(&kept, "# built in\n").unwrap();
    kept
}

/// Whether `program` is on the `PATH`.
fn installed(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

#[test]
fn models_sync_without_a_clone_says_how_it_is_run_and_keeps_the_data() {
    if !installed("python3") {
        eprintln!("python3 is not installed: models_sync.py not run");
        return;
    }
    let home = Home::new();
    let kept = catalog_beside(&home);
    let empty = home.path().join("not-a-clone");
    std::fs::create_dir_all(&empty).unwrap();
    for args in [&[][..], &[empty.as_path()]] {
        let run = output(
            home.command("python3")
                .arg(home.path().join("scripts/models_sync.py"))
                .args(args),
        );
        let stderr = text(&run.stderr);
        assert_eq!(run.status.code(), Some(2), "{args:?}: {stderr}");
        assert!(
            stderr
                .lines()
                .any(|line| line.starts_with("usage: ") && line.contains("models_sync.py")),
            "{args:?}: {stderr}"
        );
        assert!(!stderr.contains("Traceback"), "{args:?}: {stderr}");
        assert!(kept.is_file(), "{args:?} removed data/: {stderr}");
    }
}
