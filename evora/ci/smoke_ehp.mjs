// Scénario de fumée « pont-ehp » du build web de Lumbra (patch E4, HOST-PROTOCOL.md d'Evora).
//
// Importé par evora/ci/smoke.mjs quand SMOKE_SCENARIO=pont-ehp. La page parente (origine B) est une
// coquille minimale : elle encadre le moteur (origine A, ?host=evora) et deux fenêtres témoins, l'une
// d'une troisième origine C (non admise), l'autre de l'origine B elle-même mais distincte de la page
// parente. Le banc sert evora-host.json comme le ferait le provisionnement d'Evora, avec la seule
// origine B admise. Tout ce que la page parente reçoit est journalisé ; le banc pilote les échanges
// par le protocole DevTools et vérifie, côté coquille :
//   - un seul evora:ready conforme (app lumbra, ehp 1.x, capacités connues), avant tout démarrage ;
//   - aucune réponse à un evora:init de version 2, ni à un evora:init dont l'enveloppe a un champ en
//     trop, ni aux messages des deux témoins (origine non admise ; même origine, autre fenêtre) ;
//   - l'accusé positif du vrai evora:init, puis le démarrage du moteur ;
//   - evora:open d'une petite image PNG reçue en octets : accusé positif, puis evora:dirty false ;
//   - une modification (Ctrl+A, Sélection › Tout) : evora:dirty true ; evora:request-save : un
//     evora:save conforme (re = la demande, docRef, nom .pcraft, octets non vides d'une archive zip),
//     puis evora:dirty false ; une seconde demande : evora:ack {ok: true, code: "clean"} ; Ctrl+S
//     (action de l'utilisateur) : un evora:save sans re ;
//   - evora:theme et evora:lang : accusés positifs, thème et langue enregistrés par le moteur ;
//   - aucune enveloppe hors norme (champ inconnu, version, type, ids, charge utile) et aucun
//     evora:failed ; chaque capacité annoncée est exercée par ce scénario.
// Chaque contrôle porte une étiquette [ehp:…] dans la liste des problèmes du rapport.
import zlib from 'node:zlib';

export const SCENARIO = 'pont-ehp';
// Référence du document ouvert par la coquille et espace de noms du compte (valeurs de banc).
const DOC_REF = '01JZZSMKEDCQMENTREF0000001';
const STORAGE_NS = 'ab'.repeat(32);
// Plafond transmis dans evora:init : valeur de banc (pas un plafond du produit, D-15), assez large
// pour la petite image et son document de travail.
const BENCH_MAX_BYTES = 8 * 1024 * 1024;
const ULID_RE = /^[0-9A-HJKMNP-TV-Z]{26}$/;
const CAPABILITY_NAMES = ['doc.open', 'doc.save', 'doc.saved-ack', 'doc.dirty', 'doc.multi', 'doc.request-save',
  'export.drive', 'media.need', 'media.ranges', 'pick.drive', 'command', 'storage.ns', 'storage.memory',
  'storage.purge', 'live.lang', 'live.theme', 'font.inject', 'author'];
// Capacités que ce scénario exerce : le moteur ne doit en annoncer aucune autre.
const EXERCISED = ['doc.open', 'doc.save', 'doc.dirty', 'doc.request-save', 'live.lang', 'live.theme'];
const ACK_CODES = ['clean', 'cancelled', 'conflict', 'too_large', 'quota', 'forbidden', 'not_found', 'invalid_content',
  'unsupported', 'rate_limited', 'read_only', 'busy', 'capability', 'network', 'server', 'timeout'];
const FAIL_CODES = ['no_webgl2', 'wasm', 'memory', 'panic', 'unsupported_browser', 'protocol', 'init'];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Une image PNG RGBA de 32 × 32 (indigo), écrite ici : aucun fichier binaire dans le dépôt.
function png() {
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (buf) => {
    let c = 0xffffffff;
    for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
    const sum = Buffer.alloc(4);
    sum.writeUInt32BE(crc(body));
    return Buffer.concat([len, body, sum]);
  };
  const size = 32;
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr.set([8, 6, 0, 0, 0], 8);
  const rows = [];
  for (let y = 0; y < size; y++) {
    rows.push(Buffer.from([0]));
    for (let x = 0; x < size; x++) rows.push(Buffer.from([0x4f, 0x46, 0xe5, 0xff]));
  }
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(Buffer.concat(rows))), chunk('IEND', Buffer.alloc(0))]);
}

