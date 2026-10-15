// Autotest, sans navigateur, du verdict des témoins sabordés du banc pont-ehp (smoke_ehp.mjs).
//
// Usage : node evora/ci/witness_selftest.mjs (code 0 : chaque cas a le verdict attendu).
//
// Un témoin dont le verdict ne sait pas dire INVALIDE ne prouve rien : chaque façon de mentir
// (rouge manquant, rouge crédité par le sabotage d'un autre, débordement sur un contrôle non visé,
// problème hors banc, inventaire incomplet ou rejoué, moteur non démarré, rouge au mauvais contrôle)
// est fabriquée ici et doit rendre INVALIDE ; les trois témoins sains doivent rendre VALIDE. Le
// constat du pont muet (muteProblems) est éprouvé de même, cas sain et chaque défaut.
import { BANC_SABOTAGES, MOTEUR_REDS, MUET_REDS, muteProblems, tagOf, witness } from './smoke_ehp.mjs';

// Contrôles nommés du banc (storagePhases) qu'aucun témoin ne vise : ils doivent rester verts.
const QUIET_CHECKS = ['init:banc', 'init:opfs', 'init:caches', 'init:session', 'blocage:banc', 'après-accusé:accusé',
  'après-accusé:autre-moteur', 'après-accusé:opfs', 'après-accusé:caches', 'après-accusé:session', 'après-écriture:thème'];
const ID = (i) => `01JZZTEMOIN${String(i).padStart(15, '0')}`;

function banc() {
  const sabotaged = BANC_SABOTAGES.map((s, i) => ({ tag: s.tag, kind: s.kind, at: s.at || null, what: 'cas', id: s.kind === 'message' ? ID(i) : null }));
  const problems = sabotaged.map((e) => `[${e.tag}] rouge fabriqué${e.id ? ` id ${e.id}` : ''}`);
  const labels = new Set(BANC_SABOTAGES.filter((s) => s.at).map((s) => s.at));
  const checks = [...BANC_SABOTAGES.filter((s) => s.at).map((s) => ({ label: s.at, tags: [s.tag] })),
    ...QUIET_CHECKS.filter((l) => !labels.has(l)).map((label) => ({ label, tags: [] }))];
  return { mode: 'banc', problems, sabotaged, checks, started: true };
}

function moteur() {
  const labels = new Set(MOTEUR_REDS.map((s) => s.at));
  const checks = [...MOTEUR_REDS.map((s) => ({ label: s.at, tags: [s.tag] })),
    ...['init:autre-moteur', 'après-écriture:opfs', ...QUIET_CHECKS].filter((l) => !labels.has(l)).map((label) => ({ label, tags: [] }))];
  return { mode: 'moteur', problems: MOTEUR_REDS.map((s) => `[${s.tag}] rouge`), sabotaged: [], checks, started: true };
}

function muet() {
  return { mode: 'muet', problems: MUET_REDS.map((s) => `[${s.tag}] rouge`), sabotaged: [], checks: [], started: false };
}

