// Scénario de fumée « pont-documents » du build web de Lumbra (patch E5 pour plusieurs documents,
// HOST-PROTOCOL.md d'Evora §7.7) : la purge d'un compte quand d'autres documents moteur de la même
// origine sont ouverts.
//
// Usage : node evora/ci/smoke_docs.mjs <dossier du site> <csp-hashes.txt> <rapport.json>
//
// Même banc que smoke.mjs (Chrome sans interface piloté par le protocole DevTools sur un tube,
// moteur sur l'origine A, coquille minimale sur l'origine B, evora-host.json écrit par le banc avec
// la seule origine B admise), mais la coquille encadre TROIS documents moteur (?host=evora), « un »,
// « deux » et « trois », comme trois onglets d'un même navigateur :
//   0. reste d'un ancien compte posé avant tout démarrage (base et préférences, sans marqueur, comme
//      les laisse un build antérieur) ;
//   1. « un » démarre pour le compte A et enregistre ses préférences ; il reste ouvert, sa base
//      ouverte. « deux » démarre pour le compte B : juste après l'accusé de son evora:init, la base
//      du compte A, son marqueur et le reste de l'ancien compte ont disparu (§7.7 b), alors que le
//      document du compte A vit encore ; puis un changement de thème dans « un » ne réécrit rien ;
//   2. « trois » démarre pour le compte B (second onglet du même compte) et enregistre ses
//      préférences ; evora:purge {scope: "current"} dans « deux » est accusé positivement (pas de
//      busy, pas de silence) alors que « trois » tient la base ouverte, plus rien du compte B ne
//      reste juste après l'accusé, et un changement de thème dans « trois » ne réécrit rien.
// Chaque contrôle porte une étiquette [docs:…] et un nom (checkpoint) pour l'attribution du témoin.
//
// Variantes (SMOKE_DOCS_VARIANT), posées dans chaque document moteur avant son démarrage :
//   - « normal » ;
//   - « sans-liste » : indexedDB.databases() retiré (navigateur qui ne sait pas lister ses bases) ;
//     le banc lit les bases par une copie qu'il garde pour lui ;
//   - « versionchange » : l'écoute storage des documents moteur est neutralisée (seule l'écoute
//     versionchange de la base agit) ;
//   - « storage » : l'écoute versionchange est neutralisée (seule l'écoute storage agit).
// Témoin sabordé (SMOKE_SABOTAGE=ecoutes) : les deux écoutes neutralisées ; VALIDE (code 0) seulement
// si les rouges sont exactement DOCS_REDS, chacun à son contrôle (smoke_ehp.mjs, witness).
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { tagOf, witness } from './smoke_ehp.mjs';

const [siteDir, cspFile, reportFile] = process.argv.slice(2);
const CHROME = process.env.CHROME_BIN || 'google-chrome';
const VARIANT = process.env.SMOKE_DOCS_VARIANT || 'normal';
const SABOTAGE = process.env.SMOKE_SABOTAGE || '';
if (!['normal', 'sans-liste', 'versionchange', 'storage'].includes(VARIANT)) throw new Error(`SMOKE_DOCS_VARIANT inconnue : ${VARIANT}`);
if (!['', 'ecoutes'].includes(SABOTAGE)) throw new Error(`SMOKE_SABOTAGE inconnu pour ce scénario : ${SABOTAGE}`);
if (SABOTAGE && VARIANT !== 'normal') throw new Error('le témoin « ecoutes » se joue sur la variante normale');
const ALLOWED_404 = ['/fonts/manifest.txt'];
const DOCS = ['un', 'deux', 'trois'];
// Comptes (valeurs de banc) : A, B, et un ancien compte laissé par un build antérieur.
const NS_A = 'cd'.repeat(32);
const NS_B = 'ab'.repeat(32);
const NS_OLD = 'ef'.repeat(32);
const PREFS = 'photocraft.preferences';
const PRESETS = 'photocraft.presets';
const BENCH_MAX_BYTES = 8 * 1024 * 1024;
const ULID_RE = /^[0-9A-HJKMNP-TV-Z]{26}$/;
const ACK_CODES = ['clean', 'cancelled', 'conflict', 'too_large', 'quota', 'forbidden', 'not_found', 'invalid_content',
  'unsupported', 'rate_limited', 'read_only', 'busy', 'capability', 'network', 'server', 'timeout'];
