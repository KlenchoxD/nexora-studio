import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

// Banner de auto-update: al iniciar consulta el endpoint de GitHub Releases.
// Si hay versión nueva (firmada), la descarga, instala y reinicia la app.
export default function UpdateBanner() {
  const [upd, setUpd] = useState<Update | null>(null);
  const [busy, setBusy] = useState(false);
  const [pct, setPct] = useState(0);
  const [err, setErr] = useState("");

  useEffect(() => {
    check().then((u) => { if (u) setUpd(u); }).catch(() => { /* sin red / sin release: silencio */ });
  }, []);

  if (!upd) return null;

  const install = async () => {
    setBusy(true); setErr("");
    try {
      let total = 0; let got = 0;
      await upd.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        else if (ev.event === "Progress") { got += ev.data.chunkLength; if (total) setPct(Math.round((got / total) * 100)); }
      });
      await relaunch();
    } catch (e) {
      setErr(String(e)); setBusy(false);
    }
  };

  return (
    <div className="update-banner">
      <span className="ub-dot" />
      <span>Nueva versión <b>{upd.version}</b> disponible</span>
      {err && <span className="ub-err">{err}</span>}
      <button className="btn sm primary" onClick={install} disabled={busy}>
        {busy ? (pct ? `Descargando ${pct}%` : "Instalando…") : "Actualizar ahora"}
      </button>
      {!busy && <button className="ub-x" onClick={() => setUpd(null)} aria-label="Descartar">✕</button>}
    </div>
  );
}