export function hostConfig(parentOrigin) {
  return JSON.stringify({ parentOrigins: [parentOrigin] });
}

export function parentPage(engineSrc, foreignOrigin, parentOrigin) {
  const w = (origin) => `${origin}/witness.html?engine=${encodeURIComponent(new URL(engineSrc).origin)}&parent=${encodeURIComponent(parentOrigin)}`;
  return `<!doctype html><html lang="fr"><head><meta charset="utf-8"><title>Evora : coquille minimale du test de fumée</title>
<link rel="icon" href="data:,"></head><body style="margin:0;background:#fff">
<iframe id="engine" src="${engineSrc}" width="1280" height="800" style="border:0;display:block"
 sandbox="allow-scripts allow-same-origin" allow="fullscreen; clipboard-read; clipboard-write"></iframe>
<iframe id="foreign" src="${w(foreignOrigin)}" width="8" height="8" style="border:0"></iframe>
<iframe id="sibling" src="${w(parentOrigin)}" width="8" height="8" style="border:0"></iframe>
<script src="/harness.js"></script>
</body></html>`;
}

export function parentCsp(engineOrigin, foreignOrigin) {
  return `default-src 'none'; script-src 'self'; frame-src ${engineOrigin} ${foreignOrigin} 'self'; img-src data:; style-src 'unsafe-inline'`;
}

export const WITNESS_HTML = `<!doctype html><html><head><meta charset="utf-8"><title>témoin</title></head><body>
<script src="/witness.js"></script></body></html>`;
export const WITNESS_CSP = "default-src 'none'; script-src 'self'";

// Fenêtre témoin : sur ordre de la page parente, poste un message au moteur (frames[0] du parent)
// depuis sa propre fenêtre, puis confirme l'envoi à la page parente.
export const WITNESS_JS = `(() => {
  const q = new URLSearchParams(location.search);
  const engine = q.get('engine');
  const parentOrigin = q.get('parent');
  addEventListener('message', (e) => {
    if (e.source !== parent || e.origin !== parentOrigin || !e.data || e.data.cmd !== 'send') return;
    parent.frames[0].postMessage(e.data.msg, engine);
    parent.postMessage({ witnessSent: e.data.msg.id }, parentOrigin);
  });
})();`;

// Coquille minimale : journal de tout ce qui arrive, envoi d'enveloppes EHP vers le moteur et ordres
// aux témoins. Les octets sont résumés (taille, quatre premiers octets) pour le journal.
export function harnessJs() {
  const fixture = png().toString('base64');
  return `(() => {
  const engine = document.getElementById('engine');
  const ENGINE = new URL(engine.src).origin;
  const A = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
  const ulid = () => {
    let t = Date.now(), s = '';
    for (let i = 0; i < 10; i++) { s = A[t % 32] + s; t = Math.floor(t / 32); }
    for (const b of crypto.getRandomValues(new Uint8Array(16))) s += A[b % 32];
    return s;
  };
  const summarize = (v) => JSON.parse(JSON.stringify(v, (k, x) => {
    if (x instanceof ArrayBuffer) {
      const head = Array.from(new Uint8Array(x, 0, Math.min(4, x.byteLength))).map((b) => b.toString(16).padStart(2, '0')).join('');
      return { $arrayBuffer: x.byteLength, head };
    }
    if (typeof SharedArrayBuffer !== 'undefined' && x instanceof SharedArrayBuffer) return { $shared: x.byteLength };
    if (ArrayBuffer.isView(x)) return { $view: x.constructor.name };
    return x;
  }));
  const log = [];
  const sent = [];
  addEventListener('message', (e) => {
    const d = e.data;
    const plain = d !== null && typeof d === 'object' && Object.getPrototypeOf(d) === Object.prototype;
    const entry = { at: Math.round(performance.now()), origin: e.origin, fromEngine: e.source === engine.contentWindow, plain,
      keys: plain ? Object.keys(d) : null };
    try { entry.msg = plain ? summarize(d) : String(d).slice(0, 200); } catch (err) { entry.msg = 'illisible'; }
    log.push(entry);
  });
  window.__ehp = {
    log, sent,
    send(type, payload, opts) {
      const o = opts || {};
      const id = ulid();
      const msg = { v: o.v === undefined ? 1 : o.v, type, id };
      if (o.re) msg.re = o.re;
      msg.payload = payload;
      if (o.extraField) msg.extra = true;
      const transfer = payload && payload.bytes instanceof ArrayBuffer ? [payload.bytes] : [];
      engine.contentWindow.postMessage(msg, ENGINE, transfer);
      sent.push({ id, type, v: msg.v, re: msg.re || null, via: 'parent', extraField: !!o.extraField });
      return id;
    },
    witness(which, type, payload, re) {
      const frame = document.getElementById(which);
      const id = ulid();
      const msg = { v: 1, type, id, payload };
      if (re) msg.re = re;
      frame.contentWindow.postMessage({ cmd: 'send', msg }, new URL(frame.src).origin);
      sent.push({ id, type, v: 1, re: re || null, via: which });
      return id;
    },
    png() { return Uint8Array.from(atob('${fixture}'), (c) => c.charCodeAt(0)).buffer; },
  };
})();`;
}