// Attente d'une purge : au-delà de l'échéance de stockage du moteur (10 s, indexed_presets.rs), pour
// voir sa réponse busy plutôt que le délai du banc.
const PURGE_WAIT_MS = 25000;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const TYPES = {
  '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.wasm': 'application/wasm',
  '.css': 'text/css; charset=utf-8', '.txt': 'text/plain; charset=utf-8', '.json': 'application/json', '.png': 'image/png',
};

const report = { ok: false, variant: VARIANT, sabotage: SABOTAGE, errors: [], allowed: [], warnings: [], console: [], served: {}, steps: [], storage: [] };
const log = (m) => { report.steps.push(m); console.log(m); };
const problems = [];
const checks = [];
const fail = (tag, text) => problems.push(`[docs:${tag}] ${text}`);
// Un contrôle nommé : les étiquettes qu'il a fait rougir (attribution du témoin).
async function checkpoint(label, fn) {
  const before = problems.length;
  await fn();
  checks.push({ label, tags: [...new Set(problems.slice(before).map(tagOf).filter(Boolean))] });
}

// Posés dans chaque document moteur avant son démarrage (Page.addScriptToEvaluateOnNewDocument).
const NO_LIST_JS = `(() => {
  const list = IDBFactory.prototype.databases;
  Object.defineProperty(window, '__evoraListDatabases', { value: list, enumerable: false });
  delete IDBFactory.prototype.databases;
})();`;
const NO_VERSIONCHANGE_JS = `(() => {
  Object.defineProperty(IDBDatabase.prototype, 'onversionchange', { configurable: true, get() { return null; }, set(v) {} });
  const add = EventTarget.prototype.addEventListener;
  EventTarget.prototype.addEventListener = function (type, ...rest) {
    if (type === 'versionchange' && this instanceof IDBDatabase) return undefined;
    return add.call(this, type, ...rest);
  };
})();`;
const NO_STORAGE_JS = `(() => {
  Object.defineProperty(window, 'onstorage', { configurable: true, get() { return null; }, set(v) {} });
  const add = EventTarget.prototype.addEventListener;
  EventTarget.prototype.addEventListener = function (type, ...rest) {
    if (type === 'storage' && this === window) return undefined;
    return add.call(this, type, ...rest);
  };
})();`;
const ENGINE_SCRIPTS = {
  normal: SABOTAGE === 'ecoutes' ? [NO_VERSIONCHANGE_JS, NO_STORAGE_JS] : [],
  'sans-liste': [NO_LIST_JS],
  versionchange: [NO_STORAGE_JS],
  storage: [NO_VERSIONCHANGE_JS],
}[VARIANT];

// Le reste d'un ancien compte, posé depuis le document « un » avant tout evora:init.
const SEED_OLD = `(async () => {
  localStorage.setItem('${NS_OLD}:${PREFS}', '{}');
  await new Promise((r) => { const q = indexedDB.open('${NS_OLD}:${PRESETS}', 1); q.onupgradeneeded = () => q.result.createObjectStore('files');
    q.onsuccess = () => { q.result.close(); r(); }; q.onerror = () => r(); });
  return true;
})()`;

// Le stockage de l'origine moteur, lu depuis un document moteur (le banc garde sa copie de
// indexedDB.databases() dans la variante sans-liste).
const STORAGE = `(async () => {
  const list = window.__evoraListDatabases || IDBFactory.prototype.databases;
  const dbs = list ? (await list.call(indexedDB)).map((d) => d.name) : null;
  const keys = Object.keys(localStorage);
  const theme = (ns) => { try { return ((JSON.parse(localStorage.getItem(ns + ':${PREFS}') || 'null') || {}).interface || {}).theme ?? null; } catch (e) { return 'illisible'; } };
  return JSON.stringify({ keys, dbs, themeA: theme('${NS_A}'), themeB: theme('${NS_B}') });
})()`;

