//! Terminal integrado (estilo VS Code): un PTY real dentro de la app.
//! El backend abre un shell con pseudo-terminal y transmite su salida por eventos;
//! el frontend (xterm.js) lo pinta y manda las teclas de vuelta.

use crate::AppState;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use tauri::{AppHandle, Emitter, State};

pub struct TermSession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
}

/// Abre el terminal (una sesión global). Si ya hay una, no hace nada.
#[tauri::command]
pub fn term_open(app: AppHandle, state: State<AppState>, cwd: Option<String>) -> Result<(), String> {
    let mut guard = state.term.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Ok(());
    }
    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;

    #[cfg(windows)]
    let mut cmd = CommandBuilder::new("powershell.exe");
    #[cfg(not(windows))]
    let mut cmd = CommandBuilder::new("bash");
    if let Some(dir) = cwd {
        if std::path::Path::new(&dir).is_dir() {
            cmd.cwd(dir);
        }
    }

    let _child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    // Hilo lector: emite la salida del shell al frontend.
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let _ = app.emit("term-output", chunk);
                }
                Err(_) => break,
            }
        }
        let _ = app.emit("term-exit", ());
    });

    *guard = Some(TermSession { master: pair.master, writer });
    Ok(())
}

/// Manda teclas/entrada del usuario al shell.
#[tauri::command]
pub fn term_write(state: State<AppState>, data: String) -> Result<(), String> {
    let mut guard = state.term.lock().map_err(|e| e.to_string())?;
    if let Some(s) = guard.as_mut() {
        s.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        s.writer.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Ajusta el tamaño del PTY al del xterm.
#[tauri::command]
pub fn term_resize(state: State<AppState>, rows: u16, cols: u16) -> Result<(), String> {
    let guard = state.term.lock().map_err(|e| e.to_string())?;
    if let Some(s) = guard.as_ref() {
        s.master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
