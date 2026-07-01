//! Resolución de ejecutables CONFIABLES (control de seguridad #1).
//!
//! Riesgo: en Windows, invocar un programa por nombre relativo (`Command::new("git")`)
//! desde un `cwd` controlado por el agente puede resolver un binario del propio
//! directorio actual. Un agente podría dejar un `git.exe` falso en el worktree y
//! lograr que Nexora lo ejecute en lugar del Git legítimo.
//!
//! Mitigación: resolvemos la ruta ABSOLUTA buscando SOLO en PATH (nunca en el
//! cwd), la canonicalizamos y comprobamos que sea un archivo. El llamador usa
//! siempre esa ruta absoluta. `is_inside` permite además rechazar un ejecutable
//! que viva dentro del repo/worktree del proyecto.

use std::path::{Path, PathBuf};

/// Ruta absoluta y canónica de `program`, buscada únicamente en PATH.
pub fn resolve(program: &str) -> Result<PathBuf, String> {
    let found = which(program).ok_or_else(|| format!("no se encontró '{program}' en PATH"))?;
    let canon = found
        .canonicalize()
        .map_err(|e| format!("no se pudo canonicalizar {found:?}: {e}"))?;
    if !canon.is_file() {
        return Err(format!("{canon:?} no es un archivo"));
    }
    Ok(canon)
}

/// Busca `program` en los directorios de PATH (respetando PATHEXT en Windows).
/// NO considera el directorio actual: esa es justamente la vía de suplantación.
fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into())
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    } else {
        Vec::new()
    };
    for dir in std::env::split_paths(&path) {
        let direct = dir.join(program);
        if direct.is_file() {
            return Some(direct);
        }
        for ext in &exts {
            let cand = dir.join(format!("{program}{ext}"));
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// ¿`exe` está DENTRO de `dir`? Un binario de confianza nunca debe vivir en el
/// repo/worktree controlado por el agente.
pub fn is_inside(exe: &Path, dir: &Path) -> bool {
    match (exe.canonicalize(), dir.canonicalize()) {
        (Ok(e), Ok(d)) => e.starts_with(d),
        _ => false,
    }
}

/// Resuelve `program` y RECHAZA si su ruta canónica cae dentro de cualquiera de
/// `deny_roots` (repo y sus worktrees). Exponer `is_inside` no basta: el runner
/// debe aplicar esta comprobación a cada ejecutable que vaya a lanzar (git,
/// codex, claude, node, npm), para que un binario suplantado dentro del proyecto
/// nunca se ejecute.
pub fn resolve_outside(program: &str, deny_roots: &[&Path]) -> Result<PathBuf, String> {
    let exe = resolve(program)?;
    for root in deny_roots {
        if is_inside(&exe, root) {
            return Err(format!(
                "'{program}' resuelto en {exe:?} está dentro de {root:?}: posible suplantación, se rechaza"
            ));
        }
    }
    Ok(exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_git_to_absolute_file_in_path() {
        // git está instalado en el entorno de desarrollo; si no, se salta.
        match resolve("git") {
            Ok(p) => {
                assert!(p.is_absolute(), "la ruta resuelta debe ser absoluta: {p:?}");
                assert!(p.is_file());
            }
            Err(_) => eprintln!("git no está en PATH: se salta"),
        }
        assert!(resolve("programa_que_no_existe_xyz").is_err());
    }

    #[test]
    fn is_inside_detects_containment() {
        let base = std::env::temp_dir();
        let inside = base.join("nexora_ti_child");
        std::fs::create_dir_all(&inside).unwrap();
        let f = inside.join("git.exe");
        std::fs::write(&f, "falso").unwrap();
        assert!(is_inside(&f, &base));
        assert!(!is_inside(&base, &inside));
        std::fs::remove_dir_all(&inside).ok();
    }

    #[test]
    fn resolve_outside_rejects_binary_inside_deny_root() {
        // git legítimo resuelto desde PATH NO está dentro de un worktree ajeno.
        let elsewhere = std::env::temp_dir().join("nexora_ro_repo");
        std::fs::create_dir_all(&elsewhere).unwrap();
        if let Ok(p) = resolve_outside("git", &[&elsewhere]) {
            assert!(p.is_absolute());
        }
        // Si el deny_root fuese un ancestro del git resuelto, debe rechazar.
        // Simulamos con el ancestro real del binario resuelto.
        if let Ok(git) = resolve("git") {
            if let Some(parent) = git.parent() {
                assert!(resolve_outside("git", &[parent]).is_err());
            }
        }
        std::fs::remove_dir_all(&elsewhere).ok();
    }
}