// Clavier synthétique vers le canvas d'eframe (raccourci de l'utilisateur), dans le document moteur.
const shortcut = (key) => `(() => {
  const c = document.getElementById('photocraft_canvas');
  c.focus();
  const init = { key: '${key}', code: 'Key${key.toUpperCase()}', ctrlKey: true, bubbles: true, cancelable: true };
  c.dispatchEvent(new KeyboardEvent('keydown', init));
  c.dispatchEvent(new KeyboardEvent('keyup', init));
  return document.activeElement === c;
})()`;

function initPayload(capabilities) {
  return { lang: 'fr', engineLang: 'fr', theme: 'light', accent: '#4F46E5', readOnly: false, storageNs: STORAGE_NS,
    capabilities, prefs: {}, limits: { maxDocumentBytes: BENCH_MAX_BYTES, maxExportBytes: BENCH_MAX_BYTES } };
}

export class EhpBench {
  constructor({ c, page, frame, engineOrigin, parentOrigin, log }) {
    Object.assign(this, { c, page, frame, engineOrigin, parentOrigin, log });
    this.ids = {};
    this.problems = [];
    this.state = { log: [], sent: [] };
    this.readyCaps = [];
  }

  fail(tag, text) {
    this.problems.push(`[ehp:${tag}] ${text}`);
  }