function parentPage(engineOrigin) {
  const frames = DOCS.map((d) => `<iframe id="${d}" src="${engineOrigin}/index.html?host=evora&webgl&doc=${d}" width="640" height="400"
 style="border:0;display:block" sandbox="allow-scripts allow-same-origin"></iframe>`).join('\n');
  return `<!doctype html><html lang="fr"><head><meta charset="utf-8"><title>Evora : trois documents moteur</title>
<link rel="icon" href="data:,"></head><body style="margin:0;background:#fff">
${frames}
<script src="/harness.js"></script>
</body></html>`;
}

// Coquille minimale : journal de ce que chaque document moteur poste, et envoi d'enveloppes EHP.
const HARNESS_JS = `(() => {
  const frames = ${JSON.stringify(DOCS)}.map((d) => [d, document.getElementById(d)]);
  const ENGINE = new URL(frames[0][1].src).origin;
  const A = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
  const ulid = () => {
    let t = Date.now(), s = '';
    for (let i = 0; i < 10; i++) { s = A[t % 32] + s; t = Math.floor(t / 32); }
    for (const b of crypto.getRandomValues(new Uint8Array(16))) s += A[b % 32];
    return s;
  };
  const log = [];
  addEventListener('message', (e) => {
    const from = (frames.find(([, f]) => e.source === f.contentWindow) || ['?'])[0];
    const d = e.data;
    const plain = d !== null && typeof d === 'object' && Object.getPrototypeOf(d) === Object.prototype;
    let msg;
    try { msg = plain ? JSON.parse(JSON.stringify(d)) : String(d).slice(0, 200); } catch (err) { msg = 'illisible'; }
    log.push({ at: Math.round(performance.now()), from, origin: e.origin, plain, keys: plain ? Object.keys(d) : null, msg });
  });
  window.__docs = {
    log,
    send(which, type, payload, re) {
      const id = ulid();
      const msg = { v: 1, type, id };
      if (re) msg.re = re;
      msg.payload = payload;
      document.getElementById(which).contentWindow.postMessage(msg, ENGINE);
      return id;
    },
  };
})();`;

function initPayload(ns, capabilities) {
  return { lang: 'fr', engineLang: 'fr', theme: 'light', accent: '#4F46E5', readOnly: false, storageNs: ns, capabilities,
    prefs: {}, limits: { maxDocumentBytes: BENCH_MAX_BYTES, maxExportBytes: BENCH_MAX_BYTES } };
}

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

// Le site du moteur (mêmes règles que smoke.mjs : CSP du build, .br/.gz, evora-host.json du banc).
function engineHandler(state) {
  const root = path.resolve(siteDir);
  return (req, res) => {
    const url = new URL(req.url, 'http://x');
    let rel = decodeURIComponent(url.pathname);
    if (rel.endsWith('/')) rel += 'index.html';
    if (rel === '/evora-host.json') {
      report.served[`${rel} banc`] = (report.served[`${rel} banc`] || 0) + 1;
      res.writeHead(200, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' });
      res.end(state.hostConfig);
      return;
    }
    const file = path.resolve(root, '.' + rel);
    if (!file.startsWith(root + path.sep) || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
      state.notFound.push(rel);
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('not found');
      return;
    }
    const headers = { 'Content-Type': TYPES[path.extname(file)] || 'application/octet-stream', 'X-Content-Type-Options': 'nosniff', 'Cache-Control': 'no-store' };
    if (path.extname(file) === '.html') headers['Content-Security-Policy'] = state.policy;
    const accept = String(req.headers['accept-encoding'] || '');
    let body = file;
    for (const [enc, ext] of [['br', '.br'], ['gzip', '.gz']]) {
      if (accept.includes(enc) && fs.existsSync(file + ext)) {
        headers['Content-Encoding'] = enc;
        headers.Vary = 'Accept-Encoding';
        body = file + ext;
        break;
      }
    }
    report.served[`${rel} ${headers['Content-Encoding'] || 'identity'}`] = (report.served[`${rel} ${headers['Content-Encoding'] || 'identity'}`] || 0) + 1;
    res.writeHead(200, headers);
    fs.createReadStream(body).pipe(res);
  };
}

function parentHandler(engineOrigin) {
  return (req, res) => {
    const p = new URL(req.url, 'http://x').pathname;
    if (p === '/') {
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store',
        'Content-Security-Policy': `default-src 'none'; script-src 'self'; frame-src ${engineOrigin}; img-src data:; style-src 'unsafe-inline'` });
      res.end(parentPage(engineOrigin));
    } else if (p === '/harness.js') {
      res.writeHead(200, { 'Content-Type': 'text/javascript; charset=utf-8', 'Cache-Control': 'no-store' });
      res.end(HARNESS_JS);
    } else {
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('not found');
    }
  };
}

