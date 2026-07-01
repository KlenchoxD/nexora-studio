//! Envoltura fina sobre el CLI oficial de `git` (no git2): los worktrees y el
//! respeto por la config del usuario son más predecibles con el binario real.
//!
//! Regla del Hito 1: el ORQUESTADOR controla git. El agente edita archivos y
//! corre pruebas; NUNCA commitea, cambia de rama, hace merge/rebase/push ni
//! toca worktrees. Todo eso vive aquí y lo invoca el orquestador tras validar.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Ruta absoluta y confiable a git, resuelta UNA vez desde PATH (control #1).
/// Nunca se invoca "git" por nombre relativo desde un cwd que controla el
/// agente: se usa siempre este binario absoluto. Si la resolución falla (git no
/// en PATH), degradamos a "git" para no romper el arranque; las rutas de
/// orquestación exigen aparte que la resolución haya sido exitosa.
fn git_program() -> &'static Path {
    static GIT: OnceLock<PathBuf> = OnceLock::new();
    GIT.get_or_init(|| crate::trusted_exec::resolve("git").unwrap_or_else(|_| PathBuf::from("git")))
}

/// Ruta absoluta de git si se pudo resolver de forma confiable (para que la
/// orquestación pueda exigirla antes de ejecutar nada en un worktree).
pub fn trusted_git_path() -> Result<PathBuf, String> {
    crate::trusted_exec::resolve("git")
}

