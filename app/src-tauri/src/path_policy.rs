//! Validación DETERMINISTA de rutas para el orquestador (Hito 1).
//!
//! El prompt NO es una medida de seguridad: aunque al agente se le diga "solo
//! toca src/auth", el orquestador debe VERIFICAR contra el diff real antes de
//! permitir un commit. Este módulo decide, por cada ruta modificada, si está
//! permitida (dentro de `owned`, no en `forbidden`, sin escaparse del worktree).
//!
//! ALCANCE (importante): la validación es LÉXICA — `..`, rutas absolutas, barra
//! inicial y coincidencia de globs. NO resuelve symlinks ni canonicaliza el
//! destino real en disco: un symlink dentro del worktree que apunte fuera NO se
//! detecta aquí. Esa garantía corresponde al sandbox del agente (Codex `-s`,
//! Claude permisos). No confíes en PathPolicy para contención de symlinks.

use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::Path;

/// Rutas prohibidas por defecto para cualquier tarea. En un worktree `.git`
/// suele ser un ARCHIVO (no carpeta), por eso se bloquean `.git` Y `.git/**`.
/// Se protegen también archivos que alteran integración/seguridad.
pub fn default_forbidden() -> Vec<String> {
    [
        ".git",
        ".git/**",
        ".gitmodules",
        ".gitattributes",
        ".env",
        ".env.*",
        "**/.env",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allowed,
    /// Coincide con un patrón prohibido (ej. `.env`, `.git/**`).
    Forbidden(String),
    /// No cae dentro de ningún patrón `owned` de la tarea.
    OutsideOwned(String),
    /// Intenta salir del worktree (`..`, ruta absoluta o symlink hacia fuera).
    Escape(String),
}

pub struct PathPolicy {
    owned: GlobSet,
    forbidden: GlobSet,
}

impl PathPolicy {
    /// `owned` NO puede estar vacío (fail-closed): una lista vacía sería
    /// acceso total accidental. Para el worktree entero, declara `["**"]`
    /// explícitamente.
    pub fn new(owned: &[String], forbidden: &[String]) -> Result<Self, String> {
        if owned.is_empty() {
            return Err(
                "ownedPaths vacío es inválido; declara [\"**\"] para todo el worktree".into(),
            );
        }
        Ok(Self {
            owned: build_set(owned)?,
            forbidden: build_set(forbidden)?,
        })
    }

    /// `rel_path`: ruta RELATIVA a la raíz del worktree. Acepta separadores
    /// `/` o `\`; se normaliza a `/` para el matching de globs.
    pub fn check(&self, rel_path: &str) -> PolicyDecision {
        let norm = rel_path.replace('\\', "/");
        // Escape: ruta absoluta o cualquier componente `..` → fuera del worktree.
        // `starts_with('/')` cubre las rutas absolutas POSIX (`/etc/...`), que en
        // Windows `Path::is_absolute()` NO detecta al faltarles letra de unidad.
        if norm.starts_with('/')
            || Path::new(&norm).is_absolute()
            || norm.split('/').any(|c| c == "..")
        {
            return PolicyDecision::Escape(norm);
        }
        // Prohibido gana sobre todo (defensa: `.env`, secretos, `.git`).
        if self.forbidden.is_match(&norm) {
            return PolicyDecision::Forbidden(norm);
        }
        if !self.owned.is_match(&norm) {
            return PolicyDecision::OutsideOwned(norm);
        }
        PolicyDecision::Allowed
    }

    /// Valida un cambio completo. En renombrados/copias hay DOS rutas (origen y
    /// destino) y AMBAS deben cumplir la política: un agente no puede mover un
    /// archivo autorizado a una ruta prohibida ni traerlo desde fuera de `owned`.
    /// Devuelve la primera decisión NO permitida (destino primero), o `Allowed`.
    pub fn check_change(&self, path: &str, orig: Option<&str>) -> PolicyDecision {
        match self.check(path) {
            PolicyDecision::Allowed => {}
            d => return d,
        }
        if let Some(o) = orig {
            match self.check(o) {
                PolicyDecision::Allowed => {}
                d => return d,
            }
        }
        PolicyDecision::Allowed
    }
}

fn build_set(patterns: &[String]) -> Result<GlobSet, String> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        let g = Glob::new(&p.replace('\\', "/"))
            .map_err(|e| format!("glob inválido '{p}': {e}"))?;
        b.add(g);
    }
    b.build().map_err(|e| e.to_string())
}

/// Componentes literales al inicio de un glob (hasta el primero con comodín).
/// `src/auth/**` → ["src","auth"]; `src/routes/auth.ts` → ["src","routes","auth.ts"];
/// `**` → []. Sirve para detectar solapamiento entre `ownedPaths` de dos tareas.
fn literal_prefix(glob: &str) -> Vec<String> {
    let mut out = Vec::new();
    for comp in glob.replace('\\', "/").split('/') {
        if comp.is_empty() {
            continue;
        }
        if comp.contains(['*', '?', '[', ']', '{', '}']) {
            break;
        }
        out.push(comp.to_string());
    }
    out
}