// Protocole DevTools sur tube (fd 3 : vers Chrome, fd 4 : depuis Chrome), comme smoke.mjs.
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

class DocsBench {
  constructor(c, page, sessions) {
    Object.assign(this, { c, page, sessions });
    this.log = [];
    this.caps = {};
  }

  async evaluate(session, expression) {
    const r = await this.c.send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }, session);
    if (r.exceptionDetails) throw new Error(`évaluation : ${r.exceptionDetails.text} ${(r.exceptionDetails.exception || {}).description || ''}`);
    return r.result.value;
  }

  async refresh() {
    try {
      const v = JSON.parse(await this.evaluate(this.page, 'JSON.stringify(window.__docs ? __docs.log : null)'));
      if (v) this.log = v;
    } catch {
      // Page parente en cours de chargement : relue au tour suivant.
    }
    return this.log;
  }

  messages(doc) {
    return this.log.filter((e) => e.from === doc && e.plain);
  }

  async waitFor(doc, pred, ms) {
    const end = Date.now() + ms;
    for (;;) {
      await this.refresh();
      const hit = this.messages(doc).find((e) => pred(e.msg));
      if (hit || Date.now() > end) return hit ? hit.msg : null;
      await sleep(200);
    }
  }

  async send(doc, type, payload, re) {
    return this.evaluate(this.page, `__docs.send(${JSON.stringify(doc)}, ${JSON.stringify(type)}, ${JSON.stringify(payload)}, ${JSON.stringify(re || null)})`);
  }

  // Une demande et sa réponse evora:ack (null si absente au terme de ms).
  async request(doc, type, payload, ms) {
    const id = await this.send(doc, type, payload);
    const ack = await this.waitFor(doc, (m) => m.type === 'evora:ack' && m.re === id, ms);
    return { id, ack };
  }

  async storage(label) {
    const st = JSON.parse(await this.evaluate(this.sessions.un, STORAGE));
    report.storage.push({ label, ...st });
    return st;
  }

  // Attendu au plus ms : pred(stockage) vrai.
  async storageUntil(label, pred, ms) {
    const end = Date.now() + ms;
    let st;
    do {
      st = await this.storage(label);
      if (pred(st)) return st;
      await sleep(500);
    } while (Date.now() < end);
    return st;
  }

  async init(doc, ns) {
    const ready = await this.waitFor(doc, (m) => m.type === 'evora:ready', 90000);
    if (!ready) {
      fail('ready', `aucun evora:ready du document ${doc}`);
      return false;
    }
    this.caps[doc] = (ready.payload && ready.payload.capabilities) || [];
    const id = await this.send(doc, 'evora:init', initPayload(ns, this.caps[doc]), ready.id);
    const ack = await this.waitFor(doc, (m) => m.type === 'evora:ack' && m.re === id, 120000);
    if (!ack || !ack.payload || ack.payload.ok !== true) {
      fail('init', `accusé de evora:init du document ${doc} absent ou négatif : ${JSON.stringify(ack)}`);
      return false;
    }
    log(`[docs] ${doc} : evora:init accusé (espace ${ns.slice(0, 4)}…)`);
    return true;
  }

  async theme(doc, theme) {
    const { ack } = await this.request(doc, 'evora:theme', { theme, accent: '#4F46E5' }, 15000);
    if (!ack || ack.payload.ok !== true) fail('theme', `accusé de evora:theme ${theme} du document ${doc} absent ou négatif : ${JSON.stringify(ack)}`);
  }

  async run() {
    for (const doc of DOCS) {
      if (!(await this.waitFor(doc, (m) => m.type === 'evora:ready', 90000))) {
        fail('ready', `aucun evora:ready du document ${doc}`);
        return false;
      }
    }
    await this.evaluate(this.sessions.un, SEED_OLD);
    const seeded = await this.storage('reste posé');
    await checkpoint('depart:banc', () => {
      if (seeded.dbs === null) fail('bench', 'bases illisibles par le banc');
      else if (!seeded.dbs.includes(`${NS_OLD}:${PRESETS}`) || !seeded.keys.includes(`${NS_OLD}:${PREFS}`)) fail('bench', `reste de l'ancien compte non posé : ${JSON.stringify(seeded)}`);
    });
    // 1. Compte A dans « un », vivant, préférences enregistrées.
    if (!(await this.init('un', NS_A))) return false;
    await this.theme('un', 'dark');
    const alive = await this.storageUntil('A vivant', (st) => st.themeA === 'evora' && (st.dbs || []).includes(`${NS_A}:${PRESETS}`), 10000);
    await checkpoint('a:banc', () => {
      if (alive.themeA !== 'evora' || !(alive.dbs || []).includes(`${NS_A}:${PRESETS}`)) {
        fail('bench', `le document du compte A n'a ni base ni préférences : scénario sans objet : ${JSON.stringify(alive)}`);
      }
    });
    // Compte B dans « deux » : relevé juste après l'accusé de son evora:init.
    if (!(await this.init('deux', NS_B))) return false;
    const afterInit = await this.storage('juste après l\'init de B');
    await checkpoint('b-init:base', () => {
      const left = [`${NS_A}:${PRESETS}`, `${NS_OLD}:${PRESETS}`].filter((n) => (afterInit.dbs || []).includes(n))
        .concat([`${NS_A}:${PRESETS}`, `${NS_OLD}:${PRESETS}`].filter((k) => afterInit.keys.includes(k)).map((k) => `marqueur ${k}`));
      if (left.length) fail('base', `bases d'autres comptes encore là juste après l'accusé de evora:init du compte B (document du compte A ouvert) : ${left.join(', ')}`);
    });
    await checkpoint('b-init:propre', () => {
      if (!(afterInit.dbs || []).includes(`${NS_B}:${PRESETS}`)) fail('bench', `le compte B n'a pas de base après son init : ${JSON.stringify(afterInit)}`);
    });
    // Le document du compte A ne réécrit plus rien.
    await this.theme('un', 'light');
    await sleep(2500);
    const aLater = await this.storage('A après une écriture tentée');
    await checkpoint('a-ecriture:cle', () => {
      const back = [`${NS_A}:${PREFS}`, `${NS_A}:${PRESETS}`].filter((k) => aLater.keys.includes(k));
      if (back.length) fail('reecriture', `le document du compte A a réécrit après l'init du compte B : ${back.join(', ')}`);
    });
    // 2. Second document du compte B (« trois »), préférences enregistrées, base ouverte.
    if (!(await this.init('trois', NS_B))) return false;
    await this.theme('trois', 'dark');
    const twoOpen = await this.storageUntil('B dans deux documents', (st) => st.themeB === 'evora' && (st.dbs || []).includes(`${NS_B}:${PRESETS}`), 10000);
    await checkpoint('deux:banc', () => {
      if (twoOpen.themeB !== 'evora' || !(twoOpen.dbs || []).includes(`${NS_B}:${PRESETS}`)) {
        fail('bench', `le second document du compte B n'a ni base ni préférences : ${JSON.stringify(twoOpen)}`);
      }
    });
    const t0 = Date.now();
    const purge = await this.request('deux', 'evora:purge', { scope: 'current' }, PURGE_WAIT_MS);
    log(`[docs] evora:purge dans deux : ${JSON.stringify(purge.ack)} en ${Date.now() - t0} ms`);
    await checkpoint('purge-deux:accuse', () => {
      if (!purge.ack || purge.ack.payload.ok !== true) {
        fail('purge-accuse', `evora:purge {current} non accusé positivement en ${PURGE_WAIT_MS} ms alors qu'un second document du compte tient la base : ${JSON.stringify(purge.ack)}`);
      }
    });
    const purged = await this.storage('juste après la purge de B');
    await checkpoint('purge-deux:restes', () => {
      const left = [`${NS_B}:${PREFS}`, `${NS_B}:${PRESETS}`].filter((k) => purged.keys.includes(k)).map((k) => `clé ${k}`)
        .concat((purged.dbs || []).filter((n) => n === `${NS_B}:${PRESETS}`).map((n) => `base ${n}`));
      if (left.length) fail('purge-restes', `stockage du compte B encore là juste après la réponse à evora:purge : ${left.join(', ')}`);
    });
    await this.theme('trois', 'light');
    await sleep(2500);
    const bLater = await this.storage('trois après une écriture tentée');
    await checkpoint('deux-ecriture:cle', () => {
      const back = [`${NS_B}:${PREFS}`, `${NS_B}:${PRESETS}`].filter((k) => bLater.keys.includes(k));
      if (back.length) fail('reecriture', `le second document du compte B a réécrit après la purge : ${back.join(', ')}`);
    });
    return true;
  }

  // Enveloppes reçues des trois documents : un seul evora:ready chacun, accusés conformes, aucun échec.
  verify() {
    for (const doc of DOCS) {
      const msgs = this.messages(doc).map((e) => e.msg);
      const readies = msgs.filter((m) => m.type === 'evora:ready');
      if (readies.length !== 1 || (readies[0].payload || {}).app !== 'lumbra') fail('ready', `${doc} : evora:ready reçu ${readies.length} fois, ou non conforme`);
      for (const m of msgs) {
        if (m.v !== 1 || !ULID_RE.test(m.id || '')) fail('envelope', `${doc} : enveloppe non conforme : ${JSON.stringify(m).slice(0, 200)}`);
        if (m.type === 'evora:failed') fail('failed', `${doc} : le moteur a signalé un échec : ${JSON.stringify(m.payload)}`);
        if (m.type === 'evora:ack') {
          const p = m.payload || {};
          if (!ULID_RE.test(m.re || '') || typeof p.ok !== 'boolean' || (p.code !== undefined && !ACK_CODES.includes(p.code)) || (p.ok === false && p.code === undefined)) {
            fail('envelope', `${doc} : evora:ack non conforme : ${JSON.stringify(m).slice(0, 200)}`);
          }
        }
      }
    }
  }
}