  async evaluate(session, expression, userGesture = false) {
    const r = await this.c.send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true, userGesture }, session);
    if (r.exceptionDetails) throw new Error(`évaluation : ${r.exceptionDetails.text} ${(r.exceptionDetails.exception || {}).description || ''}`);
    return r.result.value;
  }

  async refresh() {
    try {
      const v = await this.evaluate(this.page, 'JSON.stringify(window.__ehp ? { log: __ehp.log, sent: __ehp.sent } : null)');
      if (v) this.state = JSON.parse(v);
    } catch {
      // Page parente en cours de chargement : le journal sera relu au tour suivant.
    }
    return this.state;
  }

  engineMessages() {
    return this.state.log.filter((e) => e.fromEngine);
  }

  // Une condition sur le journal, attendue au plus ms millisecondes.
  async until(cond, ms) {
    const end = Date.now() + ms;
    for (;;) {
      await this.refresh();
      if (cond()) return true;
      if (Date.now() > end) return false;
      await sleep(200);
    }
  }

  // Un message du moteur qui satisfait pred, attendu au plus ms millisecondes.
  async waitEngine(pred, ms) {
    const end = Date.now() + ms;
    for (;;) {
      await this.refresh();
      const hit = this.engineMessages().find((e) => e.plain && pred(e.msg));
      if (hit || Date.now() > end) return hit || null;
      await sleep(200);
    }
  }

  async send(type, payload, opts) {
    return this.evaluate(this.page, `__ehp.send(${JSON.stringify(type)}, ${payload}, ${JSON.stringify(opts || {})})`);
  }

  async witness(which, type, payload, re) {
    return this.evaluate(this.page, `__ehp.witness(${JSON.stringify(which)}, ${JSON.stringify(type)}, ${JSON.stringify(payload)}, ${JSON.stringify(re || null)})`);
  }

  response(type, re) {
    return (m) => m.type === type && m.re === re;
  }

  // Avant le démarrage : evora:ready, témoins, puis le vrai evora:init et son accusé.
  async handshake() {
    const ready = await this.waitEngine((m) => m.type === 'evora:ready', 90000);
    if (!ready) {
      this.fail('ready', 'aucun evora:ready reçu par la page parente');
      return false;
    }
    this.ids.ready = ready.msg.id;
    this.readyCaps = (ready.msg.payload && ready.msg.payload.capabilities) || [];
    this.log(`[ehp] evora:ready ${JSON.stringify(ready.msg.payload)}`);
    const init = JSON.stringify(initPayload(this.readyCaps));
    // Témoins, avant le vrai evora:init : un moteur qui en accepterait un serait déjà initialisé.
    this.ids.v2 = await this.send('evora:init', init, { v: 2, re: this.ids.ready });
    this.ids.extraField = await this.send('evora:init', init, { re: this.ids.ready, extraField: true });
    this.ids.foreign = await this.witness('foreign', 'evora:init', initPayload(this.readyCaps), this.ids.ready);
    this.ids.sibling = await this.witness('sibling', 'evora:init', initPayload(this.readyCaps), this.ids.ready);
    const end = Date.now() + 15000;
    while (Date.now() < end) {
      await this.refresh();
      const confirmed = this.state.log.filter((e) => e.msg && e.msg.witnessSent).map((e) => e.msg.witnessSent);
      if (confirmed.includes(this.ids.foreign) && confirmed.includes(this.ids.sibling)) break;
      await sleep(200);
    }
    const confirmed = this.state.log.filter((e) => e.msg && e.msg.witnessSent).map((e) => e.msg.witnessSent);
    for (const w of ['foreign', 'sibling']) {
      if (!confirmed.includes(this.ids[w])) this.fail('witness', `le témoin ${w} n'a pas confirmé son envoi : banc inopérant`);
    }
    await sleep(1500);
    this.ids.init = await this.send('evora:init', init, { re: this.ids.ready });
    const ack = await this.waitEngine(this.response('evora:ack', this.ids.init), 120000);
    if (!ack || !ack.msg.payload || ack.msg.payload.ok !== true) {
      this.fail('init', `accusé de evora:init absent ou négatif : ${JSON.stringify(ack && ack.msg)}`);
      return false;
    }
    this.log('[ehp] evora:init accusé (ok)');
    return true;
  }

  // Ctrl+<key> dans le document moteur, renouvelé tant que pred n'est pas observé.
  async press(key, pred, what) {
    for (let attempt = 1; attempt <= 3; attempt++) {
      const focused = await this.evaluate(this.frame(), shortcut(key), true);
      const hit = await this.waitEngine(pred, 5000);
      if (hit) return hit;
      this.log(`[ehp] Ctrl+${key.toUpperCase()} (essai ${attempt}, canvas focalisé : ${focused}) : ${what} non observé`);
    }
    return null;
  }

  // Après le démarrage : ouverture, modification, enregistrements, thème, langue.
  async session() {
    this.ids.open = await this.send('evora:open', `{ docRef: ${JSON.stringify(DOC_REF)}, name: 'evora-smoke.png', bytes: __ehp.png() }`);
    const openAck = await this.waitEngine(this.response('evora:ack', this.ids.open), 30000);
    if (!openAck || openAck.msg.payload.ok !== true) {
      this.fail('open', `accusé de evora:open absent ou négatif : ${JSON.stringify(openAck && openAck.msg)}`);
      return;
    }
    const clean = await this.waitEngine((m) => m.type === 'evora:dirty' && m.payload.docRef === DOC_REF && m.payload.dirty === false, 10000);
    if (!clean) this.fail('dirty', 'evora:dirty {dirty: false} absent après l\'ouverture');
    const dirty = await this.press('a', (m) => m.type === 'evora:dirty' && m.payload.docRef === DOC_REF && m.payload.dirty === true, 'evora:dirty true');
    if (!dirty) {
      this.fail('dirty', 'evora:dirty {dirty: true} absent après une modification (Ctrl+A)');
      return;
    }
    this.ids.request = await this.send('evora:request-save', `{ reason: 'user', docRef: ${JSON.stringify(DOC_REF)} }`);
    const save = await this.waitEngine(this.response('evora:save', this.ids.request), 30000);
    if (!save) this.fail('save', 'aucun evora:save en réponse à evora:request-save');
    else {
      const p = save.msg.payload || {};
      const b = p.bytes || {};
      if (p.docRef !== DOC_REF || p.name !== 'evora-smoke.pcraft' || p.saveAs !== false || !(b.$arrayBuffer > 0) || b.head !== '504b0304') {
        this.fail('save', `evora:save inattendu (docRef, nom .pcraft, saveAs, octets zip non vides) : ${JSON.stringify(p)}`);
      }
      this.log(`[ehp] evora:save : ${p.name}, ${b.$arrayBuffer} octets, en-tête ${b.head}`);
    }
    if (!(await this.until(() => this.dirtyFalseAfterSave(save), 10000))) this.fail('dirty', 'evora:dirty {dirty: false} absent après l\'enregistrement');
    this.ids.clean = await this.send('evora:request-save', `{ reason: 'autosave' }`);
    const cleanAck = await this.waitEngine(this.response('evora:ack', this.ids.clean), 15000);
    if (!cleanAck || cleanAck.msg.payload.ok !== true || cleanAck.msg.payload.code !== 'clean') {
      this.fail('clean', `evora:request-save sur un document enregistré : attendu evora:ack {ok: true, code: "clean"}, reçu ${JSON.stringify(cleanAck && cleanAck.msg)}`);
    }
    const savesBefore = this.engineMessages().filter((e) => e.msg.type === 'evora:save').length;
    const action = await this.press('s', (m) => m.type === 'evora:save' && !m.re
      && this.engineMessages().filter((e) => e.msg.type === 'evora:save').length > savesBefore, 'evora:save (Ctrl+S)');
    if (!action) this.fail('save-action', 'aucun evora:save après Ctrl+S (Fichier › Enregistrer)');
    else if ((action.msg.payload || {}).docRef !== DOC_REF || !((action.msg.payload.bytes || {}).$arrayBuffer > 0)) {
      this.fail('save-action', `evora:save de Ctrl+S inattendu : ${JSON.stringify(action.msg.payload)}`);
    }
    this.ids.theme = await this.send('evora:theme', `{ theme: 'dark', accent: '#4F46E5' }`);
    this.ids.lang = await this.send('evora:lang', `{ lang: 'de', engineLang: 'de' }`);
    for (const step of ['theme', 'lang']) {
      const ack = await this.waitEngine(this.response('evora:ack', this.ids[step]), 15000);
      if (!ack || ack.msg.payload.ok !== true) this.fail(step, `accusé de evora:${step} absent ou négatif : ${JSON.stringify(ack && ack.msg)}`);
    }
  }

  // Le dernier evora:dirty du document, après l'evora:save de la demande, vaut false.
  dirtyFalseAfterSave(save) {
    if (!save) return false;
    const msgs = this.engineMessages();
    const at = msgs.findIndex((e) => e.msg.id === save.msg.id);
    return msgs.slice(at + 1).some((e) => e.msg.type === 'evora:dirty' && e.msg.payload.dirty === false);
  }

  // Conformité de chaque enveloppe reçue du moteur, et des réponses.
  verify(storage) {
    const msgs = this.engineMessages();
    const ids = new Set();
    for (const e of msgs) {
      const m = e.msg;
      const where = `${(m && m.type) || '?'} ${JSON.stringify(m).slice(0, 200)}`;
      if (e.origin !== this.engineOrigin || !e.plain) { this.fail('envelope', `message hors norme : ${JSON.stringify(e).slice(0, 200)}`); continue; }
      const keys = e.keys || [];
      if (!keys.every((k) => ['v', 'type', 'id', 're', 'payload'].includes(k)) || !['v', 'type', 'id', 'payload'].every((k) => keys.includes(k))) {
        this.fail('envelope', `enveloppe aux champs inattendus (${keys.join(', ')}) : ${where}`);
      }
      if (m.v !== 1) this.fail('envelope', `v ≠ 1 : ${where}`);
      if (!ULID_RE.test(m.id || '') || ids.has(m.id)) this.fail('envelope', `id absent, non ULID ou réutilisé : ${where}`);
      ids.add(m.id);
      if (m.re !== undefined && !ULID_RE.test(m.re)) this.fail('envelope', `re non ULID : ${where}`);
      const p = m.payload;
      const only = (allowed) => p && typeof p === 'object' && Object.keys(p).every((k) => allowed.includes(k));
      switch (m.type) {
        case 'evora:ready':
          if (m.re !== undefined || !only(['app', 'version', 'ehp', 'capabilities']) || p.app !== 'lumbra' || !/^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$/.test(p.version || '')
            || !/^1\.[0-9]+$/.test(p.ehp || '') || !Array.isArray(p.capabilities) || new Set(p.capabilities).size !== p.capabilities.length
            || !p.capabilities.every((x) => CAPABILITY_NAMES.includes(x))) this.fail('ready', `evora:ready non conforme : ${where}`);
          break;
        case 'evora:ack':
          if (!ULID_RE.test(m.re || '') || !only(['ok', 'code', 'result']) || typeof p.ok !== 'boolean' || (p.code !== undefined && !ACK_CODES.includes(p.code))
            || (p.ok === false && p.code === undefined)) this.fail('envelope', `evora:ack non conforme : ${where}`);
          break;
        case 'evora:dirty':
          if (m.re !== undefined || !only(['dirty', 'docRef']) || typeof p.dirty !== 'boolean' || (p.docRef !== undefined && !ULID_RE.test(p.docRef))) {
            this.fail('envelope', `evora:dirty non conforme : ${where}`);
          }
          break;
        case 'evora:save': {
          const b = (p && p.bytes) || {};
          if (!only(['docRef', 'name', 'saveAs', 'bytes']) || !ULID_RE.test(p.docRef || '') || typeof p.name !== 'string' || !p.name.length
            || /[\\/\u0000-\u001f\u007f]/.test(p.name) || typeof p.saveAs !== 'boolean' || !(b.$arrayBuffer >= 1) || b.$shared !== undefined || b.$view !== undefined) {
            this.fail('save-shape', `evora:save non conforme : ${where}`);
          }
          break;
        }
        case 'evora:failed':
          this.fail('failed', `le moteur a signalé un échec : ${where}`);
          if (!only(['code']) || !FAIL_CODES.includes(p.code)) this.fail('envelope', `evora:failed non conforme : ${where}`);
          break;
        default:
          this.fail('envelope', `type inattendu pour ce moteur : ${where}`);
      }
    }
    const readies = msgs.filter((e) => e.msg && e.msg.type === 'evora:ready').length;
    if (readies !== 1) this.fail('ready-count', `evora:ready reçu ${readies} fois (attendu : 1)`);
    const answered = (id) => msgs.filter((e) => e.msg && e.msg.re === id);
    for (const [key, tag, label] of [['v2', 'answer-v2', 'un evora:init de version 2'],
      ['extraField', 'answer-envelope', 'un evora:init à l\'enveloppe augmentée d\'un champ'],
      ['foreign', 'answer-foreign', 'un message d\'une origine non admise'],
      ['sibling', 'answer-source', 'un message de l\'origine admise venu d\'une autre fenêtre']]) {
      if (this.ids[key] && answered(this.ids[key]).length) this.fail(tag, `le moteur a répondu à ${label} : ${JSON.stringify(answered(this.ids[key])[0].msg)}`);
    }
    const announced = this.readyCaps;
    const extra = announced.filter((x) => !EXERCISED.includes(x));
    if (extra.length) this.fail('caps', `capacités annoncées non exercées par ce scénario : ${extra.join(', ')}`);
    const missing = EXERCISED.filter((x) => !announced.includes(x));
    if (missing.length) this.fail('caps', `capacités attendues absentes de evora:ready : ${missing.join(', ')}`);
    if (storage) {
      if (storage.theme !== 'evora') this.fail('theme', `thème enregistré après evora:theme dark : ${storage.theme}, attendu evora`);
      if (storage.language !== 'de') this.fail('lang', `langue enregistrée après evora:lang de : ${storage.language}, attendue de`);
    }
    return this.problems;
  }

  summary() {
    return this.engineMessages().map((e) => {
      const m = e.msg || {};
      const p = m.payload ? JSON.stringify(m.payload).slice(0, 120) : '';
      return `${m.type}${m.re ? ` (re …${m.re.slice(-4)})` : ''} ${p}`;
    });
  }
}
