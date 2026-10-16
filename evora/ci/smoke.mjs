// Test de fumée Evora du build web de Lumbra (fork tazerty/photocraft).
//
// Usage : node evora/ci/smoke.mjs <dossier du site> <csp-hashes.txt> <rapport.json>
//
// Sert le site sur une origine A (http://127.0.0.1:<port A>, en-têtes CSP de csp-hashes.txt avec
// frame-ancestors limité à l'origine B, .br/.gz servis s'ils existent) et une page parente sur une
// origine B (http://localhost:<port B>, autre hôte et autre port : origine et site différents). La
// page parente encadre le moteur dans une iframe sandbox="allow-scripts allow-same-origin". Chrome
// sans interface (celui du runner) est piloté par le protocole DevTools sur un tube
// (--remote-debugging-pipe) : aucune dépendance npm, aucune action tierce.
//
// Réussite : le document du moteur s'exécute dans l'iframe sur l'origine A, eframe démarre (le
// voile de chargement #photocraft_loading est retiré), le canvas #photocraft_canvas existe et a une
// taille, et aucune erreur de page n'est relevée (exception, console.error, entrée de journal de
// niveau erreur, violation CSP, plantage de cible) pendant le démarrage et les 3 s qui suivent.
// Seule exception admise et comptée : le 404 de fonts/manifest.txt, que l'amont traite comme le cas
// normal d'un hôte sans polices servies (patch E6 à venir). Mesure imprimée : démarrage à froid.
//
// SMOKE_SCENARIO=pont-ehp (patch E4) : le moteur est chargé avec ?host=evora sous une coquille
// minimale, le banc sert evora-host.json avec la seule page parente admise et exerce le protocole
// hôte EHP v1 (evora/ci/smoke_ehp.mjs, qui décrit ce qu'il exige).
//
// SMOKE_SCENARIO=pont-muet (patch E4) : ?host=evora sous la page parente simple, avec
// l'evora-host.json livré dans l'artefact (vide) : le moteur demande ce fichier, ne parle pas à la
// page parente, ne démarre pas, n'écrit aucun stockage, et le dit (avertissement « admits no parent
// origin »).
//
// SMOKE_SCENARIO=pont-panique (patch LUM-04) : sur le build de fumée (feature evora-smoke) seulement,
// la coquille minimale fait paniquer le moteur après l'accusé de son evora:init ; le crochet de
// panique doit envoyer un seul evora:failed {code: "panic"}. Les erreurs de page qui suivent la
// demande sont celles de la panique : comptées à part.
//
// SMOKE_SABOTAGE=banc|moteur (avec pont-ehp) ou muet (avec pont-muet) : témoin sabordé du banc ; le
// passage sort en 0 seulement si le verdict exact du témoin est VALIDE (smoke_ehp.mjs, witness).
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import * as ehp from './smoke_ehp.mjs';