async function main() {
  const state = { notFound: [], policy: '', hostConfig: '' };
  const engine = await listen(engineHandler(state));
  const engineOrigin = `http://127.0.0.1:${engine.address().port}`;
  const parent = await listen(parentHandler(engineOrigin));
  const parentOrigin = `http://localhost:${parent.address().port}`;
  state.hostConfig = JSON.stringify({ parentOrigins: [parentOrigin] });
  state.policy = policyFor(parentOrigin);
  log(`scénario : pont-documents (variante ${VARIANT}${SABOTAGE ? `, témoin ${SABOTAGE}` : ''}) ; moteur : ${engineOrigin} ; parent : ${parentOrigin}`);

  const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'evora-smoke-docs-'));
  const chrome = spawn(CHROME, [
    '--headless=new', '--no-sandbox', '--remote-debugging-pipe', `--user-data-dir=${profile}`,
    '--no-first-run', '--no-default-browser-check', '--disable-extensions', '--disable-background-networking',
    '--site-per-process', '--window-size=1400,1300',
    '--enable-unsafe-swiftshader', '--use-angle=swiftshader', '--ignore-gpu-blocklist', 'about:blank',
  ], { stdio: ['ignore', 'ignore', 'pipe', 'pipe', 'pipe'] });
  let stderr = '';
  chrome.stderr.on('data', (d) => { stderr += d.toString(); });
  const c = cdp(chrome);
  const version = await c.send('Browser.getVersion');
  report.chrome = version.product;
  log(`navigateur : ${version.product}`);

  const sessions = {};
  const docOf = {};
  const frames = [];
  const isAllowed = (url, text) => ALLOWED_404.some((p) => (url || '').endsWith(p)) && /404/.test(text || '');
  c.on((msg) => {
    const p = msg.params || {};
    const where = docOf[msg.sessionId] || 'parent';
    switch (msg.method) {
      case 'Target.attachedToTarget': {
        // Les documents moteur (origine A, autre site) sont des iframes hors processus ; leur URL
        // n'est pas encore connue à l'attache : chacun est identifié ensuite par son ?doc=.
        if (p.targetInfo.type === 'iframe' && !frames.includes(p.sessionId)) {
          frames.push(p.sessionId);
          docOf[p.sessionId] = `iframe ${frames.length}`;
          (async () => {
            await c.send('Runtime.enable', {}, p.sessionId);
            await c.send('Log.enable', {}, p.sessionId);
            await c.send('Page.enable', {}, p.sessionId);
            for (const source of ENGINE_SCRIPTS) await c.send('Page.addScriptToEvaluateOnNewDocument', { source }, p.sessionId);
            await c.send('Runtime.runIfWaitingForDebugger', {}, p.sessionId);
          })().catch((e) => report.errors.push(`attache d'un document moteur : ${e.message}`));
        } else if (p.waitingForDebugger) {
          c.send('Runtime.runIfWaitingForDebugger', {}, p.sessionId).catch(() => {});
        }
        break;
      }
      case 'Runtime.exceptionThrown': {
        const d = p.exceptionDetails || {};
        report.errors.push(`${where} : exception ${(d.exception && d.exception.description) || d.text} (${d.url || ''}:${d.lineNumber})`);
        break;
      }
      case 'Runtime.consoleAPICalled': {
        const text = (p.args || []).map((a) => a.value ?? a.description ?? '').join(' ');
        if (p.type === 'error' || p.type === 'assert') report.errors.push(`${where} : console.${p.type} ${text}`);
        else if (p.type === 'warning') report.warnings.push(`${where} : ${text}`.slice(0, 300));
        else if (report.console.length < 80) report.console.push(`${where} : ${text}`.slice(0, 300));
        break;
      }
      case 'Log.entryAdded': {
        const e = p.entry || {};
        if (e.level === 'error') {
          const item = `${where} : [${e.source}] ${e.text} ${e.url || ''}`.trim();
          if (isAllowed(e.url, e.text)) report.allowed.push(item);
          else report.errors.push(item);
        }
        break;
      }
      case 'Target.targetCrashed':
      case 'Inspector.targetCrashed':
        report.errors.push(`${where} : plantage de la cible`);
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
  await c.send('Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: true, flatten: true }, page);
  await c.send('Page.navigate', { url: `${parentOrigin}/` }, page);

  const end = Date.now() + 60000;
  while (DOCS.some((d) => !sessions[d]) && Date.now() < end) {
    await sleep(300);
    for (const s of frames.filter((f) => !Object.values(sessions).includes(f))) {
      try {
        const r = await c.send('Runtime.evaluate', { expression: "new URLSearchParams(location.search).get('doc')", returnByValue: true }, s);
        const doc = r.result && r.result.value;
        if (DOCS.includes(doc) && !sessions[doc]) {
          sessions[doc] = s;
          docOf[s] = doc;
        }
      } catch {
        // Document pas encore chargé : relu au tour suivant.
      }
    }
  }
  const bench = new DocsBench(c, page, sessions);
  let completed = false;
  if (DOCS.some((d) => !sessions[d])) {
    fail('bench', `documents moteur non attachés : ${DOCS.filter((d) => !sessions[d]).join(', ')}`);
  } else {
    try {
      completed = await bench.run();
    } catch (e) {
      fail('bench', `scénario interrompu : ${e.message}`);
    }
    await bench.refresh();
    bench.verify();
  }
  report.exchanges = bench.log.map((e) => `${e.from} ${e.msg && e.msg.type}${e.msg && e.msg.re ? ` (re …${e.msg.re.slice(-4)})` : ''} ${e.msg && e.msg.payload ? JSON.stringify(e.msg.payload).slice(0, 120) : ''}`);
  report.notFound = state.notFound;

  try { await c.send('Browser.close'); } catch { /* Chrome ferme le tube */ }
  setTimeout(() => chrome.kill('SIGKILL'), 5000).unref();
  engine.close();
  parent.close();

  const stray404 = state.notFound.filter((p) => !ALLOWED_404.includes(p));
  const all = [...report.errors, ...problems, ...(stray404.length ? [`404 inattendus : ${stray404.join(', ')}`] : [])];
  report.problems = all;
  report.checks = checks;
  report.ok = all.length === 0 && completed;
  if (!completed && !all.length) all.push('scénario inachevé');
  if (SABOTAGE) {
    const w = witness({ mode: SABOTAGE, problems: all, sabotaged: [], checks, started: completed });
    report.witness = w;
    report.ok = w.ok;
    all.push(w.ok
      ? `TÉMOIN SABORDÉ « ${SABOTAGE} » VALIDE : ${w.seen.length} détecteur(s) rougi(s), chacun à sa place et seuls (${w.seen.join(', ')}) : le banc sait rougir`
      : `TÉMOIN SABORDÉ « ${SABOTAGE} » INVALIDE : ${w.reasons.join(' ; ')}`);
  }
  if (!report.ok && stderr) report.chromeStderr = stderr.split('\n').slice(-40);
  fs.writeFileSync(reportFile, JSON.stringify(report, null, 1));

  const lines = [
    `### Test de fumée : pont-documents (variante ${VARIANT}${SABOTAGE ? `, témoin ${SABOTAGE}` : ''})`, '',
    '| Élément | Valeur |', '| --- | --- |',
    `| Navigateur | ${report.chrome} |`,
    `| Documents moteur | ${DOCS.join(', ')} (\`?host=evora\`, origine \`${engineOrigin}\`) |`,
    `| Contrôles nommés | ${checks.map((k) => `${k.label} : ${k.tags.join(', ') || 'vert'}`).join(' ; ')} |`,
    `| Verdict | ${report.witness ? (report.ok ? 'témoin VALIDE (rouge attendu, exact)' : 'témoin INVALIDE') : (report.ok ? 'vert' : 'ROUGE')} |`, '',
    ...all.map((p) => `- ${p}`), '',
  ];
  const text = lines.join('\n');
  console.log(text);
  for (const w of report.warnings.slice(0, 30)) console.log(`avertissement : ${w}`);
  for (const m of report.console.slice(0, 60)) console.log(`console : ${m}`);
  for (const m of report.exchanges) console.log(`ehp : ${m}`);
  for (const st of report.storage) console.log(`stockage : ${JSON.stringify(st)}`);
  if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, text + '\n');
  process.exit(report.ok ? 0 : 1);
}

main().catch((e) => {
  report.problems = [`échec du test : ${e.stack || e}`];
  try { fs.writeFileSync(reportFile, JSON.stringify(report, null, 1)); } catch { /* rien */ }
  console.error(e);
  process.exit(1);
});