const clone = (o) => JSON.parse(JSON.stringify(o));
const firstMessage = (c) => c.sabotaged.find((e) => e.kind === 'message');
const cases = [
  ['banc sain', banc(), true],
  ['banc : un rouge attendu manque', ((c) => { c.problems = c.problems.filter((p) => tagOf(p) !== 'ehp:purge-left'); return c; })(banc()), false],
  ['banc : rouge d\'un message qui ne cite pas le message fabriqué', ((c) => {
    const e = firstMessage(c);
    c.problems = c.problems.map((p) => (p.includes(e.id) ? p.replace(e.id, ID(99)) : p));
    return c;
  })(banc()), false],
  ['banc : contrôle visé rougi par deux détecteurs', ((c) => { c.checks.find((k) => k.label === 'init:espace').tags.push('ehp:purge-witness'); return c; })(banc()), false],
  ['banc : contrôle visé resté vert', ((c) => { c.checks.find((k) => k.label === 'après-accusé:restes').tags = []; return c; })(banc()), false],
  ['banc : contrôle non visé rougi', ((c) => {
    c.checks.find((k) => k.label === 'init:banc').tags = ['ehp:bench'];
    c.problems.push('[ehp:bench] rouge du banc');
    return c;
  })(banc()), false],
  // Remise en état ratée : le même détecteur rougit encore à un contrôle non visé (seule la règle
  // du débordement le voit : l'étiquette est attendue et créditée ailleurs).
  ['banc : sabotage débordant sur un contrôle non visé du même détecteur', ((c) => {
    c.checks.find((k) => k.label === 'après-accusé:autre-moteur').tags = ['ehp:foreign-names'];
    c.problems.push('[ehp:foreign-names] après la purge : effacé');
    return c;
  })(banc()), false],
  ['banc : rouge hors attente', ((c) => { c.problems.push('[ehp:caps] capacité'); return c; })(banc()), false],
  ['banc : problème hors banc', ((c) => { c.problems.push('moteur : console.error panne'); return c; })(banc()), false],
  ['banc : sabotage non joué', ((c) => { c.sabotaged = c.sabotaged.filter((e) => e.tag !== 'ehp:opfs'); return c; })(banc()), false],
  ['banc : sabotage joué deux fois', ((c) => { c.sabotaged.push(clone(c.sabotaged[0])); return c; })(banc()), false],
  ['banc : sabotage hors inventaire', ((c) => { c.sabotaged.push({ tag: 'ehp:inconnu', kind: 'stockage', at: null, what: 'cas', id: null }); return c; })(banc()), false],
  ['banc : contrôle exécuté deux fois', ((c) => { c.checks.push({ label: 'init:espace', tags: ['ehp:namespace'] }); return c; })(banc()), false],
  ['banc : moteur non démarré', ((c) => { c.started = false; return c; })(banc()), false],
  ['moteur sain', moteur(), true],
  ['moteur : un rouge attendu manque', ((c) => { c.problems.pop(); return c; })(moteur()), false],
  ['moteur : rouge hors attente', ((c) => { c.problems.push('[ehp:foreign-names] effacé'); return c; })(moteur()), false],
  ['moteur : rouge au mauvais contrôle', ((c) => {
    c.checks.find((k) => k.label === 'init:espace').tags = [];
    c.checks.find((k) => k.label === 'init:autre-moteur').tags = ['ehp:namespace'];
    return c;
  })(moteur()), false],
  ['moteur : sabotage du banc joué', ((c) => { c.sabotaged = [banc().sabotaged[0]]; return c; })(moteur()), false],
  ['moteur : moteur non démarré', ((c) => { c.started = false; return c; })(moteur()), false],
  ['muet sain', muet(), true],
  ['muet : rouge hors attente', ((c) => { c.problems.push('[muet:demarrage] démarré'); return c; })(muet()), false],
  ['muet : un rouge attendu manque', ((c) => { c.problems.shift(); return c; })(muet()), false],
];

let failures = 0;
for (const [name, input, valid] of cases) {
  const w = witness(input);
  const good = w.ok === valid;
  if (!good) failures++;
  console.log(`${good ? 'conforme' : 'NON CONFORME'} : ${name} : ${w.ok ? 'VALIDE' : `INVALIDE (${w.reasons.join(' ; ')})`}`);
}
try {
  witness({ mode: 'autre', problems: [], started: true });
  failures++;
  console.log('NON CONFORME : témoin inconnu accepté');
} catch {
  console.log('conforme : témoin inconnu refusé');
}

// Constat du pont muet : sain, puis chaque défaut rougit à son étiquette, et à elle seule.
const healthy = { started: false, last: { loading: 'Loading Lumbra…' }, storage: { keys: [], dbs: [] }, parentMessages: [],
  served: { '/evora-host.json identity': 1 }, warnings: ['moteur : photocraft-web: evora-host.json admits no parent origin: the Evora bridge stays mute'] };
const mute = [
  ['sain', healthy, []],
  ['message au parent', { ...healthy, parentMessages: [{ origin: 'x', data: {} }] }, ['muet:message']],
  ['journal illisible', { ...healthy, parentMessages: null }, ['muet:message']],
  ['démarré', { ...healthy, started: true }, ['muet:demarrage']],
  ['voile retiré', { ...healthy, last: { loading: null } }, ['muet:demarrage']],
  ['stockage écrit', { ...healthy, storage: { keys: ['photocraft.preferences'], dbs: [] } }, ['muet:stockage']],
  ['base ouverte', { ...healthy, storage: { keys: [], dbs: ['photocraft.presets'] } }, ['muet:stockage']],
  ['stockage non relevé', { ...healthy, storage: null }, ['muet:stockage']],
  ['configuration jamais demandée', { ...healthy, served: {} }, ['muet:config']],
  ['autre raison', { ...healthy, warnings: [] }, ['muet:raison']],
];
for (const [name, input, tags] of mute) {
  const got = muteProblems(input).map(tagOf);
  const good = JSON.stringify(got) === JSON.stringify(tags);
  if (!good) failures++;
  console.log(`${good ? 'conforme' : 'NON CONFORME'} : pont muet, ${name} : ${got.join(', ') || 'vert'}`);
}

const total = cases.length + 1 + mute.length;
console.log(`Autotest des témoins : ${total - failures}/${total} cas conformes`);
process.exit(failures ? 1 : 0);