// Quatrième argument facultatif (E6) : le dossier des polices servies à part, superposé au site.
const [siteDir, cspFile, reportFile, fontsDir] = process.argv.slice(2);
const TIMEOUT_MS = Number(process.env.SMOKE_TIMEOUT_MS || 120000);
const SETTLE_MS = 3000;
const CHROME = process.env.CHROME_BIN || 'google-chrome';
const ALLOWED_404 = ['/fonts/manifest.txt'];
// Paramètres passés au moteur (E3 : langue imposée ; E5 : espace de noms du stockage) et attentes.
const QUERY = process.env.SMOKE_QUERY || 'webgl';
const EXPECT_LANG = process.env.SMOKE_EXPECT_LANG || '';
const EXPECT_THEME = process.env.SMOKE_EXPECT_THEME || '';
// E5 corrigé (avec E4) : hors pont, un ?storageNs= dans l'URL est ignoré (noms de l'amont), et
// l'ancien message de purge nu ({type: "evora:purge", storageNs}, avec ?parentOrigin=) ne purge rien.
// L'espace de noms et la purge ne passent plus que par le pont EHP (scénario pont-ehp).
const URL_NS = process.env.SMOKE_URL_STORAGE_NS || '';
const OLD_PURGE = process.env.SMOKE_OLD_PURGE === '1';
// E6 : langue du navigateur (navigator.languages), pour que le moteur charge les polices servies
// de cette écriture, et exigence de polices servies (manifeste lu, au moins une police reçue).
const BROWSER_LANG = process.env.SMOKE_BROWSER_LANG || '';
const EXPECT_FONTS = process.env.SMOKE_EXPECT_FONTS === '1';
// E4 : scénario du pont hôte EHP v1.
const SCENARIO = process.env.SMOKE_SCENARIO || 'hors-pont';
// LUM-04 : panique du build de fumée, sous la même coquille minimale.
const PANIQUE = SCENARIO === ehp.PANIC_SCENARIO;
const EHP = SCENARIO === ehp.SCENARIO || PANIQUE;
// E4 : pont muet (evora-host.json de l'artefact, vide).
const MUET = SCENARIO === 'pont-muet';
if (!['hors-pont', ehp.SCENARIO, 'pont-muet', ehp.PANIC_SCENARIO].includes(SCENARIO)) throw new Error(`SMOKE_SCENARIO inconnu : ${SCENARIO}`);
// Témoins sabordés : banc et moteur sur pont-ehp, muet sur pont-muet.
const SABOTAGE = ehp.SABOTAGE;
if (SABOTAGE && (SABOTAGE === 'muet') !== MUET) throw new Error(`témoin ${SABOTAGE} incompatible avec le scénario ${SCENARIO}`);
if (SABOTAGE && !EHP && !MUET) throw new Error(`témoin ${SABOTAGE} sans scénario de pont`);
if (SABOTAGE && PANIQUE) throw new Error(`aucun témoin sabordé pour ${SCENARIO}`);

const TYPES = {
  '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm', '.css': 'text/css; charset=utf-8', '.txt': 'text/plain; charset=utf-8',
  '.json': 'application/json', '.png': 'image/png', '.svg': 'image/svg+xml', '.ttf': 'font/ttf', '.woff2': 'font/woff2',
};

const report = { ok: false, errors: [], allowed: [], warnings: [], console: [], served: {}, steps: [], afterPanic: [] };
// LUM-04 : une fois la panique demandée (pont-panique), les erreurs de page sont les siennes.
let panicSent = false;
const pageError = (item) => (panicSent ? report.afterPanic : report.errors).push(item);
const log = (m) => { report.steps.push(m); console.log(m); };

function policyFor(parentOrigin) {
  const line = fs.readFileSync(cspFile, 'utf8').split('\n').find((l) => l.startsWith('policy '));
  if (!line) throw new Error(`pas de ligne « policy » dans ${cspFile}`);
  return line.slice('policy '.length).replace('{{EVORA_ORIGIN}}', parentOrigin);
}

function listen(handler) {
  return new Promise((resolve) => {
    const srv = http.createServer(handler);
    srv.listen(0, () => resolve(srv));
  });
}

