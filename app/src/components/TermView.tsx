import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// Terminal integrado (PTY real en el backend, xterm en el front).
export default function TermView({ cwd }: { cwd: string }) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const term = new Terminal({
      fontFamily: "JetBrains Mono, ui-monospace, monospace",
      fontSize: 12.5,
      cursorBlink: true,
      theme: {
        background: "#0A0A0B", foreground: "#ECECEE", cursor: "#ECECEE",
        black: "#0A0A0B", brightBlack: "#5E5E66",
      },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host.current!);
    fit.fit();

    const sync = () => {
      try { fit.fit(); } catch { /* noop */ }
      invoke("term_resize", { rows: term.rows, cols: term.cols }).catch(() => {});
    };

    invoke("term_open", { cwd: cwd || null })
      .then(() => sync())
      .catch(() => {});

    const unP = listen<string>("term-output", (e) => term.write(e.payload));
    const onData = term.onData((d) => { invoke("term_write", { data: d }).catch(() => {}); });
    window.addEventListener("resize", sync);
    const t = setTimeout(sync, 60);

    return () => {
      clearTimeout(t);
      window.removeEventListener("resize", sync);
      onData.dispose();
      unP.then((f) => f());
      term.dispose();
    };
  }, [cwd]);

  return <div className="term-host" ref={host} />;
}