fn is_prefix_of(a: &[String], b: &[String]) -> bool {
    a.len() <= b.len() && a.iter().zip(b).all(|(x, y)| x == y)
}

/// ¿Dos conjuntos de `ownedPaths` se solapan? Conservador: dos globs solapan si
/// el prefijo literal de uno es prefijo (por componentes) del otro. Así
/// `src/auth/**` y `src/auth/components/**` se marcan como solapados (uno anida
/// en el otro), pero `src/auth/**` y `src/ui/**` no. Un `**` (prefijo vacío)
/// solapa con todo, que es lo correcto (reclama el worktree entero).
pub fn owned_paths_overlap(a: &[String], b: &[String]) -> bool {
    for ga in a {
        let pa = literal_prefix(ga);
        for gb in b {
            let pb = literal_prefix(gb);
            if is_prefix_of(&pa, &pb) || is_prefix_of(&pb, &pa) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn forbidden_wins_even_inside_owned() {
        let p = PathPolicy::new(&s(&["**"]), &s(&[".env", ".git/**"])).unwrap();
        assert_eq!(p.check(".env"), PolicyDecision::Forbidden(".env".into()));
        assert_eq!(p.check(".git/config"), PolicyDecision::Forbidden(".git/config".into()));
    }

    #[test]
    fn allows_inside_owned_rejects_outside() {
        let p = PathPolicy::new(&s(&["src/auth/**"]), &s(&[".env"])).unwrap();
        assert_eq!(p.check("src/auth/service.ts"), PolicyDecision::Allowed);
        // acepta separadores de Windows
        assert_eq!(p.check("src\\auth\\login.ts"), PolicyDecision::Allowed);
        assert_eq!(
            p.check("src/frontend/App.tsx"),
            PolicyDecision::OutsideOwned("src/frontend/App.tsx".into())
        );
    }

    #[test]
    fn empty_owned_is_invalid() {
        // fail-closed: sin owned no se crea la política (evita acceso total accidental)
        assert!(PathPolicy::new(&[], &s(&[".env"])).is_err());
        // acceso total debe declararse explícito
        assert!(PathPolicy::new(&s(&["**"]), &s(&[".env"])).is_ok());
    }

    #[test]
    fn blocks_git_file_and_dir() {
        // en un worktree `.git` es un ARCHIVO: deben bloquearse `.git` y `.git/**`
        let p = PathPolicy::new(&s(&["**"]), &default_forbidden()).unwrap();
        assert!(matches!(p.check(".git"), PolicyDecision::Forbidden(_)));
        assert!(matches!(p.check(".git/config"), PolicyDecision::Forbidden(_)));
        assert!(matches!(p.check(".gitmodules"), PolicyDecision::Forbidden(_)));
        assert!(matches!(p.check("src/.env"), PolicyDecision::Forbidden(_)));
    }

    #[test]
    fn rename_validates_both_endpoints() {
        let p = PathPolicy::new(&s(&["src/**"]), &default_forbidden()).unwrap();
        // rename permitido → permitido
        assert_eq!(p.check_change("src/b.rs", Some("src/a.rs")), PolicyDecision::Allowed);
        // rename permitido → prohibido (destino .env): rechazado
        assert!(matches!(
            p.check_change("src/.env", Some("src/a.rs")),
            PolicyDecision::Forbidden(_)
        ));
        // rename desde fuera de owned (origen no permitido): rechazado
        assert!(matches!(
            p.check_change("src/b.rs", Some("otro/a.rs")),
            PolicyDecision::OutsideOwned(_)
        ));
    }

    #[test]
    fn rejects_escape_attempts() {
        let p = PathPolicy::new(&s(&["**"]), &[]).unwrap();
        assert!(matches!(p.check("../otro/secreto"), PolicyDecision::Escape(_)));
        assert!(matches!(p.check("src/../../x"), PolicyDecision::Escape(_)));
        assert!(matches!(p.check("C:/Windows/System32/x"), PolicyDecision::Escape(_)));
        assert!(matches!(p.check("/etc/passwd"), PolicyDecision::Escape(_)));
    }

    #[test]
    fn overlap_detection() {
        // Anidados: solapan.
        assert!(owned_paths_overlap(&s(&["src/auth/**"]), &s(&["src/auth/components/**"])));
        // Disjuntos: no solapan.
        assert!(!owned_paths_overlap(&s(&["src/auth/**"]), &s(&["src/ui/**"])));
        // `**` reclama todo → solapa con cualquier cosa.
        assert!(owned_paths_overlap(&s(&["**"]), &s(&["src/backend/**"])));
        // Mismo archivo exacto.
        assert!(owned_paths_overlap(&s(&["src/routes/auth.ts"]), &s(&["src/routes/**"])));
        // Archivos hermanos distintos: no solapan.
        assert!(!owned_paths_overlap(&s(&["src/auth.ts"]), &s(&["src/auth/**"])));
    }
}