function engineHandler(state) {
  const roots = [siteDir, fontsDir].filter(Boolean).map((d) => path.resolve(d));
  return (req, res) => {
    const url = new URL(req.url, 'http://x');
    let rel = decodeURIComponent(url.pathname);
    if (rel.endsWith('/')) rel += 'index.html';
    if (state.hostConfig && rel === '/evora-host.json') {
      // Ce qu'écrit le provisionnement d'Evora à côté d'index.html (HOST-PROTOCOL §2.3).
      report.served[`${rel} banc`] = (report.served[`${rel} banc`] || 0) + 1;
      res.writeHead(200, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' });
      res.end(state.hostConfig);
      return;
    }
    const file = roots.map((root) => path.resolve(root, '.' + rel))
      .find((f, i) => f.startsWith(roots[i] + path.sep) && fs.existsSync(f) && fs.statSync(f).isFile());
    if (!file) {
      state.notFound.push(rel);
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('not found');
      return;
    }
    const headers = {
      'Content-Type': TYPES[path.extname(file)] || 'application/octet-stream',
      'X-Content-Type-Options': 'nosniff',
      'Cache-Control': 'no-store',
    };
    if (path.extname(file) === '.html') headers['Content-Security-Policy'] = state.policy;
    const accept = String(req.headers['accept-encoding'] || '');
    let body = file;
    for (const [enc, ext] of [['br', '.br'], ['gzip', '.gz']]) {
      if (accept.includes(enc) && fs.existsSync(file + ext)) {
        headers['Content-Encoding'] = enc;
        headers['Vary'] = 'Accept-Encoding';
        body = file + ext;
        break;
      }
    }
    const key = `${rel} ${headers['Content-Encoding'] || 'identity'}`;
    report.served[key] = (report.served[key] || 0) + 1;
    res.writeHead(200, headers);
    fs.createReadStream(body).pipe(res);
  };
}

function ehpHandler(engineOrigin, parentOrigin, foreignOrigin) {
  const engineSrc = `${engineOrigin}/index.html?${QUERY}`;
  const assets = {
    '/': ['text/html; charset=utf-8', ehp.parentPage(engineSrc, foreignOrigin, parentOrigin), ehp.parentCsp(engineOrigin, foreignOrigin)],
    '/harness.js': ['text/javascript; charset=utf-8', ehp.harnessJs(), null],
    '/witness.html': ['text/html; charset=utf-8', ehp.WITNESS_HTML, ehp.WITNESS_CSP],
    '/witness.js': ['text/javascript; charset=utf-8', ehp.WITNESS_JS, null],
  };
  return (req, res) => {
    const asset = assets[new URL(req.url, 'http://x').pathname];
    if (!asset) {
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('not found');
      return;
    }
    const headers = { 'Content-Type': asset[0], 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' };
    if (asset[2]) headers['Content-Security-Policy'] = asset[2];
    res.writeHead(200, headers);
    res.end(asset[1]);
  };
}

function parentHandler(engineOrigin, parentOrigin) {
  const page = `<!doctype html><html lang="fr"><head><meta charset="utf-8"><title>Evora : page parente du test de fumée</title>
<link rel="icon" href="data:,"></head><body style="margin:0;background:#fff">
<iframe id="engine" src="${engineOrigin}/index.html?${QUERY.replace('{{PARENT_ORIGIN}}', encodeURIComponent(parentOrigin))}" width="1280" height="800" style="border:0;display:block"
 sandbox="allow-scripts allow-same-origin" allow="fullscreen; clipboard-read; clipboard-write"></iframe>
</body></html>`;
  return (req, res) => {
    res.writeHead(200, {
      'Content-Type': 'text/html; charset=utf-8',
      'Content-Security-Policy': `default-src 'none'; frame-src ${engineOrigin}; img-src data:; style-src 'unsafe-inline'`,
      'Cache-Control': 'no-store',
    });
    res.end(page);
  };
}

// Protocole DevTools sur tube : messages JSON terminés par un octet nul (fd 3 : vers Chrome, fd 4 : depuis Chrome).
function cdp(proc) {
  const write = proc.stdio[3];
  const read = proc.stdio[4];
  let seq = 0;
  let buf = Buffer.alloc(0);
  const pending = new Map();
  const listeners = [];
  read.on('data', (chunk) => {
    buf = Buffer.concat([buf, chunk]);
    let i;
    while ((i = buf.indexOf(0)) >= 0) {
      const msg = JSON.parse(buf.subarray(0, i).toString('utf8'));
      buf = buf.subarray(i + 1);
      if (msg.id && pending.has(msg.id)) {
        const p = pending.get(msg.id);
        pending.delete(msg.id);
        if (msg.error) p.reject(new Error(`${p.method} : ${JSON.stringify(msg.error)}`));
        else p.resolve(msg.result);
      } else {
        for (const l of listeners) l(msg);
      }
    }
  });
  return {
    send(method, params = {}, sessionId) {
      const msg = { id: ++seq, method, params };
      if (sessionId) msg.sessionId = sessionId;
      write.write(JSON.stringify(msg) + '\0');
      return new Promise((resolve, reject) => pending.set(msg.id, { resolve, reject, method }));
    },
    on(fn) { listeners.push(fn); },
  };
}

const READY_PROBE = `(() => {
  const mark = () => {
    if (window.__evoraReady === undefined && document.readyState !== 'loading' && !document.getElementById('photocraft_loading')) {
      window.__evoraReady = performance.now();
    }
  };
  new MutationObserver(mark).observe(document, { childList: true, subtree: true });
  document.addEventListener('DOMContentLoaded', mark);
})();`;

const STATE = `JSON.stringify((() => {
  const c = document.getElementById('photocraft_canvas');
  const l = document.getElementById('photocraft_loading');
  return { ready: window.__evoraReady ?? null, now: performance.now(), readyState: document.readyState, origin: location.origin,
    parent: (location.ancestorOrigins && location.ancestorOrigins[0]) || null, title: document.title,
    loading: l ? l.textContent.trim().slice(0, 300) : null,
    canvas: c ? { width: c.width, height: c.height, client: [c.clientWidth, c.clientHeight] } : null };
})())`;

// Stockage du document moteur : clés localStorage et bases IndexedDB (E3, E5).
const STORAGE = `(async () => {
  const keys = Object.keys(localStorage);
  const prefsKey = keys.find((k) => k.endsWith('photocraft.preferences')) || null;
  let language = null;
  let theme = null;
  try {
    const iface = prefsKey ? JSON.parse(localStorage.getItem(prefsKey)).interface || {} : {};
    language = iface.language ?? null;
    theme = iface.theme ?? null;
  } catch { language = 'illisible'; }
  const dbs = indexedDB.databases ? (await indexedDB.databases()).map((d) => d.name) : null;
  return JSON.stringify({ keys, prefsKey, language, theme, dbs });
})()`;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function main() {
  const state = { notFound: [], policy: '' };
  const engine = await listen(engineHandler(state));
  const engineOrigin = `http://127.0.0.1:${engine.address().port}`;
  // L'origine B n'est connue qu'une fois le port attribué : le gestionnaire la lit à chaque requête.
  const origins = { parent: '', foreign: '' };
  // E4 : une troisième origine (C, même hôte, autre port) sert la fenêtre témoin non admise.
  const foreign = EHP ? await listen((req, res) => ehpHandler(engineOrigin, origins.parent, origins.foreign)(req, res)) : null;
  origins.foreign = foreign ? `http://localhost:${foreign.address().port}` : '';
  const parent = await listen((req, res) => (EHP ? ehpHandler(engineOrigin, origins.parent, origins.foreign) : parentHandler(engineOrigin, origins.parent))(req, res));
  const parentOrigin = `http://localhost:${parent.address().port}`;
  origins.parent = parentOrigin;
  if (EHP) state.hostConfig = ehp.hostConfig(parentOrigin);
  // Témoin « muet » : le banc admet à tort la page parente (le pont doit alors parler, et le banc le voir).
  if (MUET && SABOTAGE === 'muet') state.hostConfig = ehp.hostConfig(parentOrigin);
  state.policy = policyFor(parentOrigin);
  report.engineOrigin = engineOrigin;
  report.parentOrigin = parentOrigin;
  report.policy = state.policy;
  report.scenario = SCENARIO;
  log(`scénario : ${SCENARIO} ; moteur : ${engineOrigin} ; parent : ${parentOrigin}${EHP ? ` ; témoin d'origine non admise : ${origins.foreign}` : ''}`);

  const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'evora-smoke-'));
  const args = [
    '--headless=new', '--no-sandbox', '--remote-debugging-pipe', `--user-data-dir=${profile}`,
    '--no-first-run', '--no-default-browser-check', '--disable-extensions', '--disable-background-networking',
    '--site-per-process', '--window-size=1400,900',
    '--enable-unsafe-swiftshader', '--use-angle=swiftshader', '--ignore-gpu-blocklist',
    ...(BROWSER_LANG ? [`--lang=${BROWSER_LANG}`, `--accept-lang=${BROWSER_LANG}`] : []), 'about:blank',
  ];
  const chrome = spawn(CHROME, args, { stdio: ['ignore', 'ignore', 'pipe', 'pipe', 'pipe'] });
  let stderr = '';
  chrome.stderr.on('data', (d) => { stderr += d.toString(); });
  chrome.on('exit', (code) => log(`Chrome terminé (code ${code})`));
  const c = cdp(chrome);
  const version = await c.send('Browser.getVersion');
  report.chrome = version.product;
  log(`navigateur : ${version.product}`);

  let frameSession = null;
  let frameUrl = null;
  const isAllowed = (url, text) => ALLOWED_404.some((p) => (url || '').endsWith(p)) && /404/.test(text || '');
  c.on((msg) => {
    const p = msg.params || {};
    const where = msg.sessionId && msg.sessionId === frameSession ? 'moteur' : 'parent';
    switch (msg.method) {
      case 'Target.attachedToTarget':
        // Seul le moteur (origine A, autre site) est une iframe hors processus : les témoins du
        // scénario pont-ehp sont du même site que la page parente et n'ont pas de cible propre.
        if (p.targetInfo.type === 'iframe' && !frameSession) {
          frameSession = p.sessionId;
          frameUrl = p.targetInfo.url;
          (async () => {
            // E4 : le document moteur se croit au premier plan (le canvas peut prendre le focus clavier).
            if (EHP) await c.send('Emulation.setFocusEmulationEnabled', { enabled: true }, frameSession).catch(() => {});
            await c.send('Runtime.enable', {}, frameSession);
            await c.send('Log.enable', {}, frameSession);
            await c.send('Page.enable', {}, frameSession);
            await c.send('Page.addScriptToEvaluateOnNewDocument', { source: READY_PROBE }, frameSession);
            // E5 : restes d'autres comptes et de l'amont, et noms d'un autre moteur de la même
            // origine, posés avant le démarrage du moteur (evora:init doit purger les premiers).
            if (EHP) await c.send('Page.addScriptToEvaluateOnNewDocument', { source: ehp.SEED_JS }, frameSession);
            // Témoin « moteur » : le moteur croit effacer (removeItem, clear, deleteDatabase sans effet).
            if (EHP && SABOTAGE === 'moteur') await c.send('Page.addScriptToEvaluateOnNewDocument', { source: ehp.ENGINE_SABOTAGE_JS }, frameSession);
            await c.send('Runtime.runIfWaitingForDebugger', {}, frameSession);
          })().catch((e) => report.errors.push(`attache de l'iframe : ${e.message}`));
        } else if (p.waitingForDebugger) {
          c.send('Runtime.runIfWaitingForDebugger', {}, p.sessionId).catch(() => {});
        }
        break;
      case 'Runtime.exceptionThrown': {
        const d = p.exceptionDetails || {};
        pageError(`${where} : exception ${(d.exception && d.exception.description) || d.text} (${d.url || ''}:${d.lineNumber})`);
        break;
      }
      case 'Runtime.consoleAPICalled': {
        const text = (p.args || []).map((a) => a.value ?? a.description ?? '').join(' ');
        if (p.type === 'error' || p.type === 'assert') pageError(`${where} : console.${p.type} ${text}`);
        else if (p.type === 'warning') report.warnings.push(`${where} : ${text}`.slice(0, 300));
        else if (report.console.length < 60) report.console.push(`${where} : ${text}`.slice(0, 300));
        break;
      }
      case 'Log.entryAdded': {
        const e = p.entry || {};
        if (e.level === 'error') {
          const item = `${where} : [${e.source}] ${e.text} ${e.url || ''}`.trim();
          if (isAllowed(e.url, e.text)) report.allowed.push(item);
          else pageError(item);
        } else if (e.level === 'warning') {
          report.warnings.push(`${where} : [${e.source}] ${e.text}`.slice(0, 300));
        }
        break;
      }
      case 'Target.targetCrashed':
      case 'Inspector.targetCrashed':
        pageError(`${where} : plantage de la cible`);
        break;
      default:
        break;
    }
  });

  const { targetId } = await c.send('Target.createTarget', { url: 'about:blank' });
  const { sessionId: page } = await c.send('Target.attachToTarget', { targetId, flatten: true });
  await c.send('Runtime.enable', {}, page);
  await c.send('Log.enable', {}, page);
  await c.send('Page.enable', {}, page);
  // Hors pont : tout message reçu par la page parente est relevé (aucun n'est attendu du moteur).
  if (!EHP) {
    await c.send('Page.addScriptToEvaluateOnNewDocument', {
      source: "window.__parentMessages = []; addEventListener('message', (e) => window.__parentMessages.push({ origin: e.origin, data: (() => { try { return JSON.parse(JSON.stringify(e.data)); } catch { return String(e.data); } })() }));",
    }, page);
  }
  await c.send('Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: true, flatten: true }, page);
  const t0 = Date.now();
  await c.send('Page.navigate', { url: `${parentOrigin}/` }, page);
  log('navigation de la page parente lancée');

  // E4 : avec ?host=evora, le moteur annonce evora:ready puis attend evora:init avant de démarrer.
  const bench = EHP ? new ehp.EhpBench({ c, page, frame: () => frameSession, engineOrigin, parentOrigin, log }) : null;
  let handshake = true;
  if (bench) {
    try {
      handshake = await bench.handshake();
    } catch (e) {
      bench.fail('bench', `poignée de main interrompue : ${e.message}`);
      handshake = false;
    }
  }

  let last = null;
  let readyAt = null;
  while (handshake && Date.now() - t0 < TIMEOUT_MS) {
    await sleep(200);
    if (!frameSession) continue;
    try {
      const r = await c.send('Runtime.evaluate', { expression: STATE, returnByValue: true }, frameSession);
      last = JSON.parse(r.result.value);
    } catch {
      continue;
    }
    if (last.loading && /failed to start/i.test(last.loading)) break;
    const started = last.ready !== null || (last.loading === null && last.readyState === 'complete');
    if (started && last.canvas && last.canvas.width > 0 && last.canvas.height > 0) {
      readyAt = Date.now();
      // Sonde absente (script d'initialisation non injecté) : borne haute, l'instant du sondage.
      if (last.ready === null) report.coldStartUpperBound = true;
      report.coldStartMs = Math.round(last.ready ?? last.now);
      break;
    }
  }
  if (readyAt) {
    await sleep(SETTLE_MS);
    const r = await c.send('Runtime.evaluate', { expression: STATE, returnByValue: true }, frameSession);
    last = JSON.parse(r.result.value);
    if (bench) {
      try {
        if (PANIQUE) await bench.panic(() => { panicSent = true; });
        else await bench.session();
      } catch (e) {
        bench.fail('bench', `scénario interrompu : ${e.message}`);
      }
      await sleep(2000);
    }
    const st = await c.send('Runtime.evaluate', { expression: STORAGE, returnByValue: true, awaitPromise: true }, frameSession);
    report.storage = JSON.parse(st.result.value);
    if (OLD_PURGE && URL_NS && !bench) {
      // L'ancienne purge, désormais sans effet : le stockage de l'amont doit rester intact.
      await c.send('Runtime.evaluate', {
        expression: `document.getElementById('engine').contentWindow.postMessage({ type: 'evora:purge', storageNs: ${JSON.stringify(URL_NS)} }, ${JSON.stringify(engineOrigin)}); true`,
      }, page);
      await sleep(2500);
      const after = await c.send('Runtime.evaluate', { expression: STORAGE, returnByValue: true, awaitPromise: true }, frameSession);
      report.oldPurge = JSON.parse(after.result.value);
    }
    if (!bench) {
      const m = await c.send('Runtime.evaluate', { expression: 'JSON.stringify(window.__parentMessages || null)', returnByValue: true }, page);
      report.parentMessages = JSON.parse(m.result.value);
    }
  }
  if (MUET && frameSession) {
    // Pont muet : rien n'a démarré ; stockage du document moteur et messages reçus par la page parente.
    try {
      const st = await c.send('Runtime.evaluate', { expression: STORAGE, returnByValue: true, awaitPromise: true }, frameSession);
      report.storage = JSON.parse(st.result.value);
      const m = await c.send('Runtime.evaluate', { expression: 'JSON.stringify(window.__parentMessages || null)', returnByValue: true }, page);
      report.parentMessages = JSON.parse(m.result.value);
    } catch (e) {
      report.errors.push(`pont muet : relevé impossible : ${e.message}`);
    }
  }
  if (bench) {
    try { await bench.refresh(); } catch { /* journal déjà relevé */ }
    report.ehp = { ids: bench.ids, exchanges: bench.summary(), storage: bench.storageLog, problems: bench.verify() };
  }
  report.frame = { url: frameUrl, state: last };
  if (report.coldStartMs === undefined) report.coldStartMs = null;
  report.wallClockMs = readyAt ? readyAt - t0 : null;
  report.notFound = state.notFound;

  try { await c.send('Browser.close'); } catch { /* Chrome ferme le tube */ }
  setTimeout(() => chrome.kill('SIGKILL'), 5000).unref();
  engine.close();
  parent.close();
  if (foreign) foreign.close();

  const problems = [...report.errors, ...(report.ehp ? report.ehp.problems : [])];
  if (!frameSession) problems.push("aucune iframe hors processus attachée : l'origine du moteur n'est pas distincte");
  if (!readyAt && !MUET) problems.push(`le moteur n'a pas démarré en ${TIMEOUT_MS} ms : ${JSON.stringify(last)}`);
  if (MUET) {
    problems.push(...ehp.muteProblems({ started: Boolean(readyAt), last, storage: report.storage, parentMessages: report.parentMessages,
      served: report.served, warnings: report.warnings }));
  }
  if (last && last.origin !== engineOrigin) problems.push(`origine du document moteur inattendue : ${last && last.origin}`);
  if (last && last.parent !== parentOrigin) problems.push(`origine parente inattendue : ${last && last.parent}`);
  const stray404 = state.notFound.filter((p) => !ALLOWED_404.includes(p));
  if (stray404.length) problems.push(`404 inattendus : ${stray404.join(', ')}`);
  const st = report.storage || { keys: [], dbs: [] };
  if (EXPECT_THEME && st.theme !== EXPECT_THEME) problems.push(`thème imposé non appliqué : préférence ${st.theme}, attendu ${EXPECT_THEME}`);
  if (EXPECT_LANG && st.language !== EXPECT_LANG) problems.push(`langue imposée non appliquée : préférence ${st.language}, attendu ${EXPECT_LANG}`);
  if (URL_NS && !bench) {
    // Hors pont : noms de l'amont, le ?storageNs= de l'URL n'a aucun effet.
    if (!st.keys.includes('photocraft.preferences')) problems.push(`hors pont : préférences absentes sous le nom de l'amont (clés : ${st.keys.join(', ')})`);
    const fromUrl = st.keys.filter((k) => k.startsWith(`${URL_NS}:`)).concat((st.dbs || []).filter((d) => d.startsWith(`${URL_NS}:`)));
    if (fromUrl.length) problems.push(`hors pont : l'espace de noms de l'URL a été appliqué (${fromUrl.join(', ')})`);
    if (st.dbs && !st.dbs.includes('photocraft.presets')) problems.push(`hors pont : base des préréglages absente sous le nom de l'amont (bases : ${st.dbs.join(', ')})`);
  }
  if (OLD_PURGE && URL_NS && !bench) {
    const after = report.oldPurge;
    if (!after) problems.push('hors pont : contrôle de l\'ancienne purge non exécuté');
    else if (!after.keys.includes('photocraft.preferences') || (after.dbs && !after.dbs.includes('photocraft.presets'))) {
      problems.push(`hors pont : l'ancien message de purge (sans pont) a encore effacé le stockage : ${JSON.stringify(after)}`);
    }
  }
  if (!bench && !MUET && report.parentMessages && report.parentMessages.length) {
    problems.push(`hors pont : le moteur a écrit à la page parente : ${JSON.stringify(report.parentMessages).slice(0, 300)}`);
  }
  if (EXPECT_FONTS) {
    const served = Object.keys(report.served);
    if (!served.some((k) => k.startsWith('/fonts/manifest.txt '))) problems.push('manifeste des polices servies non lu');
    if (!served.some((k) => /^\/fonts\/.+\.(ttf|otf|ttc) /.test(k))) problems.push(`aucune police servie reçue (langue du navigateur : ${BROWSER_LANG || 'défaut'})`);
  }
  report.problems = problems;
  report.ok = problems.length === 0;
  // Témoin sabordé : VALIDE (code 0) seulement si chaque rouge attendu est vu, à sa place, crédité à
  // son sabotage, et si rien d'autre ne rougit (smoke_ehp.mjs, witness).
  if (SABOTAGE) {
    const w = ehp.witness({ mode: SABOTAGE, problems, sabotaged: bench ? bench.sabotaged : [], checks: bench ? bench.checks : [],
      started: Boolean(readyAt) });
    report.witness = w;
    report.ok = w.ok;
    problems.push(w.ok
      ? `TÉMOIN SABORDÉ « ${SABOTAGE} » VALIDE : ${w.seen.length} détecteur(s) rougi(s), chacun à sa place et seuls (${w.seen.join(', ')}) : le banc sait rougir`
      : `TÉMOIN SABORDÉ « ${SABOTAGE} » INVALIDE : ${w.reasons.join(' ; ')}`);
  }
  if (!report.ok && stderr) report.chromeStderr = stderr.split('\n').slice(-40);
  fs.writeFileSync(reportFile, JSON.stringify(report, null, 1));

  const lines = [
    '### Test de fumée (Chrome sans interface, origine différente)', '',
    `| Élément | Valeur |`, `| --- | --- |`,
    `| Navigateur | ${report.chrome} |`,
    `| Origine du moteur (A) | \`${engineOrigin}\` |`,
    `| Page parente (B) | \`${parentOrigin}\` |`,
    `| Démarrage à froid (iframe, performance.now) | ${report.coldStartMs ?? '—'} ms${report.coldStartUpperBound ? ' (borne haute)' : ''} |`,
    `| Délai mesuré depuis la navigation parente | ${report.wallClockMs ?? '—'} ms |`,
    `| Canvas | ${last && last.canvas ? `${last.canvas.width}×${last.canvas.height}` : 'absent'} |`,
    `| Titre du document moteur | ${last ? last.title : '—'} |`,
    `| Scénario | ${SCENARIO} |`,
    `| Paramètres du moteur | \`?${QUERY}\` |`,
    ...(report.ehp ? [`| Échanges EHP vus de la page parente | ${report.ehp.exchanges.length} message(s) du moteur |`] : []),
    ...(PANIQUE ? [`| Erreurs de page après la demande de panique (attendues) | ${report.afterPanic.length} |`] : []),
    `| Stockage (localStorage / IndexedDB) | ${report.storage ? `${report.storage.keys.join(', ') || 'aucune clé'} / ${(report.storage.dbs || []).join(', ') || 'aucune base'}` : '—'} |`,
    `| Ancienne purge hors pont (sans effet attendu) | ${report.oldPurge ? `${report.oldPurge.keys.length} clé(s), ${(report.oldPurge.dbs || []).length} base(s) après le message` : '—'} |`,
    `| Polices servies reçues | ${Object.keys(report.served).filter((k) => k.startsWith('/fonts/')).join(', ') || 'aucune'} |`,
    `| Thème des préférences | ${report.storage ? report.storage.theme ?? '—' : '—'}${EXPECT_THEME ? ` (attendu : ${EXPECT_THEME})` : ''} |`,
    `| Langue des préférences | ${report.storage ? report.storage.language ?? '—' : '—'}${EXPECT_LANG ? ` (attendue : ${EXPECT_LANG})` : ''} |`,
    `| Erreurs de page | ${report.errors.length} |`,
    `| Exceptions admises (404 fonts/manifest.txt) | ${report.allowed.length} |`,
    `| Avertissements de console | ${report.warnings.length} |`,
    ...(report.witness ? [`| Témoin sabordé « ${report.witness.mode} » | ${report.witness.seen.length} rouge(s) vu(s) sur ${report.witness.expected.length} attendu(s), ${report.witness.sabotaged.length} sabotage(s) du banc joué(s) |`] : []),
    `| Verdict | ${report.witness ? (report.ok ? 'témoin VALIDE (rouge attendu, exact)' : 'témoin INVALIDE') : (report.ok ? 'vert' : 'ROUGE')} |`, '',
    ...problems.map((p) => `- ${p}`), '',
  ];
  const text = lines.join('\n');
  console.log(text);
  for (const w of report.warnings.slice(0, 20)) console.log(`avertissement : ${w}`);
  for (const m of report.console.slice(0, 30)) console.log(`console : ${m}`);
  for (const m of report.afterPanic.slice(0, 10)) console.log(`après la panique : ${m.slice(0, 300)}`);
  for (const [k, v] of Object.entries(report.served)) console.log(`servi : ${k} ×${v}`);
  if (report.ehp) for (const m of report.ehp.exchanges) console.log(`ehp : ${m}`);
  if (report.ehp) for (const st of report.ehp.storage) console.log(`ehp stockage : ${JSON.stringify(st)}`);
  if (report.witness) {
    for (const x of report.witness.sabotaged) console.log(`témoin, sabotage joué : ${x}`);
    for (const ck of (bench ? bench.checks : [])) console.log(`témoin, contrôle ${ck.label} : ${ck.tags.join(', ') || 'vert'}`);
  }
  if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, text + '\n');
  process.exit(report.ok ? 0 : 1);
}

main().catch((e) => {
  report.problems = [`échec du test : ${e.stack || e}`];
  try { fs.writeFileSync(reportFile, JSON.stringify(report, null, 1)); } catch { /* rien */ }
  console.error(e);
  process.exit(1);
});
