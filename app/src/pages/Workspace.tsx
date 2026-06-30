import Icon from '../components/Icon';
import '../styles/workspace.css';

const tree = [
  { d: 0, t: 'folder', n: 'src' },
  { d: 1, t: 'folder', n: 'auth' },
  { d: 2, t: 'file', n: 'auth.ts', st: 'M', who: 'cx' },
  { d: 2, t: 'file', n: 'middleware.ts', st: 'M', who: 'cx' },
  { d: 2, t: 'file', n: 'users.ts', st: 'A', who: 'cx' },
  { d: 1, t: 'folder', n: 'components' },
  { d: 2, t: 'file', n: 'Login.tsx', st: 'A', who: 'cl' },
  { d: 2, t: 'file', n: 'Button.tsx', st: 'M', who: 'cl' },
  { d: 1, t: 'file', n: 'styles.css', st: 'M', who: 'cl' },
  { d: 0, t: 'file', n: 'package.json' },
  { d: 0, t: 'file', n: 'AGENTS.md' },
];

export default function Workspace() {
  return (
    <div className="v-ws">
      <aside className="pane explorer">
        <div className="search"><Icon name="search" cls="icon sm" /><input placeholder="Buscar archivos" /></div>
        <div className="px"><span className="label">Explorador</span><span className="tag">git status</span></div>
        <div className="tree">
          {tree.map((it, i) => (
            <div className="trow" key={i} style={{ paddingLeft: `${12 + it.d * 15}px` }}>
              <Icon name={it.t === 'folder' ? 'folder' : 'file'} cls="icon sm fi" />
              <span className="fn">{it.n}</span>
              {it.st && <span className={`gs ${it.who}`}>{it.st}</span>}
            </div>
          ))}
        </div>
      </aside>

      <section className="pane editor">
        <div className="tabs">
          <div className="tab active">auth.ts</div>
          <div className="tab">Login.tsx</div>
          <div className="tab">Diff · worktree codex</div>
          <span className="flex" />
          <button className="iconbtn" title="Abrir en VS Code"><Icon name="code" cls="icon sm" /></button>
        </div>

        <div className="code-area">
          <pre className="code">
<span className="g">12</span>{'  '}<span className="kw">export async function</span> <span className="fn2">login</span>(req, res) {'{'}{'\n'}
<span className="g">13</span>{'    '}<span className="kw">const</span> {'{ email, password }'} = req.body;{'\n'}
<span className="g add">14</span>{'  '}<span className="addln"><span className="kw">const</span> user = <span className="kw">await</span> db.users.findByEmail(email);</span>{'\n'}
<span className="g add">15</span>{'  '}<span className="addln"><span className="kw">if</span> (!user) <span className="kw">return</span> res.status(<span className="nm3">401</span>).json({'{ error: '}<span className="str">'no auth'</span>{' }'});</span>{'\n'}
<span className="g">16</span>{'    '}<span className="kw">const</span> token = signJwt(user.id);{'\n'}
<span className="g">17</span>{'    '}<span className="cm">{'// ponytail: 1 ruta, sin refresh tokens hasta que haga falta'}</span>{'\n'}
<span className="g">18</span>{'    '}res.json({'{ token }'});{'\n'}
<span className="g">19</span>{'  }'}
          </pre>
        </div>

        <div className="term">
          <div className="ttabs">
            <span className="tt active"><span className="dot run" /> Codex · exec</span>
            <span className="tt"><span className="dot run" /> Claude · stream</span>
            <span className="tt">Logs</span>
            <span className="flex" />
            <span className="wt mono">worktree: .nexora/wt-codex</span>
          </div>
          <pre className="out">
<span className="cx">codex</span> <span className="dim">exec --json -s workspace-write</span>{'\n'}
<span className="ev">tool</span> apply_patch <span className="dim">auth.ts (+2)</span>{'\n'}
<span className="ev">tool</span> shell <span className="dim">npm run typecheck</span>{'\n'}
<span className="ok">✓</span> <span className="dim">typecheck passed</span>{'\n'}
<span className="ev">msg</span>{'  '}<span className="dim">Endpoints listos: POST /login, POST /logout</span>{'\n'}
<span className="cur">▍</span>
          </pre>
        </div>
      </section>

      <aside className="pane right">
        <div className="px"><span className="label">Agentes</span></div>
        <div className="agents">
          <div className="ag">
            <div className="agt"><span className="nm2"><i className="d" style={{ background: 'var(--codex)' }} />Codex</span><span className="pct num">65%</span></div>
            <div className="agtask">Creando API de autenticación</div>
            <div className="track"><span style={{ width: '65%', background: 'var(--codex)' }} /></div>
            <div className="agf mono">auth.ts · users.ts · middleware.ts</div>
          </div>
          <div className="ag">
            <div className="agt"><span className="nm2"><i className="d" style={{ background: 'var(--claude)' }} />Claude</span><span className="pct num">42%</span></div>
            <div className="agtask">Construyendo interfaz de Login</div>
            <div className="track"><span style={{ width: '42%', background: 'var(--claude)' }} /></div>
            <div className="agf mono">Login.tsx · Button.tsx · styles.css</div>
          </div>
        </div>

        <div className="px bt"><span className="label">Conversación</span></div>
        <div className="chat">
          <div className="cm-u">Implementa autenticación con Google.</div>
          <div className="cm-a"><span className="who" style={{ color: 'var(--codex)' }}>Codex</span> API lista: <code>POST /login</code>. <span style={{ color: 'var(--claude)' }}>@Claude</span> puedes empezar el frontend.</div>
          <div className="cm-a"><span className="who" style={{ color: 'var(--claude)' }}>Claude</span> Recibido. Conectando <code>Login.tsx</code>.</div>
        </div>
        <div className="composer">
          <input placeholder="Escribe al equipo…" />
          <button className="send"><Icon name="arrowRight" cls="icon sm" /></button>
        </div>
      </aside>
    </div>
  );
}
