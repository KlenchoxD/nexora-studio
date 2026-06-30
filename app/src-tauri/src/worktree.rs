//! Ayudas de git (opcionales). Nexora trabaja directo sobre tu carpeta local;
//! si además resulta ser un repo git, lo usamos para mostrar la rama y el diff
//! de cambios. git NO es obligatorio.

use std::path::Path;
use std::process::Command;

fn run_git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// ¿Es `path` un repositorio git?
pub fn is_git_repo(path: &Path) -> bool {
    run_git(path, &["rev-parse", "--is-inside-work-tree"])
        .map(|s| s.trim() == "true")
        .unwrap_or(false)
}

/// Diff del árbol de trabajo respecto al último commit (para revisar lo que
/// cambió el agente). Solo tiene sentido si la carpeta es un repo git.
pub fn diff(repo: &Path) -> Result<String, String> {
    run_git(repo, &["diff", "HEAD"])
}

/// Rama actual del repo (para la cabecera). Solo si es un repo git.
pub fn current_branch(repo: &Path) -> Result<String, String> {
    run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn detects_repo_and_diffs_changes() {
        let dir: PathBuf =
            std::env::temp_dir().join(format!("nexora-git-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        // una carpeta normal no es repo
        assert!(!is_git_repo(&dir));

        run_git(&dir, &["init", "-b", "main"]).unwrap();
        run_git(&dir, &["config", "user.email", "t@t"]).unwrap();
        run_git(&dir, &["config", "user.name", "t"]).unwrap();
        fs::write(dir.join("README.md"), "hi").unwrap();
        run_git(&dir, &["add", "-A"]).unwrap();
        run_git(&dir, &["commit", "-m", "init"]).unwrap();

        assert!(is_git_repo(&dir));
        assert_eq!(current_branch(&dir).unwrap(), "main");

        // un cambio en el árbol de trabajo aparece en el diff
        fs::write(dir.join("README.md"), "changed").unwrap();
        assert!(diff(&dir).unwrap().contains("changed"));

        let _ = fs::remove_dir_all(&dir);
    }
}
