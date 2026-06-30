import { getCurrentWindow } from "@tauri-apps/api/window";
import Icon from "./Icon";

const win = getCurrentWindow();

type Props = {
  active: string;
  onNav: (id: string) => void;
  children: React.ReactNode;
};

const nav = [{ id: "conversation", icon: "chat", label: "Conversación" }];

export default function Shell({ active, onNav, children }: Props) {
  return (
    <div className="app">
      <header className="titlebar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region><span className="mark">N</span> Nexora Studio</div>
        <div className="spacer" data-tauri-drag-region />
        <div className="wctl">
          <button className="wbtn" onClick={() => win.minimize()} aria-label="Minimizar"><Icon name="minus" cls="icon sm" /></button>
          <button className="wbtn" onClick={() => win.toggleMaximize()} aria-label="Maximizar"><Icon name="square" cls="icon sm" /></button>
          <button className="wbtn close" onClick={() => win.close()} aria-label="Cerrar"><Icon name="x" cls="icon sm" /></button>
        </div>
      </header>

      <div className="shell-body">
        <nav className="rail">
          {nav.map((n) => (
            <button key={n.id} className={n.id === active ? "active" : ""} title={n.label} onClick={() => onNav(n.id)}>
              <Icon name={n.icon} />
            </button>
          ))}
          <span className="grow" />
          <button title="Ajustes"><Icon name="settings" /></button>
        </nav>
        <main className="main">{children}</main>
      </div>

      <footer className="statusbar">
        <span className="s">Nexora Studio · orquestador local</span>
        <span className="spacer" />
        <span className="s">Codex CLI · Claude Code</span>
      </footer>
    </div>
  );
}
