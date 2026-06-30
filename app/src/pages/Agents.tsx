import type { CSSProperties } from 'react';
import '../styles/agents.css';

type Feed = [string, string, string];
const agents = [
  {
    name: 'Codex', mark: 'C', c: 'var(--codex)', caps: ['backend', 'arquitectura', 'tests', 'refactor'],
    task: 'Creando API de autenticación', pct: 65, wt: '.nexora/wt-codex', tok: '18.2k',
    files: [['auth.ts', 'M'], ['users.ts', 'A'], ['middleware.ts', 'M']] as [string, string][],
    feed: [['tool', 'apply_patch · auth.ts (+24 −2)', 'ok'], ['tool', 'shell · npm run typecheck', 'ok'], ['msg', 'Definiendo contrato: POST /login, POST /logout', ''], ['tool', 'apply_patch · middleware.ts', 'run']] as Feed[],
  },
  {
    name: 'Claude', mark: 'A', c: 'var(--claude)', caps: ['frontend', 'UI', 'debugging', 'docs', 'review'],
    task: 'Construyendo interfaz de Login', pct: 42, wt: '.nexora/wt-claude', tok: '11.7k',
    files: [['Login.tsx', 'A'], ['Button.tsx', 'M'], ['styles.css', 'M']] as [string, string][],
    feed: [['tool', 'write · Login.tsx', 'ok'], ['msg', 'Esperando contrato de API…', 'wait'], ['msg', 'Contrato recibido · usando POST /login', ''], ['tool', 'edit · Button.tsx', 'run']] as Feed[],
  },
];

export default function Agents() {
  return (
    <div className="v-agents">
      <header className="ph">
        <div>
          <h1>Agentes</h1>
          <p className="sub">Quién hace qué, en tiempo real. Tú apruebas las integraciones.</p>
        </div>
        <div className="acts"><button className="btn">Capacidades</button><button className="btn primary">Asignar tarea</button></div>
      </header>

      <div className="review">
        <div className="rv-l">
          <span className="label">Revisión pendiente</span>
          <p>Codex terminó la API y se la pasó a Claude. Aprueba el merge a <code>feat/google-oauth</code> para integrar.</p>
        </div>
        <div className="acts"><button className="btn">Ver diff</button><button className="btn primary">Aprobar merge</button></div>
      </div>

      <div className="grid">
        {agents.map((a) => (
          <article className="panel" key={a.name} style={{ ['--c']: a.c } as CSSProperties}>
            <header className="pa-head">
              <span className="pa-mark">{a.mark}</span>
              <div className="pa-id">
                <div className="pa-name">{a.name}</div>
                <div className="caps">{a.caps.map((c) => <span className="cap" key={c}>{c}</span>)}</div>
              </div>
              <span className="state"><span className="dot run" /> Trabajando</span>
            </header>

            <div className="task">
              <div className="tk-top"><span className="label">Tarea actual</span><span className="pct num">{a.pct}%</span></div>
              <div className="tk-val">{a.task}</div>
              <div className="track"><span style={{ width: `${a.pct}%`, background: 'var(--c)' }} /></div>
              <div className="wt mono">{a.wt}</div>
            </div>

            <div className="split">
              <div>
                <div className="label sl">Archivos · git status</div>
                {a.files.map(([f, s]) => (
                  <div className="frow" key={f}><span className="fn mono">{f}</span><span className={`gs ${s === 'A' ? 'add' : 'mod'}`}>{s}</span></div>
                ))}
              </div>
              <div className="feed">
                <div className="label sl">Actividad</div>
                {a.feed.map(([k, t, s], i) => (
                  <div className="erow" key={i}>
                    <span className={`ek ${s}`}>{k === 'tool' ? '›' : '·'}</span>
                    <span className="et mono">{t}</span>
                    {s === 'ok' && <span className="es ok">✓</span>}
                    {s === 'run' && <span className="es">···</span>}
                    {s === 'wait' && <span className="es wt2">espera</span>}
                  </div>
                ))}
              </div>
            </div>

            <footer className="pf">
              <button className="btn sm">Pausar</button>
              <button className="btn sm">Cancelar</button>
              <span className="flex" />
              <span className="tok mono num">{a.tok} tok</span>
            </footer>
          </article>
        ))}
      </div>
    </div>
  );
}