/// Directorio de hooks VACÍO administrado por Nexora. Se pasa como
/// `core.hooksPath` en el commit del orquestador para que git NO ejecute hooks
/// del repo (`pre-commit`, etc.) que podrían correr código del proyecto.
fn empty_hooks_dir() -> PathBuf {
    let d = std::env::temp_dir().join("nexora-empty-hooks");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn git_cmd(cwd: &Path) -> Command {
    let mut c = Command::new(git_program());
    c.current_dir(cwd);
    // CREATE_NO_WINDOW: sin ventana de consola en Windows.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}

fn run(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let out = git_cmd(cwd)
        .args(args)
        .output()
        .map_err(|e| format!("git no disponible: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Raíz real del repositorio que contiene `dir`. Error si no es un repo git.
pub fn repo_root(dir: &Path) -> Result<PathBuf, String> {
    if run(dir, &["rev-parse", "--is-inside-work-tree"])?.trim() != "true" {
        return Err("la carpeta no es un repositorio git".into());
    }
    Ok(PathBuf::from(run(dir, &["rev-parse", "--show-toplevel"])?.trim()))
}

/// Commit actual (HEAD). Se captura UNA vez por ejecución: todas las ramas del
/// run parten de él, no se vuelve a consultar por agente (HEAD podría moverse).
pub fn head_commit(repo: &Path) -> Result<String, String> {
    Ok(run(repo, &["rev-parse", "HEAD"])?.trim().to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChangedFile {
    /// Ruta relativa a la raíz del repo/worktree, con separadores `/`.
    pub path: String,
    /// Código XY de porcelain (`M`, `A`, `D`, `R`, `??`, …), sin espacios.
    pub status: String,
    /// Ruta original en renombrados/copias.
    pub orig: Option<String>,
}

/// Archivos con cambios en el árbol de trabajo (vacío = limpio). Usa
/// `--porcelain=v1 -z`, que SÍ incluye archivos no rastreados (nuevos),
/// a diferencia de `git diff --name-only`.
pub fn changed_files(repo: &Path) -> Result<Vec<ChangedFile>, String> {
    Ok(parse_porcelain_z(&run(repo, &["status", "--porcelain=v1", "-z"])?))
}

/// ¿El árbol de trabajo está limpio? (para bloquear runs sobre repos sucios).
pub fn is_clean(repo: &Path) -> Result<bool, String> {
    Ok(changed_files(repo)?.is_empty())
}

/// ¿Es `name` un nombre de rama válido para git? Usa `check-ref-format`, que no
/// requiere estar dentro de un repo. Evita caracteres inválidos y entradas
/// manipuladas (ej. `..`, `~`, `^`, espacios, opciones tipo `-x`).
pub fn branch_name_valid(name: &str) -> bool {
    git_cmd(&std::env::temp_dir())
        .args(["check-ref-format", "--branch", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Sanea un componente (runId/taskId/agente) para usarlo en un nombre de rama:
/// solo `[A-Za-z0-9._-]`, sin `..`, sin `-`/`.` en los extremos, nunca vacío.
pub fn sanitize_ref_component(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' })
        .collect();
    while out.contains("..") {
        out = out.replace("..", ".");
    }
    let out = out.trim_matches(|c| c == '-' || c == '.').to_string();
    if out.is_empty() { "x".into() } else { out }
}

/// Construye un nombre de rama seguro para el run/agente/tarea.
pub fn build_branch(run_id: &str, agent: &str, task_id: &str) -> String {
    format!(
        "nexora/run-{}/{}-{}",
        sanitize_ref_component(run_id),
        sanitize_ref_component(agent),
        sanitize_ref_component(task_id)
    )
}

/// Crea un worktree aislado en `path` con una rama nueva `branch` desde `base`.
/// Rechaza nombres de rama inválidos ANTES de invocar a git.
pub fn add_worktree(repo: &Path, path: &Path, branch: &str, base: &str) -> Result<(), String> {
    if !branch_name_valid(branch) {
        return Err(format!("nombre de rama inválido: '{branch}'"));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("no se pudo crear {parent:?}: {e}"))?;
    }
    run(
        repo,
        &["worktree", "add", "-b", branch, &path.to_string_lossy(), base],
    )?;
    Ok(())
}

/// Elimina un worktree (forzado) y limpia referencias colgadas.
pub fn remove_worktree(repo: &Path, path: &Path) -> Result<(), String> {
    run(repo, &["worktree", "remove", "--force", &path.to_string_lossy()])?;
    let _ = run(repo, &["worktree", "prune"]);
    Ok(())
}

/// Rutas presentes en el índice (staged), vía `diff --cached --name-status -z`.
/// En renombrados devuelve AMBAS rutas (origen y destino).
fn staged_paths(worktree: &Path) -> Result<Vec<String>, String> {
    let out = run(worktree, &["diff", "--cached", "--name-status", "-z"])?;
    let toks: Vec<&str> = out.split('\0').filter(|t| !t.is_empty()).collect();
    let mut paths = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let status = toks[i];
        i += 1;
        let extra = if status.starts_with('R') || status.starts_with('C') { 2 } else { 1 };
        for _ in 0..extra {
            if i < toks.len() {
                paths.push(toks[i].replace('\\', "/"));
                i += 1;
            }
        }
    }
    Ok(paths)
}

/// Commit creado POR EL ORQUESTADOR (nunca por el agente), solo con rutas ya
/// validadas contra la política. Endurecido contra un índice manipulado por el
/// agente:
///   1. HEAD debe seguir en `base_commit` (el agente NO pudo crear commits).
///   2. Se limpia el índice (`reset --mixed`) sin tocar el working tree, así se
///      descarta cualquier `git add` que el agente haya hecho (ej. `.env`).
///   3. Se stagean SOLO las rutas autorizadas.
///   4. El staged diff debe coincidir EXACTO con lo autorizado; si sobra o
///      falta algo, se aborta sin commitear.
/// Identidad propia vía `-c` (no altera config global) + trailers de trazabilidad.
pub fn commit_authorized(
    worktree: &Path,
    base_commit: &str,
    authorized: &[String],
    message: &str,
    trailers: &[(&str, &str)],
) -> Result<String, String> {
    if authorized.is_empty() {
        return Err("no hay archivos autorizados para commitear".into());
    }
    // 1. El agente no debe haber movido HEAD (commits propios).
    let head = head_commit(worktree)?;
    if head != base_commit {
        return Err(format!(
            "HEAD ({head}) != baseCommit ({base_commit}): el agente creó commits; se bloquea"
        ));
    }
    // 2. Limpiar el índice sin tocar el working tree (descarta staging del agente).
    run(worktree, &["reset", "-q", "--mixed", "HEAD"])?;
    // 3. Stagear solo lo autorizado.
    let mut add = vec!["add", "--"];
    add.extend(authorized.iter().map(|s| s.as_str()));
    run(worktree, &add)?;
    // 4. El índice debe coincidir EXACTO con el conjunto autorizado.
    let staged: BTreeSet<String> = staged_paths(worktree)?.into_iter().collect();
    let want: BTreeSet<String> = authorized.iter().map(|s| s.replace('\\', "/")).collect();
    if staged != want {
        let _ = run(worktree, &["reset", "-q", "--mixed", "HEAD"]);
        return Err(format!(
            "el índice no coincide con lo autorizado (autorizado={want:?}, staged={staged:?}); no se commitea"
        ));
    }

    let mut msg = message.to_string();
    if !trailers.is_empty() {
        msg.push_str("\n\n");
        for (k, v) in trailers {
            msg.push_str(&format!("{k}: {v}\n"));
        }
    }
    // Commit endurecido (control #2): sin hooks del repo (core.hooksPath a un
    // directorio vacío), sin firma GPG, e identidad propia sin tocar config global.
    let hooks = empty_hooks_dir();
    let hooks_arg = format!("core.hooksPath={}", hooks.to_string_lossy());
    run(
        worktree,
        &[
            "-c",
            &hooks_arg,
            "-c",
            "commit.gpgSign=false",
            "-c",
            "user.name=Nexora Studio",
            "-c",
            "user.email=nexora@local",
            "commit",
            "--no-verify",
            "-m",
            &msg,
        ],
    )?;
    head_commit(worktree)
}

/// Parsea la salida de `git status --porcelain=v1 -z`. Los registros van
/// separados por NUL; en renombrados/copias (`R`/`C`) el registro trae un token
/// extra con la ruta original.
fn parse_porcelain_z(s: &str) -> Vec<ChangedFile> {
    let toks: Vec<&str> = s.split('\0').filter(|t| !t.is_empty()).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let t = toks[i];
        if t.len() < 4 {
            i += 1;
            continue;
        }
        let xy = &t[0..2];
        let path = t[3..].replace('\\', "/"); // formato "XY PATH"
        let is_rename = xy.starts_with('R') || xy.starts_with('C');
        let orig = if is_rename && i + 1 < toks.len() {
            i += 1;
            Some(toks[i].replace('\\', "/"))
        } else {
            None
        };
        out.push(ChangedFile {
            path,
            status: xy.trim().to_string(),
            orig,
        });
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn porcelain_parses_new_modified_and_rename() {
        // ?? nuevo, M modificado, R renombrado (con ruta original extra).
        let raw = "?? nuevo.txt\0 M src/app.rs\0R  viejo.txt\0nuevo_nombre.txt\0";
        // Nota: el registro de rename real es "R  NEW\0OLD"; ajustamos el orden.
        let raw = raw.replace("R  viejo.txt\0nuevo_nombre.txt", "R  nuevo_nombre.txt\0viejo.txt");
        let files = parse_porcelain_z(&raw);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0], ChangedFile { path: "nuevo.txt".into(), status: "??".into(), orig: None });
        assert_eq!(files[1].path, "src/app.rs");
        assert_eq!(files[1].status, "M");
        assert_eq!(files[2].path, "nuevo_nombre.txt");
        assert_eq!(files[2].orig.as_deref(), Some("viejo.txt"));
    }

    #[test]
    fn branch_name_sanitize_and_validate() {
        assert_eq!(sanitize_ref_component("task 001!"), "task-001");
        assert_eq!(sanitize_ref_component("../evil"), "evil");
        assert_eq!(sanitize_ref_component("a..b"), "a.b");
        assert_eq!(sanitize_ref_component(""), "x");
        // build_branch produce un nombre válido aun con entradas sucias
        let b = build_branch("run 1", "codex", "TASK/../x");
        assert!(branch_name_valid(&b), "rama construida debe ser válida: {b}");
        // nombres crudos manipulados se rechazan
        assert!(!branch_name_valid("mala rama con espacios"));
        assert!(!branch_name_valid("bad~ref^name"));
        assert!(!branch_name_valid("dir/../escape"));
    }

    // --- Integración con git real. Se salta SOLO si git no está instalado. Si
    //     git existe pero el setup falla, el test FALLA (no return silencioso).
    fn git_available() -> bool {
        Command::new("git").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
    }

    /// Crea un repo temporal con un commit inicial. Falla ruidosamente si algo
    /// del setup no funciona (para no ocultar errores reales del entorno).
    fn tmp_repo(name_with_spaces: bool) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "{}{}",
            if name_with_spaces { "nexora repo " } else { "nexora_repo_" },
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&base).expect("crear dir temporal");
        run(&base, &["init", "-q", "-b", "main"]).expect("git init");
        run(&base, &["config", "user.name", "T"]).expect("config name");
        run(&base, &["config", "user.email", "t@t"]).expect("config email");
        fs::write(base.join("README.md"), "hola").expect("escribir README");
        run(&base, &["add", "-A"]).expect("git add");
        run(&base, &["commit", "-q", "-m", "init"]).expect("git commit");
        base
    }

    #[test]
    fn clean_dirty_worktree_and_orchestrator_commit() {
        if !git_available() {
            eprintln!("git no instalado: se salta test de integración");
            return;
        }
        let repo = tmp_repo(false);
        let root = repo_root(&repo).unwrap();
        assert_eq!(root.canonicalize().unwrap(), repo.canonicalize().unwrap());
        assert!(is_clean(&repo).unwrap());
        let base = head_commit(&repo).unwrap();

        // repo sucio: un archivo nuevo (NO rastreado) debe aparecer.
        fs::write(repo.join("nuevo.rs"), "fn main(){}").unwrap();
        let changed = changed_files(&repo).unwrap();
        assert!(changed.iter().any(|c| c.path == "nuevo.rs" && c.status == "??"));
        assert!(!is_clean(&repo).unwrap());

        // worktree aislado desde el commit base
        let wt = std::env::temp_dir().join(format!("nexora_wt_{}", uuid::Uuid::new_v4()));
        add_worktree(&root, &wt, "nexora/test/task-1", &base).unwrap();
        assert!(wt.join("README.md").exists());
        assert!(is_clean(&wt).unwrap()); // limpio pese a que el principal esté sucio

        // el orquestador commitea SOLO el archivo autorizado
        fs::write(wt.join("feature.rs"), "// impl").unwrap();
        let hash = commit_authorized(
            &wt,
            &base,
            &["feature.rs".into()],
            "nexora(task-1): add feature",
            &[("Nexora-Agent", "codex")],
        )
        .unwrap();
        assert_eq!(hash.len(), 40);
        assert_ne!(hash, base);
        assert!(is_clean(&wt).unwrap());

        remove_worktree(&root, &wt).ok();
        fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn prestaged_forbidden_file_is_not_committed() {
        if !git_available() {
            return;
        }
        let repo = tmp_repo(false);
        let base = head_commit(&repo).unwrap();
        let wt = std::env::temp_dir().join(format!("nexora_wt_{}", uuid::Uuid::new_v4()));
        add_worktree(&repo_root(&repo).unwrap(), &wt, "nexora/test/prestaged", &base).unwrap();

        // El agente deja un secreto STAGED por su cuenta...
        fs::write(wt.join("secreto.env"), "TOKEN=abc").unwrap();
        run(&wt, &["add", "secreto.env"]).unwrap();
        // ...y también cambia el archivo realmente autorizado.
        fs::write(wt.join("feature.rs"), "// impl").unwrap();

        // El orquestador commitea SOLO feature.rs; el secreto pre-staged NO entra.
        commit_authorized(&wt, &base, &["feature.rs".into()], "add feature", &[]).unwrap();
        let committed = run(&wt, &["show", "--name-only", "--format=", "HEAD"]).unwrap();
        assert!(committed.contains("feature.rs"));
        assert!(!committed.contains("secreto.env"), "el secreto pre-staged se coló al commit");

        remove_worktree(&repo_root(&repo).unwrap(), &wt).ok();
        fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn agent_created_commit_is_blocked() {
        if !git_available() {
            return;
        }
        let repo = tmp_repo(false);
        let base = head_commit(&repo).unwrap();
        let wt = std::env::temp_dir().join(format!("nexora_wt_{}", uuid::Uuid::new_v4()));
        add_worktree(&repo_root(&repo).unwrap(), &wt, "nexora/test/agentcommit", &base).unwrap();

        // El agente crea un commit por su cuenta → HEAD deja de ser baseCommit.
        fs::write(wt.join("x.rs"), "// agente").unwrap();
        run(&wt, &["-c", "user.name=A", "-c", "user.email=a@a", "add", "-A"]).unwrap();
        run(&wt, &["-c", "user.name=A", "-c", "user.email=a@a", "commit", "-q", "-m", "agente"]).unwrap();

        let err = commit_authorized(&wt, &base, &["x.rs".into()], "msg", &[]).unwrap_err();
        assert!(err.contains("HEAD"), "debe detectar HEAD != baseCommit: {err}");

        remove_worktree(&repo_root(&repo).unwrap(), &wt).ok();
        fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn staged_diff_mismatch_blocks_commit() {
        if !git_available() {
            return;
        }
        let repo = tmp_repo(false);
        let base = head_commit(&repo).unwrap();
        let wt = std::env::temp_dir().join(format!("nexora_wt_{}", uuid::Uuid::new_v4()));
        add_worktree(&repo_root(&repo).unwrap(), &wt, "nexora/test/mismatch", &base).unwrap();

        // Se "autoriza" un DIRECTORIO ("sub"), pero contiene dos archivos: al
        // stagear, el índice queda con sub/a.rs y sub/b.rs → más de lo declarado
        // (want={"sub"}) → mismatch → no se commitea.
        fs::create_dir(wt.join("sub")).unwrap();
        fs::write(wt.join("sub/a.rs"), "// a").unwrap();
        fs::write(wt.join("sub/b.rs"), "// b").unwrap();
        let err = commit_authorized(&wt, &base, &["sub".into()], "msg", &[]).unwrap_err();
        assert!(err.contains("no coincide"), "debe rechazar por mismatch: {err}");
        // y no debe haber creado commit
        assert_eq!(head_commit(&wt).unwrap(), base);

        remove_worktree(&repo_root(&repo).unwrap(), &wt).ok();
        fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn canary_fake_git_in_worktree_is_not_used() {
        if !git_available() {
            return;
        }
        let repo = tmp_repo(false);
        let base = head_commit(&repo).unwrap();
        let wt = std::env::temp_dir().join(format!("nexora_wt_{}", uuid::Uuid::new_v4()));
        add_worktree(&repo_root(&repo).unwrap(), &wt, "nexora/test/fakegit", &base).unwrap();

        // El agente deja un "git" falso dentro del worktree. Nexora debe usar el
        // git ABSOLUTO resuelto desde PATH, que NUNCA está dentro del worktree.
        fs::write(wt.join("git"), "#!/bin/sh\necho HACKED").unwrap();
        fs::write(wt.join("git.exe"), "falso").unwrap();
        let trusted = trusted_git_path().expect("git debe resolverse en dev");
        assert!(trusted.is_absolute());
        assert!(
            matches!(crate::trusted_exec::is_inside(&trusted, &wt), Ok(false)),
            "el git de confianza no debe estar dentro del worktree: {trusted:?}"
        );

        // y las operaciones siguen funcionando con el git legítimo
        fs::write(wt.join("feature.rs"), "// impl").unwrap();
        commit_authorized(&wt, &base, &["feature.rs".into()], "add", &[]).unwrap();

        remove_worktree(&repo_root(&repo).unwrap(), &wt).ok();
        fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn canary_malicious_precommit_hook_is_not_run() {
        if !git_available() {
            return;
        }
        let repo = tmp_repo(false);
        // Hook pre-commit que crea un centinela y ABORTA el commit (exit 1). Los
        // worktrees comparten los hooks del repo común, así que afectaría a un
        // commit normal desde el worktree.
        let hooks = repo.join(".git").join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        let sentinel = repo.join("HOOK_RAN.txt");
        let hook = hooks.join("pre-commit");
        fs::write(&hook, format!("#!/bin/sh\ntouch \"{}\"\nexit 1\n", sentinel.to_string_lossy())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let base = head_commit(&repo).unwrap();
        let wt = std::env::temp_dir().join(format!("nexora_wt_{}", uuid::Uuid::new_v4()));
        add_worktree(&repo_root(&repo).unwrap(), &wt, "nexora/test/hook", &base).unwrap();
        fs::write(wt.join("feature.rs"), "// impl").unwrap();

        // El commit del orquestador desactiva hooks → debe crear el commit y el
        // hook NO debe ejecutarse (sin centinela).
        let hash = commit_authorized(&wt, &base, &["feature.rs".into()], "add", &[]).unwrap();
        assert_eq!(hash.len(), 40);
        assert!(!sentinel.exists(), "el hook pre-commit se ejecutó pese a estar desactivado");

        remove_worktree(&repo_root(&repo).unwrap(), &wt).ok();
        fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn works_with_spaces_in_path() {
        if !git_available() {
            return;
        }
        let repo = tmp_repo(true);
        assert!(repo.to_string_lossy().contains(' '));
        assert!(is_clean(&repo).unwrap());
        let base = head_commit(&repo).unwrap();
        let wt = std::env::temp_dir().join(format!("nexora wt {}", uuid::Uuid::new_v4()));
        add_worktree(&repo_root(&repo).unwrap(), &wt, "nexora/test/spaces", &base).unwrap();
        assert!(wt.join("README.md").exists());
        remove_worktree(&repo_root(&repo).unwrap(), &wt).ok();
        fs::remove_dir_all(&repo).ok();
    }
}
