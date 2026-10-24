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
//   - une modification (Ctrl+I, Image › Réglages › Négatif ; depuis v0.15.1, Sélection › Tout ne
//     modifie plus le document) : evora:dirty true ; evora:request-save : un
//     evora:save conforme (re = la demande, docRef, nom .pcraft, octets non vides d'une archive zip),
//     aucun evora:dirty false tant que la coquille n'a pas répondu (LUM-04, doc.saved-ack), puis
//     evora:dirty false après evora:saved {ok: true} ; une seconde demande : evora:ack {ok: true,
//     code: "clean"} ; Ctrl+S (action de l'utilisateur) : un evora:save sans re ;
//   - accusé d'enregistrement (LUM-04) : après evora:saved {ok: false, error: "conflict"}, le
//     document reste modifié (aucun dirty false, la demande suivante rend un evora:save) ; après un
//     accusé positif d'un enregistrement que le document a dépassé (Ctrl+I après l'émission,
//     révision lue par diag.stats), il reste modifié de même ; la garde beforeunload du moteur se tait
//     sous le pont ; evora:open du même docRef remplace le document modifié (un seul accusé, positif,
//     puis dirty false, un seul document de ce docRef, demande suivante « clean ») ;
//   - evora:theme et evora:lang : accusés positifs, thème et langue enregistrés par le moteur, sous
//     l'espace de noms reçu dans evora:init ;
//   - export.drive (LUM-03) : Export As (file.export.exportAs, Entrée) rend un evora:export PNG
//     conforme (docRef, nom, MIME, octets PNG), accusé par la coquille ; un second, refusé (quota),
//     n'est ni renvoyé ni remplacé par un téléchargement ; Enregistrer sous (Ctrl+Maj+S) rend
//     evora:save {saveAs: true} en .pcraft, et le moteur adopte le docRef de evora:saved (dirty
//     false, demande « clean » pour le nouveau, not_found pour l'ancien, plus aucun message pour
//     l'ancien) ; un document du moteur (file.new, Entrée) crée au premier Ctrl+S un document de la
//     coquille ; aucun téléchargement (compté par smoke.mjs, Browser.downloadWillBegin) ;
//   - evora:open {new: true} (LUM-12) : le dialogue Nouveau document du moteur s'ouvre pour le
//     docRef (compté par diag.stats) ; Entrée : un seul accusé positif, dirty false, document de
//     la coquille, demande « clean » ; Échap sur un second : un seul accusé {ok: false, code:
//     "cancelled"}, aucun document ;
//   - evora:command (LUM-10) : diag.stats et diag.menus accusés avec leur résultat ; file.new,
//     file.export.exportAs et edit.keyboardShortcuts accusés positivement, chacun ouvrant un
//     dialogue du moteur (compté par diag.stats) que la touche Échap referme ; view.zoomIn et
//     file.exit, hors liste, refusés (code capability) sans effet ; un seul accusé par commande ;
//   - stockage (E5 corrigé) : restes posés avant le démarrage (autre compte, noms nus de l'amont)
//     purgés à l'init, noms d'un autre moteur de la même origine conservés ; tout le stockage du
//     moteur sous <storageNs>: (marqueur de base <storageNs>:photocraft.presets compris, aucun
//     marqueur d'un autre espace) ; aucune purge par un message de version 2, à champ en trop ou venu
//     des témoins ; la vraie purge efface d'abord les clés, n'est PAS accusée tant qu'une autre
//     connexion bloque la suppression de la base, puis l'est une fois la base supprimée, avec un
//     stockage vide juste après l'accusé (restes hors espace de noms compris) ; plus aucune
//     écriture ensuite ; OPFS et Cache Storage restent vides ;
//   - menus masqués (LUM-09) : diag.menus liste les articles masqués (masked), dont un
//     échantillon attendu, aucun parmi les articles vivants ; Ctrl+P (Fichier › Imprimer, masqué)
//     n'ouvre aucun dialogue ;
//   - polices (LUM-11) : evora:font d'une police servie (TrueType ou OpenType du dossier des polices
//     de la CI) accusé positivement, ses familles listées par diag.stats (fonts) ; des octets qui ne
//     sont pas une police refusés (invalid_content) ;
//   - sortie clavier (LUM-13) : F6 retenue par le moteur (ni le canevas ni le navigateur ne la
//     voient) et un seul evora:focus-out {} ; Ctrl+F6 laissée passer, sans message (Maj+F6 est le
//     raccourci Contour progressif du moteur) ;
//   - localisation (LUM-14) : une photo géolocalisée (EXIF avec répertoire GPS, XMP avec propriétés
//     GPS et ville) exportée avec Métadonnées « toutes » (choix mémorisé posé avant le démarrage)
//     sort sans répertoire GPS, sans les valeurs de latitude, sans propriété GPS ni ville dans son
//     XMP, mais avec la marque de l'appareil (EXIF) et les droits (XMP) : ce que le témoin intact
//     prouve, c'est que les métadonnées sont lues et réécrites ; de même pour les trois photos du
//     constat de la revue du 2026-10-23 (profil brut EXIF en texte, forme ImageMagick, nettoyé et
//     gardé avec la marque ; EXIF à deux pointeurs GPS, sans GPS en sortie ; noms de lieu XMP sous un
//     autre préfixe, retirés, droits gardés) ; puis pour les trois de la seconde revue (casse : gps,
//     CITY, shotLocation ; pointeur GPS dans le sous-répertoire Exif ; XMP en texte sous d'autres
//     préfixes et clé « Place », droits gardés en texte) ; l'instrument est étalonné sur chaque photo
//     avant export (elle doit se lire localisée) ;
//   - sélecteur de la coquille (LUM-02) : Fichier › Placer (evora:command file.placeEmbedded)
//     demande evora:pick {purpose: "import"}, le fichier remis est placé (evora:dirty true) ;
//     Ctrl+O (Fichier › Ouvrir) demande evora:pick {purpose: "open", multiple}, le fichier remis
//     s'ouvre (un document de plus) ; annulé, rien ne s'ouvre ; un nouveau Ctrl+O redemande
//     (aucun sélecteur resté ouvert) ;
//   - thème Evora révisé (LUM-06 r2) : diag.stats rend les jetons du thème et les traits d'egui ;
//     à l'init (clair, accent de la coquille autre que l'indigo), puis après chaque evora:theme
//     (sombre en indigo, clair et sombre en d'autres accents), bords de champ #71717A à 3:1 au
//     moins sur les champs et les panneaux, entourage du canevas neutre (R = G = B), accent et
//     anneau de focus à 3:1 au moins sur les surfaces, bord d'accent opaque, texte d'accent à 4,5:1
//     au moins, bouton principal lisible ; le bouton principal prend l'accent reçu, le thème le
//     clair ou le sombre demandé ;
//   - préférences de evora:init (LUM-14 r2 et LUM-15, première moitié) : la politique de
//     localisation reçue, l'entourage du canevas réglé par la coquille (gris moyen, dessiné
//     #535353), une préférence du moteur hors liste et une clé inconnue refusées ;
//   - aucune enveloppe hors norme (champ inconnu, version, type, ids, charge utile) et aucun
//     evora:failed ; chaque capacité annoncée est exercée par ce scénario.
// Chaque contrôle porte une étiquette [ehp:…] dans la liste des problèmes du rapport.
//
// SMOKE_SCENARIO=pont-politique (LUM-14 r2), sur le build de fumée (feature evora-smoke) seulement,
// avec SMOKE_POLICY=none|rights|all : la coquille impose la politique de localisation dans
// evora:init, ouvre la photo géolocalisée, lui ajoute deux calques à XMP propre (l'un dans
// Layer::xmp, l'autre dans un bloc shmd brut, comme un document d'une version antérieure ; message
// evora:smoke-layer-xmp, absent de l'artefact servi), puis l'exporte en PNG, JPEG, TIFF, WebP et PSD
// par le service d'export du moteur (evora:smoke-export, absent de l'artefact servi). Jugé sur les
// octets reçus : « none » et « rights » ne laissent sortir aucune localisation (latitude GPS, nom de
// propriété GPS, lieu), ni dans un XMP de calque (UTF-16 du PSD) ; « rights » ne laisse sortir ni
// la marque de l'appareil ni un XMP de calque ; « all » garde la localisation, la marque et les
// droits, dans les cinq formats, et l'XMP de calque avec sa localisation (TIFF à calques et PSD).
// Les témoins intacts (marque et droits en « none », droits en « rights », XMP de calque nettoyé en
// « none ») prouvent que les métadonnées sont lues et réécrites.
//
// Témoins sabordés (SMOKE_SABOTAGE) : le même banc, sur un défaut fabriqué ; un témoin est VALIDE
// (code 0) seulement si chaque défaut rougit à SON détecteur, à SA place, et si tout le reste du banc
// reste vert (verdict exact, function witness) :
//   - « banc » : le banc fait parler le document moteur à tort (messages postés au parent depuis la
//     fenêtre du moteur, par le protocole DevTools : la page parente ne peut pas les distinguer du
//     moteur) et touche son stockage juste avant un contrôle, puis le remet en état. Inventaire
//     constant (BANC_SABOTAGES) : un sabotage non joué, ou crédité par le rouge d'un autre, invalide
//     le témoin ;
//   - « moteur » : le document moteur reçoit, avant son démarrage, un removeItem, un clear et un
//     deleteDatabase qui réussissent sans rien effacer (le moteur croit purger) : seuls les
//     détecteurs de MOTEUR_REDS doivent rougir, chacun à son contrôle ;
//   - « muet » (scénario pont-muet de smoke.mjs) : le banc admet à tort la page parente dans
//     evora-host.json : seuls MUET_REDS doivent rougir ;
//   - « ecoutes » (scénario pont-documents, smoke_docs.mjs) : les écoutes versionchange et storage
//     des documents moteur sont neutralisées : seuls DOCS_REDS doivent rougir, chacun à son contrôle.
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';

export const SCENARIO = 'pont-ehp';
// LUM-04 (e) : scénario pont-panique, joué sur le build de fumée (feature evora-smoke) et sur lui
// seul. Le message qui fait paniquer ce build n'existe pas dans l'artefact servi.
export const PANIC_SCENARIO = 'pont-panique';
const PANIC_TRIGGER = 'evora:smoke-panic';
// Référence du document ouvert par la coquille et espace de noms du compte (valeurs de banc).
const DOC_REF = '01JZZSMKEDCQMENTREF0000001';
// LUM-03 : docRef des nouveaux documents que la coquille crée (Enregistrer sous, premier
// enregistrement d'un document du moteur).
const ADOPT_REF = '01JZZSMKEDCQMENTREF0000002';
const LOCAL_REF = '01JZZSMKEDCQMENTREF0000003';
// LUM-12 : docRef des documents neufs demandés par la coquille (créé, puis annulé).
const NEW_REF = '01JZZSMKEDCQMENTREF0000004';
const CANCEL_REF = '01JZZSMKEDCQMENTREF0000005';
// LUM-14 : docRef de la photo géolocalisée ; LUM-02 : uid Drive (fictif) des fichiers remis par le
// sélecteur de la coquille.
const GPS_REF = '01JZZSMKEDCQMENTREF0000006';
const PICK_UID = '01JZZSMKEDCQMENTREF0000007';
// LUM-14, constat de la revue du 2026-10-23 : un profil brut EXIF en texte (forme ImageMagick), un
// EXIF à deux pointeurs GPS, des noms de lieu XMP sous un autre préfixe.
const RAW_REF = '01JZZSMKEDCQMENTREF0000008';
const DUAL_REF = '01JZZSMKEDCQMENTREF0000009';
const PLACE_REF = '01JZZSMKEDCQMENTREF000000A';
// LUM-14, seconde revue (2026-10-23) : la casse (gps, CITY, shotLocation), un pointeur GPS dans le
// sous-répertoire Exif, un XMP en texte sous d'autres préfixes et une clé « Place ».
const CASE_REF = '01JZZSMKEDCQMENTREF000000B';
const SUB_REF = '01JZZSMKEDCQMENTREF000000C';
const TEXT_REF = '01JZZSMKEDCQMENTREF000000D';
const MIME_RE = /^[a-z0-9][a-z0-9!#$&^_.+-]*\/[a-z0-9][a-z0-9!#$&^_.+-]*$/;
const STORAGE_NS = 'ab'.repeat(32);
// E5 : un autre compte du même navigateur, et un autre moteur servi par la même origine.
const OTHER_NS = 'cd'.repeat(32);
const FOREIGN_KEY = 'autre-moteur:reglages';
const FOREIGN_DB = 'autre-moteur.cache';
const PREFS = 'photocraft.preferences';
const PRESETS = 'photocraft.presets';
// LUM-06 r2 : l'accent de la coquille dans evora:init (autre que l'indigo d'Evora, pour que son
// application se voie), les evora:theme suivants, et les valeurs que le thème doit tenir.
const INIT_ACCENT = '#0E7490';
const EVORA_ACCENT = '#4F46E5';
const THEME_STEPS = [{ theme: 'light', accent: '#BE185D', kind: 'evoraLight' }, { theme: 'dark', accent: '#B45309', kind: 'evora' }];
const FIELD_BORDER = '#71717A';
// Écart admis par canal entre le fond d'accent rapporté et l'accent reçu (arrondi de la
// prémultiplication d'egui, alpha 40 ou 64).
const SOFT_TOLERANCE = 4;
const THEME_KEYS = ['accent', 'accentText', 'accentBorder', 'accentSoft', 'primaryBg', 'primaryText', 'fieldBorder', 'field', 'card', 'chrome',
  'dock', 'hover', 'pressed', 'canvas', 'canvasDot', 'inactiveBorder', 'activeBorder', 'selectionStroke'];
// LUM-14 r2 et LUM-15 : préférences imposées par la coquille dans evora:init (engines.json,
// prefs.imposed) ; le moteur doit prendre les deux premières et refuser les deux autres (une
// préférence du moteur hors de sa liste, une clé inconnue).
const BENCH_PREFS = { 'evora.locationPolicy': 'none', 'interface.canvasColor': 'mediumGray', 'interface.canvasBorder': 'line', 'evora.inconnue': 'x' };
const BENCH_REFUSED = ['evora.inconnue', 'interface.canvasBorder'];
// Le gris moyen de l'entourage du canevas (Préférences › Interface du moteur).
const MEDIUM_GRAY = '#535353';
// LUM-14 r2 : scénario pont-politique (build de fumée), la politique imposée, le document et les
// formats exportés.
export const POLICY_SCENARIO = 'pont-politique';
export const POLICY = process.env.SMOKE_POLICY || '';
if (!['', 'none', 'rights', 'all'].includes(POLICY)) throw new Error(`SMOKE_POLICY inconnue : ${POLICY}`);
const POLICY_REF = '01JZZSMKEDCQMENTREF000000E';
const POLICY_FORMATS = ['png', 'jpg', 'tif', 'webp', 'psd'];
// Formats dont les témoins intacts sont exigés : tous. PNG et PSD écrivent EXIF et XMP à coup sûr ;
// JPEG, TIFF et WebP les écrivent aussi, mesuré par le premier run de ce banc (40077170134 : sous
// « all », latitude, lieu, marque et droits présents dans les cinq formats ; sous « none », marque et
// droits présents, aucune localisation). Le TIFF à calques et le PSD portent en plus l'XMP propre des
// calques (UTF-16), mesuré de même.
const POLICY_STRICT = ['png', 'jpg', 'tif', 'webp', 'psd'];
const POLICY_LAYERED = ['tif', 'psd'];
const SMOKE_EXPORT = 'evora:smoke-export';
const SMOKE_LAYER_XMP = 'evora:smoke-layer-xmp';
// Plafond transmis dans evora:init : valeur de banc (pas un plafond du produit, D-15), assez large
// pour la petite image et son document de travail.
const BENCH_MAX_BYTES = 8 * 1024 * 1024;
const ULID_RE = /^[0-9A-HJKMNP-TV-Z]{26}$/;
// Vocabulaire du §4.2 et ui.focus-out de l'amendement A-19 (1.1 en projet, §7.9).
const CAPABILITY_NAMES = ['doc.open', 'doc.save', 'doc.saved-ack', 'doc.dirty', 'doc.multi', 'doc.request-save',
  'export.drive', 'media.need', 'media.ranges', 'pick.drive', 'command', 'storage.ns', 'storage.memory',
  'storage.purge', 'live.lang', 'live.theme', 'font.inject', 'author', 'ui.focus-out'];
// Capacités que ce scénario exerce : le moteur ne doit en annoncer aucune autre.
const EXERCISED = ['doc.open', 'doc.save', 'doc.saved-ack', 'doc.dirty', 'doc.request-save', 'export.drive', 'pick.drive', 'storage.ns',
  'storage.purge', 'live.lang', 'live.theme', 'command', 'font.inject', 'ui.focus-out'];
// LUM-09 : articles que le build hôte doit masquer (échantillon : impression, lots, script qui lit
// le disque, montage vidéo, Quitter), tous présents dans l'arbre complet des menus.
const MASKED_SAMPLE = ['file.print', 'file.automate.batch', 'file.scripts.loadFilesIntoStack', 'window.panel.timeline', 'file.exit'];
// LUM-02 : motif d'un élément de accept (§4.4).
const ACCEPT_RE = /^(\.[a-z0-9]+|[a-z0-9.+-]+\/[a-z0-9.+*-]+)$/;
// LUM-10 : commandes de la liste blanche du moteur qui ouvrent un dialogue, et commandes réelles du
// moteur hors liste (sans dialogue ni effet sur le document).
const DIALOG_COMMANDS = ['file.new', 'file.export.exportAs', 'edit.keyboardShortcuts'];
const REFUSED_COMMANDS = ['view.zoomIn', 'file.exit'];
const ACK_CODES = ['clean', 'cancelled', 'conflict', 'too_large', 'quota', 'forbidden', 'not_found', 'invalid_content',
  'unsupported', 'rate_limited', 'read_only', 'busy', 'capability', 'network', 'server', 'timeout'];
const FAIL_CODES = ['no_webgl2', 'wasm', 'memory', 'panic', 'unsupported_browser', 'protocol', 'init'];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// Témoin sabordé joué par ce passage ('' : aucun, le banc normal).
export const SABOTAGE = process.env.SMOKE_SABOTAGE || '';
if (!['', 'banc', 'moteur', 'muet', 'ecoutes'].includes(SABOTAGE)) throw new Error(`SMOKE_SABOTAGE inconnu : ${SABOTAGE}`);
// Témoin « banc » : chaque sabotage, le seul détecteur qui doit rougir et, quand ce détecteur est un
// contrôle du stockage, le contrôle (checkpoint) où il doit rougir. Un sabotage « message » est
// crédité seulement par un rouge qui cite l'id du message fabriqué.
export const BANC_SABOTAGES = [
  { tag: 'ehp:ready-count', kind: 'message' },
  { tag: 'ehp:answer-v2', kind: 'message' },
  { tag: 'ehp:answer-envelope', kind: 'message' },
  { tag: 'ehp:answer-foreign', kind: 'message' },
  { tag: 'ehp:answer-source', kind: 'message' },
  { tag: 'ehp:envelope', kind: 'message' },
  { tag: 'ehp:save-shape', kind: 'message' },
  { tag: 'ehp:purge-early-ack', kind: 'message', at: 'purge-bloquée:accusé' },
  { tag: 'ehp:namespace', kind: 'stockage', at: 'init:espace' },
  { tag: 'ehp:foreign-names', kind: 'stockage', at: 'init:autre-moteur' },
  { tag: 'ehp:purge-witness', kind: 'stockage', at: 'témoins:effacement' },
  { tag: 'ehp:purge-order', kind: 'stockage', at: 'purge-bloquée:clés' },
  { tag: 'ehp:purge-left', kind: 'stockage', at: 'après-accusé:restes' },
  { tag: 'ehp:purge-rewrite', kind: 'stockage', at: 'après-écriture:réécriture' },
  { tag: 'ehp:session-storage', kind: 'stockage', at: 'après-écriture:session' },
  { tag: 'ehp:caches', kind: 'stockage', at: 'après-écriture:caches' },
  { tag: 'ehp:opfs', kind: 'stockage', at: 'après-écriture:opfs' },
  { tag: 'ehp:command-allowed', kind: 'message' },
  { tag: 'ehp:command-refused', kind: 'message' },
  { tag: 'ehp:saved-ack-ok', kind: 'message' },
  { tag: 'ehp:saved-ack-conflict', kind: 'message' },
  { tag: 'ehp:saved-ack-late-edit', kind: 'message' },
  { tag: 'ehp:reload-same-docref', kind: 'message' },
  { tag: 'ehp:beforeunload', kind: 'document', at: 'départ:garde' },
  { tag: 'ehp:export-png', kind: 'message' },
  { tag: 'ehp:export-refused', kind: 'message' },
  { tag: 'ehp:save-as-adopt', kind: 'message' },
  { tag: 'ehp:local-doc-first-save', kind: 'message' },
  { tag: 'ehp:open-new', kind: 'message' },
  { tag: 'ehp:open-new-cancel', kind: 'message' },
  { tag: 'ehp:menus-masked', kind: 'message' },
  { tag: 'ehp:font-inject', kind: 'message' },
  { tag: 'ehp:focus-out', kind: 'message' },
  { tag: 'ehp:export-no-gps', kind: 'message' },
  { tag: 'ehp:pick', kind: 'message' },
  { tag: 'ehp:theme-r2', kind: 'message' },
  { tag: 'ehp:theme-accent', kind: 'message' },
  { tag: 'ehp:prefs-init', kind: 'message' },
];
// Témoin « moteur » (removeItem, clear et deleteDatabase sans effet) : les rouges attendus, chacun à
// son contrôle. Les restes non effacés font aussi rougir les témoins de purge et la réécriture : ce
// débordement est déterministe et fait partie de l'attendu.
export const MOTEUR_REDS = [
  { tag: 'ehp:namespace', at: 'init:espace' },
  { tag: 'ehp:purge-witness', at: 'témoins:effacement' },
  { tag: 'ehp:purge-early-ack', at: 'purge-bloquée:accusé' },
  { tag: 'ehp:purge-order', at: 'purge-bloquée:clés' },
  { tag: 'ehp:purge-left', at: 'après-accusé:restes' },
  { tag: 'ehp:purge-rewrite', at: 'après-écriture:réécriture' },
];
// Témoin « muet » (scénario pont-muet, parent admis à tort) : le moteur s'annonce et ne se tait plus
// pour la raison attendue ; il ne démarre toujours pas (il attend evora:init) et n'écrit rien.
export const MUET_REDS = [{ tag: 'muet:message' }, { tag: 'muet:raison' }];
// Témoin « ecoutes » (scénario pont-documents, écoutes versionchange et storage neutralisées dans
// les documents moteur) : chaque document vivant garde sa base ouverte et réécrit ses clés. Rouges
// attendus, chacun à son contrôle : la base de l'autre compte survit à l'init du compte suivant,
// l'autre compte réécrit ses préférences, la purge d'un compte ouvert dans deux documents n'est pas
// accusée positivement (busy à l'échéance du moteur), ses restes survivent, et le second document
// réécrit ses préférences.
export const DOCS_REDS = [
  { tag: 'docs:base', at: 'b-init:base' },
  { tag: 'docs:reecriture', at: 'a-ecriture:cle' },
  { tag: 'docs:purge-accuse', at: 'purge-deux:accuse' },
  { tag: 'docs:purge-restes', at: 'purge-deux:restes' },
  { tag: 'docs:reecriture', at: 'deux-ecriture:cle' },
];

// L'étiquette d'un problème du rapport ([ehp:…], [muet:…] ou [docs:…]), ou null.
export function tagOf(problem) {
  const m = /^\[((?:ehp|muet|docs):[^\]]+)\]/.exec(String(problem));
  return m ? m[1] : null;
}

// Verdict exact d'un témoin sabordé : chaque rouge attendu est vu, à sa place et crédité à son
// sabotage ; aucun autre rouge (étiqueté ou non) ; aucun contrôle non visé n'a rougi.
export function witness({ mode, problems, sabotaged = [], checks = [], started }) {
  const inventory = { banc: BANC_SABOTAGES, moteur: MOTEUR_REDS, muet: MUET_REDS, ecoutes: DOCS_REDS }[mode];
  if (!inventory) throw new Error(`témoin inconnu : ${mode}`);
  const list = problems.map(String);
  const untagged = list.filter((p) => !tagOf(p));
  const seen = [...new Set(list.map(tagOf).filter(Boolean))].sort();
  const expected = [...new Set(inventory.map((s) => s.tag))].sort();
  const missing = expected.filter((t) => !seen.includes(t));
  const unexpected = seen.filter((t) => !expected.includes(t));
  const targeted = new Set(inventory.filter((s) => s.at).map((s) => s.at));
  const uncredited = [];
  for (const s of inventory) {
    const why = [];
    if (s.at) {
      const at = checks.filter((c) => c.label === s.at);
      if (at.length !== 1) why.push(`contrôle ${s.at} exécuté ${at.length} fois`);
      else if (at[0].tags.length !== 1 || at[0].tags[0] !== s.tag) why.push(`contrôle ${s.at} : ${at[0].tags.join(', ') || 'vert'}`);
    }
    if (mode === 'banc') {
      const done = sabotaged.filter((e) => e.tag === s.tag);
      if (done.length !== 1) why.push(`sabotage joué ${done.length} fois`);
      else if (s.kind === 'message' && !list.some((p) => tagOf(p) === s.tag && done[0].id && p.includes(done[0].id))) {
        why.push(`aucun rouge ${s.tag} ne cite le message fabriqué ${done[0].id}`);
      }
    }
    if (why.length) uncredited.push(`${s.tag} (${why.join(' ; ')})`);
  }
  const strays = mode === 'banc' ? sabotaged.filter((e) => !inventory.some((s) => s.tag === e.tag)) : sabotaged;
  if (strays.length) uncredited.push(`sabotages hors inventaire : ${strays.map((e) => e.tag).join(', ')}`);
  const spilled = checks.filter((c) => !targeted.has(c.label) && c.tags.length).map((c) => `${c.label} : ${c.tags.join(', ')}`);
  const reasons = [
    ...(mode !== 'muet' && !started ? ['moteur non démarré'] : []),
    ...(untagged.length ? [`problèmes hors banc : ${untagged.join(' | ').slice(0, 400)}`] : []),
    ...(missing.length ? [`détecteurs restés verts : ${missing.join(', ')}`] : []),
    ...(unexpected.length ? [`détecteurs rougis hors attente : ${unexpected.join(', ')}`] : []),
    ...(uncredited.length ? [`rouges non crédités : ${uncredited.join(', ')}`] : []),
    ...(spilled.length ? [`contrôles non visés rougis : ${spilled.join(', ')}`] : []),
  ];
  return { mode, ok: reasons.length === 0, expected, seen, missing, unexpected, uncredited, spilled, untagged, reasons,
    sabotaged: sabotaged.map((e) => `${e.tag} : ${e.what}`) };
}

// Témoin « moteur », posé dans le document moteur avant son démarrage : le moteur croit effacer.
// removeItem et clear ne font rien ; deleteDatabase rend une demande qui réussit au tour suivant sans
// rien supprimer (le banc lui-même n'efface jamais rien : il ne lit, n'écrit et n'ouvre).
export const ENGINE_SABOTAGE_JS = `(() => {
  if (window.__evoraEngineSabotaged) return;
  window.__evoraEngineSabotaged = true;
  Storage.prototype.removeItem = function () {};
  Storage.prototype.clear = function () {};
  IDBFactory.prototype.deleteDatabase = function () {
    const req = { result: undefined, error: null, readyState: 'pending', onsuccess: null, onerror: null, onblocked: null, onupgradeneeded: null };
    setTimeout(() => {
      req.readyState = 'done';
      if (typeof req.onsuccess === 'function') req.onsuccess(new Event('success'));
    }, 0);
    return req;
  };
})();`;

// Scénario pont-muet (smoke.mjs) : ?host=evora avec l'evora-host.json de l'artefact, livré vide.
// Le pont doit rester muet, pour cette raison, sans démarrer ni écrire.
export function muteProblems({ started, last, storage, parentMessages, served, warnings }) {
  const out = [];
  if (!Object.keys(served || {}).some((k) => k.startsWith('/evora-host.json '))) {
    out.push("[muet:config] evora-host.json jamais demandé par le moteur : le pont n'a pas été mis à l'épreuve");
  }
  if (!Array.isArray(parentMessages)) out.push('[muet:message] journal des messages de la page parente illisible');
  else if (parentMessages.length) out.push(`[muet:message] le moteur a écrit à la page parente sans parent admis : ${JSON.stringify(parentMessages).slice(0, 300)}`);
  if (started || !last || last.loading === null) out.push(`[muet:demarrage] le moteur a démarré sans parent admis : ${JSON.stringify(last)}`);
  if (!storage) out.push('[muet:stockage] stockage du document moteur non relevé');
  else if (storage.keys.length || (storage.dbs || []).length) out.push(`[muet:stockage] stockage écrit sans parent admis : ${JSON.stringify(storage)}`);
  if (!(warnings || []).some((w) => w.includes('admits no parent origin'))) {
    out.push("[muet:raison] avertissement « evora-host.json admits no parent origin » absent : le moteur ne s'est pas tu pour la raison attendue");
  }
  return out;
}

// Une image PNG RGBA de 32 × 32 (indigo), écrite ici : aucun fichier binaire dans le dépôt. `extra` :
// blocs ([type, données]) placés avant les pixels (eXIf, iTXt).
function pngFile(extra = []) {
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
    ...extra.map(([type, data]) => chunk(type, data)),
    chunk('IDAT', zlib.deflateSync(Buffer.concat(rows))), chunk('IEND', Buffer.alloc(0))]);
}

function png() {
  return pngFile();
}

// LUM-14 : l'EXIF d'une photo géolocalisée (TIFF petit-boutiste) : IFD0 avec la marque de
// l'appareil (Make, témoin intact) et le pointeur GPS ; répertoire GPS avec la version, la
// référence N et la latitude 12/1, 34/1, 56/1 (24 octets hors des entrées : le banc cherche ce
// reste après l'export).
export const GPS_LATITUDE = [12, 1, 34, 1, 56, 1];
export const EXIF_MAKE = 'EvoraSmoke';
function exifFixture() {
  const make = Buffer.from(`${EXIF_MAKE}\0`, 'latin1');
  const out = Buffer.alloc(116);
  out.write('II', 0, 'latin1');
  out.writeUInt16LE(42, 2);
  out.writeUInt32LE(8, 4);
  // IFD0 à 8 : deux entrées, puis le prochain répertoire (aucun) ; fin à 38.
  out.writeUInt16LE(2, 8);
  out.writeUInt16LE(0x010f, 10); out.writeUInt16LE(2, 12); out.writeUInt32LE(make.length, 14); out.writeUInt32LE(38, 18);
  out.writeUInt16LE(0x8825, 22); out.writeUInt16LE(4, 24); out.writeUInt32LE(1, 26); out.writeUInt32LE(50, 30);
  out.writeUInt32LE(0, 34);
  make.copy(out, 38);
  // Répertoire GPS à 50 : trois entrées, puis le prochain (aucun) ; fin à 92 ; latitude à 92.
  out.writeUInt16LE(3, 50);
  out.writeUInt16LE(0, 52); out.writeUInt16LE(1, 54); out.writeUInt32LE(4, 56); out.set([2, 3, 0, 0], 60);
  out.writeUInt16LE(1, 64); out.writeUInt16LE(2, 66); out.writeUInt32LE(2, 68); out.write('N', 72, 'latin1');
  out.writeUInt16LE(2, 76); out.writeUInt16LE(5, 78); out.writeUInt32LE(3, 80); out.writeUInt32LE(92, 84);
  out.writeUInt32LE(0, 88);
  GPS_LATITUDE.forEach((w, k) => out.writeUInt32LE(w, 92 + 4 * k));
  return out;
}

// LUM-14 (revue) : un EXIF dont IFD0 a deux pointeurs GPS, chacun vers un répertoire qui porte la
// latitude : retirer le premier seul laisse le second.
function dualExifFixture() {
  const make = Buffer.from(`${EXIF_MAKE}\0`, 'latin1');
  const out = Buffer.alloc(62 + 2 * 42);
  out.write('II', 0, 'latin1');
  out.writeUInt16LE(42, 2);
  out.writeUInt32LE(8, 4);
  // IFD0 à 8 : trois entrées, puis le prochain répertoire (aucun) ; fin à 50 ; marque à 50.
  out.writeUInt16LE(3, 8);
  out.writeUInt16LE(0x010f, 10); out.writeUInt16LE(2, 12); out.writeUInt32LE(make.length, 14); out.writeUInt32LE(50, 18);
  for (const [k, at] of [[0, 62], [1, 104]]) {
    const e = 22 + 12 * k;
    out.writeUInt16LE(0x8825, e); out.writeUInt16LE(4, e + 2); out.writeUInt32LE(1, e + 4); out.writeUInt32LE(at, e + 8);
  }
  out.writeUInt32LE(0, 46);
  make.copy(out, 50);
  // Deux répertoires GPS (62 et 104) : une entrée (latitude, hors entrée), puis le prochain (aucun).
  for (const at of [62, 104]) {
    out.writeUInt16LE(1, at);
    out.writeUInt16LE(2, at + 2); out.writeUInt16LE(5, at + 4); out.writeUInt32LE(3, at + 6); out.writeUInt32LE(at + 18, at + 10);
    out.writeUInt32LE(0, at + 14);
    GPS_LATITUDE.forEach((w, k) => out.writeUInt32LE(w, at + 18 + 4 * k));
  }
  return out;
}

// LUM-14 (seconde revue) : un EXIF dont IFD0 n'a pas de pointeur GPS, mais un pointeur Exif (0x8769)
// vers un sous-répertoire qui, lui, pointe vers un répertoire GPS à latitude.
function subExifFixture() {
  const make = Buffer.from(`${EXIF_MAKE}\0`, 'latin1');
  const out = Buffer.alloc(110);
  out.write('II', 0, 'latin1');
  out.writeUInt16LE(42, 2);
  out.writeUInt32LE(8, 4);
  // IFD0 à 8 : marque et pointeur Exif, puis le prochain répertoire (aucun) ; fin à 38 ; marque à 38.
  out.writeUInt16LE(2, 8);
  out.writeUInt16LE(0x010f, 10); out.writeUInt16LE(2, 12); out.writeUInt32LE(make.length, 14); out.writeUInt32LE(38, 18);
  out.writeUInt16LE(0x8769, 22); out.writeUInt16LE(4, 24); out.writeUInt32LE(1, 26); out.writeUInt32LE(50, 30);
  out.writeUInt32LE(0, 34);
  make.copy(out, 38);
  // Sous-répertoire Exif à 50 : un pointeur GPS vers 68 ; fin à 68.
  out.writeUInt16LE(1, 50);
  out.writeUInt16LE(0x8825, 52); out.writeUInt16LE(4, 54); out.writeUInt32LE(1, 56); out.writeUInt32LE(68, 60);
  out.writeUInt32LE(0, 64);
  // Répertoire GPS à 68 : la latitude à 86 ; fin à 110.
  out.writeUInt16LE(1, 68);
  out.writeUInt16LE(2, 70); out.writeUInt16LE(5, 72); out.writeUInt32LE(3, 74); out.writeUInt32LE(86, 78);
  out.writeUInt32LE(0, 82);
  GPS_LATITUDE.forEach((w, k) => out.writeUInt32LE(w, 86 + 4 * k));
  return out;
}

// LUM-14 (revue) : un profil brut en texte, tel qu'ImageMagick l'écrit : « \n<nom>\n<longueur sur 8>\n »,
// puis l'hexadécimal par lignes de 72 chiffres.
function rawProfile(name, bytes) {
  let out = `\n${name}\n${String(bytes.length).padStart(8)}\n`;
  for (let i = 0; i < bytes.length; i += 36) out += `${bytes.subarray(i, i + 36).toString('hex')}\n`;
  return out;
}

// LUM-14 : le XMP de la même photo : latitude (attribut), ville (attribut), longitude et version
// GPS (éléments, l'une avec un contenu imbriqué), altitude (élément vide), puis les droits (témoin
// intact).
export const XMP_RIGHTS = 'EvoraSmokeRights';
const XMP_FIXTURE = '<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/">'
  + '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about=""'
  + ' xmlns:exif="http://ns.adobe.com/exif/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/"'
  + ' xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/" exif:GPSLatitude="12,34.56N" photoshop:City="Papeete">'
  + '<exif:GPSLongitude>56,7.89E</exif:GPSLongitude><exif:GPSVersionID><rdf:Seq><rdf:li>2</rdf:li></rdf:Seq></exif:GPSVersionID>'
  + `<exif:GPSAltitude/><dc:rights><rdf:Alt><rdf:li xml:lang="x-default">${XMP_RIGHTS}</rdf:li></rdf:Alt></dc:rights>`
  + '</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end="w"?>';
// LUM-14 (revue) : des noms de lieu sous un préfixe qui n'est pas celui du schéma (ps2:City), et
// Sublocation de l'IPTC, avec les droits (témoin intact).
const XMP_PLACES_FIXTURE = '<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/">'
  + '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about=""'
  + ' xmlns:ps2="http://ns.adobe.com/photoshop/1.0/" xmlns:Iptc4xmpCore="http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/"'
  + ' xmlns:dc="http://purl.org/dc/elements/1.1/" ps2:City="Papeete">'
  + '<Iptc4xmpCore:Sublocation>Papeete</Iptc4xmpCore:Sublocation>'
  + `<dc:rights><rdf:Alt><rdf:li xml:lang="x-default">${XMP_RIGHTS}</rdf:li></rdf:Alt></dc:rights>`
  + '</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end="w"?>';

// LUM-14 (seconde revue) : la casse ne doit rien sauver (exif:gpsLatitude, ps2:CITY), ni un nom local
// qui contient « location » (xmpDM:shotLocation) ; les droits restent (témoin intact).
const XMP_CASE_FIXTURE = '<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/">'
  + '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about=""'
  + ' xmlns:exif="http://ns.adobe.com/exif/1.0/" xmlns:ps2="http://ns.adobe.com/photoshop/1.0/"'
  + ' xmlns:xmpDM="http://ns.adobe.com/xmp/1.0/DynamicMedia/" xmlns:dc="http://purl.org/dc/elements/1.1/"'
  + ' exif:gpsLatitude="12,34.56N" ps2:CITY="Papeete" xmpDM:shotLocation="Papeete">'
  + `<dc:rights><rdf:Alt><rdf:li xml:lang="x-default">${XMP_RIGHTS}</rdf:li></rdf:Alt></dc:rights>`
  + '</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end="w"?>';
// LUM-14 (seconde revue) : un XMP rangé en texte libre, d'autres préfixes et d'autre casse (seule
// l'URI le dit XMP), puis une clé « Place », puis les droits en texte (témoin intact).
const XMP_TEXT_FIXTURE = '<X:XmpMeta xmlns:X="adobe:ns:meta/"><Rdf:RDF xmlns:Rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">'
  + '<Rdf:Description xmlns:ps2="http://ns.adobe.com/photoshop/1.0/" ps2:City="Papeete"/></Rdf:RDF></X:XmpMeta>';
// LUM-14 r2 (scénario pont-politique) : l'XMP propre d'un calque : longitude GPS et ville, puis les
// droits (témoin intact).
const LAYER_XMP_FIXTURE = '<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">'
  + '<rdf:Description rdf:about="" xmlns:exif="http://ns.adobe.com/exif/1.0/" xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/"'
  + ' xmlns:dc="http://purl.org/dc/elements/1.1/" exif:GPSLongitude="56,7.89E" photoshop:City="Papeete">'
  + `<dc:rights><rdf:Alt><rdf:li xml:lang="x-default">${XMP_RIGHTS}</rdf:li></rdf:Alt></dc:rights>`
  + '</rdf:Description></rdf:RDF></x:xmpmeta>';
function textChunk(keyword, value) {
  return ['tEXt', Buffer.concat([Buffer.from(keyword, 'latin1'), Buffer.from([0]), Buffer.from(value, 'latin1')])];
}

function itxt(keyword, text) {
  return Buffer.concat([Buffer.from(keyword, 'latin1'), Buffer.from([0, 0, 0, 0, 0]), Buffer.from(text, 'utf8')]);
}

// La photo géolocalisée (LUM-14).
function geoPng() {
  return pngFile([['eXIf', exifFixture()], ['iTXt', itxt('XML:com.adobe.xmp', XMP_FIXTURE)]]);
}

// LUM-14 (revue) : les trois photos du constat.
function rawPng() {
  const value = rawProfile('exif', Buffer.concat([Buffer.from('Exif\0\0', 'latin1'), exifFixture()]));
  return pngFile([['tEXt', Buffer.concat([Buffer.from('Raw profile type exif', 'latin1'), Buffer.from([0]), Buffer.from(value, 'latin1')])]]);
}
function dualPng() {
  return pngFile([['eXIf', dualExifFixture()]]);
}
function placePng() {
  return pngFile([['iTXt', itxt('XML:com.adobe.xmp', XMP_PLACES_FIXTURE)]]);
}
// LUM-14 : les trois photos de la seconde revue.
function casePng() {
  return pngFile([['iTXt', itxt('XML:com.adobe.xmp', XMP_CASE_FIXTURE)]]);
}
function subPng() {
  return pngFile([['eXIf', subExifFixture()]]);
}
function textPng() {
  return pngFile([textChunk('Comment', XMP_TEXT_FIXTURE), textChunk('Place', 'Papeete'), textChunk('Copyright', XMP_RIGHTS)]);
}

// Les photos géolocalisées du banc, par docRef (LUM-14).
const GEO_FIXTURES = [
  { ref: GPS_REF, name: 'evora-geo.png', make: geoPng },
  { ref: RAW_REF, name: 'evora-brut.png', make: rawPng },
  { ref: DUAL_REF, name: 'evora-double.png', make: dualPng },
  { ref: PLACE_REF, name: 'evora-lieux.png', make: placePng },
  { ref: CASE_REF, name: 'evora-casse.png', make: casePng },
  { ref: SUB_REF, name: 'evora-sous-ifd.png', make: subPng },
  { ref: TEXT_REF, name: 'evora-texte.png', make: textPng },
];

// LUM-11 : la plus petite police TrueType ou OpenType du dossier des polices servies (quatrième
// argument de smoke.mjs, fonts/manifest.txt de craft-fonts), ou null.
function benchFont() {
  const dir = process.argv[5];
  if (!dir) return null;
  try {
    const manifest = fs.readFileSync(path.join(dir, 'fonts/manifest.txt'), 'utf8');
    const rows = manifest.split('\n').map((l) => l.split(' | ').map((c) => c.trim()))
      .filter((c) => c.length >= 3 && c[0] && !c[0].startsWith('#') && /\.(ttf|otf)$/i.test(c[2]) && c[2].startsWith('fonts/') && !c[2].includes('..'));
    const files = rows.map((c) => ({ family: c[0], file: path.join(dir, c[2]), format: /\.otf$/i.test(c[2]) ? 'otf' : 'ttf' }))
      .filter((f) => fs.existsSync(f.file)).map((f) => ({ ...f, size: fs.statSync(f.file).size })).sort((a, b) => a.size - b.size);
    const font = files[0];
    if (!font || font.size > BENCH_MAX_BYTES) return null;
    return { family: font.family, format: font.format, b64: fs.readFileSync(font.file).toString('base64') };
  } catch {
    return null;
  }
}

// Posé dans le document moteur avant son démarrage (Page.addScriptToEvaluateOnNewDocument).
export const SEED_JS = `(() => {
  if (window.__evoraSeeded) return;
  window.__evoraSeeded = true;
  try {
    localStorage.setItem('${PREFS}', '{}');
    localStorage.setItem('${OTHER_NS}:${PREFS}', '{}');
    localStorage.setItem('${FOREIGN_KEY}', 'garde');
    // LUM-14 : un compte qui a choisi Métadonnées « toutes » dans Exporter sous (choix mémorisé par
    // le moteur, sous son espace de noms) : la localisation doit sortir quand même.
    localStorage.setItem('${STORAGE_NS}:${PREFS}', JSON.stringify({ dialogs: { 'file.export.exportAs': { metadata: 'all' } } }));
  } catch (e) { /* stockage refusé : les contrôles le diront */ }
  for (const name of ['${PRESETS}', '${OTHER_NS}:${PRESETS}', '${FOREIGN_DB}']) {
    const r = indexedDB.open(name, 1);
    r.onupgradeneeded = () => r.result.createObjectStore('files');
    r.onsuccess = () => r.result.close();
  }
})();`;

// Tout le stockage visible du document moteur.
const STORAGE_ALL = `(async () => {
  const keys = Object.keys(localStorage);
  const dbs = indexedDB.databases ? (await indexedDB.databases()).map((d) => d.name) : null;
  let opfs = null;
  try { const root = await navigator.storage.getDirectory(); opfs = []; for await (const [n] of root.entries()) opfs.push(n); } catch (e) { opfs = 'indisponible'; }
  let cacheKeys = null;
  try { cacheKeys = await caches.keys(); } catch (e) { cacheKeys = 'indisponible'; }
  let session = null;
  try { session = Object.keys(sessionStorage); } catch (e) { session = 'indisponible'; }
  let prefs = null;
  try { prefs = JSON.parse(localStorage.getItem('${STORAGE_NS}:${PREFS}') || 'null'); } catch (e) { prefs = 'illisible'; }
  const iface = (prefs && prefs.interface) || {};
  return JSON.stringify({ keys, dbs, opfs, caches: cacheKeys, session, theme: iface.theme ?? null, language: iface.language ?? null });
})()`;

export function hostConfig(parentOrigin) {
  return JSON.stringify({ parentOrigins: [parentOrigin] });
}

export function parentPage(engineSrc, foreignOrigin, parentOrigin) {
  const w = (origin) => `${origin}/witness.html?engine=${encodeURIComponent(new URL(engineSrc).origin)}&parent=${encodeURIComponent(parentOrigin)}`;
  return `<!doctype html><html lang="fr"><head><meta charset="utf-8"><title>Evora : coquille minimale du test de fumée</title>
<link rel="icon" href="data:,"></head><body style="margin:0;background:#fff">
<iframe id="engine" src="${engineSrc}" width="1280" height="800" style="border:0;display:block"
 sandbox="allow-scripts allow-same-origin allow-downloads" allow="fullscreen; clipboard-read; clipboard-write"></iframe>
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
  const geo = Object.fromEntries(GEO_FIXTURES.map((f) => [f.ref, f.make().toString('base64')]));
  const font = benchFont();
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
  // LUM-14 : ce qu'un PNG exporté garde de ses métadonnées (blocs eXIf et iTXt XMP lus ici).
  const latin1 = (a) => Array.from(a, (c) => String.fromCharCode(c)).join('');
  const tiffTags = (d) => {
    if (d.length < 8) return { le: true, tags: null };
    const dv = new DataView(d.buffer, d.byteOffset, d.byteLength);
    const le = d[0] === 0x49;
    const ifd = dv.getUint32(4, le);
    if (ifd + 2 > d.length) return { le, tags: null };
    const n = dv.getUint16(ifd, le);
    const tags = [];
    for (let i = 0; i < n && ifd + 4 + 12 * i <= d.length; i++) tags.push(dv.getUint16(ifd + 2 + 12 * i, le));
    return { le, tags };
  };
  const latitude = (d, le) => {
    const pat = new Uint8Array(24);
    const pv = new DataView(pat.buffer);
    ${JSON.stringify(GPS_LATITUDE)}.forEach((w, k) => pv.setUint32(4 * k, w, le));
    for (let i = 0; i + 24 <= d.length; i++) {
      let same = true;
      for (let j = 0; j < 24 && same; j++) same = d[i + j] === pat[j];
      if (same) return true;
    }
    return false;
  };
  const pngMeta = (buf) => {
    const b = new Uint8Array(buf);
    const sig = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    if (b.length < 8 || sig.some((v, i) => b[i] !== v)) return null;
    const dv = new DataView(buf);
    const out = { exif: false, make: false, gpsTag: false, latitude: false, xmp: false, compressed: false, rights: false, location: false,
      raw: false, rawMake: false, rawGps: false, rawLatitude: false, textLocation: false, textCompressed: false, textRights: false };
    const text = (key, value) => {
      if (/gps|papeete|latitude|longitude/i.test(key) || /gps|papeete/i.test(value)) out.textLocation = true;
      if (value.includes(${JSON.stringify(XMP_RIGHTS)})) out.textRights = true;
      if (/^raw profile type exif$/i.test(key)) {
        const w = value.trim().split(/\\s+/);
        const hex = w.slice(2).join('');
        const bytes = /^[0-9a-f]*$/i.test(hex) && hex.length === 2 * Number(w[1]) ? Uint8Array.from(hex.match(/../g) || [], (h) => parseInt(h, 16)) : null;
        out.raw = true;
        if (!bytes) { out.rawGps = true; return; }
        const body = latin1(bytes.subarray(0, 6)) === 'Exif\\0\\0' ? bytes.subarray(6) : bytes;
        const t = tiffTags(body);
        out.rawMake = !!t.tags && t.tags.includes(0x010f) && latin1(body).includes(${JSON.stringify(EXIF_MAKE)});
        out.rawGps = !t.tags || t.tags.includes(0x8825);
        out.rawLatitude = latitude(body, t.le);
      }
    };
    let at = 8;
    while (at + 12 <= b.length) {
      const len = dv.getUint32(at);
      const type = latin1(b.subarray(at + 4, at + 8));
      const data = b.subarray(at + 8, Math.min(b.length, at + 8 + len));
      if (type === 'eXIf') {
        const t = tiffTags(data);
        out.exif = true;
        out.make = !!t.tags && t.tags.includes(0x010f) && latin1(data).includes(${JSON.stringify(EXIF_MAKE)});
        out.gpsTag = !t.tags || t.tags.includes(0x8825);
        out.latitude = latitude(data, t.le);
      } else if (type === 'tEXt') {
        const k = data.indexOf(0);
        if (k > 0) text(latin1(data.subarray(0, k)), latin1(data.subarray(k + 1)));
      } else if (type === 'zTXt') {
        out.textCompressed = true;
      } else if (type === 'iTXt') {
        const k = data.indexOf(0);
        if (k > 0 && latin1(data.subarray(0, k)) !== 'XML:com.adobe.xmp') {
          if (data[k + 1]) out.textCompressed = true;
          else {
            let q = data.indexOf(0, k + 3) + 1;
            q = data.indexOf(0, q) + 1;
            text(latin1(data.subarray(0, k)), new TextDecoder().decode(data.subarray(q)));
          }
        } else if (k > 0) {
          out.xmp = true;
          if (data[k + 1]) out.compressed = true;
          else {
            let q = data.indexOf(0, k + 3) + 1;
            q = data.indexOf(0, q) + 1;
            const text = new TextDecoder().decode(data.subarray(q));
            out.rights = text.includes(${JSON.stringify(XMP_RIGHTS)});
            out.location = /gps|papeete/i.test(text);
          }
        }
      }
      at += 12 + len;
    }
    return out;
  };
  // LUM-14 r2 : ce que les octets d'un export contiennent, quel que soit le format : la latitude
  // GPS (24 octets, dans les deux ordres), un nom de propriété GPS, le lieu, la marque de l'appareil
  // (chaîne EXIF terminée par NUL), les droits ; puis la même chose en UTF-16 (XMP de calque du PSD).
  const scanMeta = (buf) => {
    const b = new Uint8Array(buf);
    const t = latin1(b);
    const wide = (w) => [Array.from(w, (ch) => ch + '\0').join(''), Array.from(w, (ch) => '\0' + ch).join('')];
    const has16 = (w) => wide(w).some((x) => t.includes(x));
    return { bytes: b.length, latitude: latitude(b, true) || latitude(b, false),
      gpsName: /GPSLatitude|GPSLongitude|GPSVersionID|GPSAltitude/i.test(t), place: t.includes('Papeete'),
      make: t.includes(${JSON.stringify(`${EXIF_MAKE} `)}), rights: t.includes(${JSON.stringify(XMP_RIGHTS)}),
      gps16: ['GPSLatitude', 'GPSLongitude', 'GPSVersionID'].some(has16), place16: has16('Papeete'), rights16: has16(${JSON.stringify(XMP_RIGHTS)}) };
  };
  const log = [];
  const sent = [];
  addEventListener('message', (e) => {
    const d = e.data;
    const plain = d !== null && typeof d === 'object' && Object.getPrototypeOf(d) === Object.prototype;
    const entry = { at: Math.round(performance.now()), origin: e.origin, fromEngine: e.source === engine.contentWindow, plain,
      keys: plain ? Object.keys(d) : null };
    try { entry.msg = plain ? summarize(d) : String(d).slice(0, 200); } catch (err) { entry.msg = 'illisible'; }
    if (plain && d.type === 'evora:export' && d.payload && d.payload.bytes instanceof ArrayBuffer) {
      try { entry.png = pngMeta(d.payload.bytes); } catch (err) { entry.png = 'illisible'; }
      try { entry.scan = scanMeta(d.payload.bytes); } catch (err) { entry.scan = 'illisible'; }
    }
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
      // Repère dans le journal : ce qui précède et ce qui suit l'envoi (fenêtres de LUM-04).
      log.push({ at: Math.round(performance.now()), bench: { id, type, re: msg.re || null } });
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
    geo(ref) { return Uint8Array.from(atob(${JSON.stringify(geo)}[ref]), (c) => c.charCodeAt(0)).buffer; },
    geoPng() { return this.geo(${JSON.stringify(GPS_REF)}); },
    // Étalonnage de l'instrument (LUM-14) : ce que pngMeta lit dans chaque photo du banc avant export.
    calibrate() { return Object.fromEntries(Object.keys(${JSON.stringify(geo)}).map((ref) => [ref, pngMeta(this.geo(ref))])); },
    // LUM-14 r2 : étalonnage du relevé des octets (scénario pont-politique), sur la photo d'origine.
    scanGeo() { return scanMeta(this.geoPng()); },
    fontInfo() { return ${JSON.stringify(font ? { family: font.family, format: font.format } : null)}; },
    font() { return Uint8Array.from(atob('${font ? font.b64 : ''}'), (c) => c.charCodeAt(0)).buffer; },
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

// Une touche nommée (Échap, Entrée) vers le canvas d'eframe, avec ses modificateurs.
const keyPress = (key, code, mods = {}) => `(() => {
  const c = document.getElementById('photocraft_canvas');
  c.focus();
  const init = { key: ${JSON.stringify(key)}, code: ${JSON.stringify(code)}, ctrlKey: ${!!mods.ctrl}, shiftKey: ${!!mods.shift},
    altKey: ${!!mods.alt}, bubbles: true, cancelable: true };
  c.dispatchEvent(new KeyboardEvent('keydown', init));
  c.dispatchEvent(new KeyboardEvent('keyup', init));
  return document.activeElement === c;
})()`;

// Une touche vers le canvas d'eframe ; rend vrai si le moteur l'a retenue (preventDefault).
const keyDown = (key, code, mods = {}) => `(() => {
  const c = document.getElementById('photocraft_canvas');
  c.focus();
  const init = { key: ${JSON.stringify(key)}, code: ${JSON.stringify(code)}, ctrlKey: ${!!mods.ctrl}, shiftKey: ${!!mods.shift},
    altKey: ${!!mods.alt}, bubbles: true, cancelable: true };
  const down = new KeyboardEvent('keydown', init);
  c.dispatchEvent(down);
  c.dispatchEvent(new KeyboardEvent('keyup', init));
  return down.defaultPrevented;
})()`;

function initPayload(capabilities) {
  // LUM-14 r2 : le scénario pont-politique n'impose que la politique de localisation.
  const prefs = POLICY ? { 'evora.locationPolicy': POLICY } : BENCH_PREFS;
  return { lang: 'fr', engineLang: 'fr', theme: 'light', accent: INIT_ACCENT, readOnly: false, storageNs: STORAGE_NS,
    capabilities, prefs, limits: { maxDocumentBytes: BENCH_MAX_BYTES, maxExportBytes: BENCH_MAX_BYTES } };
}

// LUM-06 r2 : une couleur #RRGGBB ou #RRGGBBAA du rapport de thème, en [r, g, b, a], ou null.
function rgba(text) {
  const m = /^#([0-9A-F]{2})([0-9A-F]{2})([0-9A-F]{2})([0-9A-F]{2})?$/i.exec(String(text || ''));
  return m ? [1, 2, 3, 4].map((i) => (m[i] === undefined ? 255 : parseInt(m[i], 16))) : null;
}
const hex6 = (c) => `#${c.slice(0, 3).map((v) => v.toString(16).padStart(2, '0')).join('').toUpperCase()}`;
// Rapport de contraste WCAG 2 de deux couleurs opaques.
function contrast(a, b) {
  const lin = (v) => { const c = v / 255; return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; };
  const lum = (c) => 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2]);
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

// LUM-06 r2 (E2-a, E2-b, E2-d) : ce que le thème doit tenir, quelle que soit la teinte.
export function themeProblems(t) {
  if (!t || typeof t !== 'object') return ['rapport de thème absent'];
  const c = Object.fromEntries(THEME_KEYS.map((k) => [k, rgba(t[k])]));
  const unread = THEME_KEYS.filter((k) => !c[k]);
  if (unread.length) return [`couleurs illisibles : ${unread.join(', ')}`];
  const out = [];
  if (hex6(c.fieldBorder) !== FIELD_BORDER) out.push(`bord de champ ${t.fieldBorder}, attendu ${FIELD_BORDER}`);
  if (t.inactiveBorder !== t.fieldBorder) out.push(`trait des champs au repos ${t.inactiveBorder}, attendu le bord de champ ${t.fieldBorder}`);
  for (const k of ['field', 'card']) {
    const r = contrast(c.fieldBorder, c[k]);
    if (r < 3) out.push(`bord de champ sur ${k} : ${r.toFixed(2)}:1`);
  }
  for (const k of ['canvas', 'canvasDot']) if (c[k][0] !== c[k][1] || c[k][1] !== c[k][2]) out.push(`${k} ${t[k]} non neutre`);
  for (const k of ['field', 'card', 'chrome', 'dock']) {
    const a = contrast(c.accent, c[k]);
    const f = contrast(c.activeBorder, c[k]);
    if (a < 3) out.push(`accent sur ${k} : ${a.toFixed(2)}:1`);
    if (f < 3) out.push(`anneau de focus (trait actif) sur ${k} : ${f.toFixed(2)}:1`);
  }
  if (c.accentBorder[3] !== 255 || hex6(c.accentBorder) !== hex6(c.accent)) out.push(`bord d'accent ${t.accentBorder}, attendu l'accent ${t.accent}, opaque`);
  if (t.activeBorder !== t.accentBorder) out.push(`trait actif ${t.activeBorder}, attendu le bord d'accent ${t.accentBorder}`);
  for (const k of ['card', 'chrome', 'field', 'dock', 'hover', 'pressed']) {
    const r = contrast(c.accentText, c[k]);
    if (r < 4.5) out.push(`texte d'accent sur ${k} : ${r.toFixed(2)}:1`);
  }
  if (t.selectionStroke !== t.accentText) out.push(`trait de sélection ${t.selectionStroke}, attendu le texte d'accent ${t.accentText}`);
  const p = contrast(c.primaryText, c.primaryBg);
  if (p < 4.5) out.push(`bouton principal : ${p.toFixed(2)}:1`);
  return out;
}

// LUM-06 r2 (E2-c) : le thème et l'accent demandés par la coquille sont ceux du moteur.
export function accentProblems(t, want) {
  if (!t || typeof t !== 'object') return ['rapport de thème absent'];
  const out = [];
  if (t.kind !== want.kind) out.push(`thème ${t.kind}, attendu ${want.kind}`);
  const bg = rgba(t.primaryBg);
  const soft = rgba(t.accentSoft);
  const sent = rgba(want.accent);
  if (!bg || hex6(bg) !== want.accent.toUpperCase()) out.push(`bouton principal ${t.primaryBg}, attendu l'accent de la coquille ${want.accent}`);
  // Le fond d'accent est translucide : egui le garde prémultiplié, et le rapport le rend
  // déprémultiplié, à quelques unités près (alpha 40 : jusqu'à 3,2 par canal ; run 40077170134).
  if (!soft || soft.slice(0, 3).some((v, i) => Math.abs(v - sent[i]) > SOFT_TOLERANCE)) {
    out.push(`fond d'accent ${t.accentSoft}, attendu l'accent de la coquille ${want.accent} (à ${SOFT_TOLERANCE} près par canal)`);
  }
  return out;
}

// LUM-14 r2 et LUM-15 : ce que le moteur a pris et refusé des préférences de evora:init.
export function prefsProblems(p) {
  if (!p || typeof p !== 'object') return ['rapport des préférences absent'];
  const out = [];
  if (p.locationPolicy !== BENCH_PREFS['evora.locationPolicy']) out.push(`politique de localisation ${p.locationPolicy}, attendue ${BENCH_PREFS['evora.locationPolicy']}`);
  if (p.canvasColor !== 'mediumGray') out.push(`entourage du canevas ${p.canvasColor}, attendu mediumGray`);
  const drawn = rgba(p.canvasEffective);
  if (!drawn || hex6(drawn) !== MEDIUM_GRAY) out.push(`entourage dessiné ${p.canvasEffective}, attendu ${MEDIUM_GRAY}`);
  if (JSON.stringify(p.applied) !== JSON.stringify(['interface.canvasColor'])) out.push(`préférences prises ${JSON.stringify(p.applied)}, attendu ["interface.canvasColor"]`);
  if (!Array.isArray(p.refused) || JSON.stringify([...p.refused].sort()) !== JSON.stringify(BENCH_REFUSED)) out.push(`préférences refusées ${JSON.stringify(p.refused)}, attendu ${JSON.stringify(BENCH_REFUSED)}`);
  return out;
}

export class EhpBench {
  constructor({ c, page, frame, engineOrigin, parentOrigin, log }) {
    Object.assign(this, { c, page, frame, engineOrigin, parentOrigin, log });
    this.ids = {};
    this.problems = [];
    this.state = { log: [], sent: [] };
    this.readyCaps = [];
    this.storageLog = [];
    // Témoins sabordés : sabotages joués (banc), ids des messages fabriqués, et chaque contrôle nommé
    // du stockage avec les étiquettes qu'il a fait rougir.
    this.sabotaged = [];
    this.injectedIds = [];
    this.checks = [];
    // LUM-10 : chaque evora:command envoyé ; LUM-04 : fenêtres du journal où le document doit rester
    // modifié (aucun evora:dirty false).
    this.ids.commands = [];
    this.windows = [];
    // LUM-06 r2 : relevés diag.stats du thème, chacun avec le thème et l'accent attendus.
    this.ids.themeChecks = [];
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
      const v = JSON.parse(await this.evaluate(this.page, 'JSON.stringify(window.__ehp ? { log: __ehp.log, sent: __ehp.sent } : null)'));
      // Harnais pas encore chargé (null) : le journal reste celui du tour précédent.
      if (v) this.state = v;
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

  // Un message du moteur qui satisfait pred, attendu au plus ms millisecondes ; avec `from`, seuls
  // comptent les messages entrés au journal à partir de cette position (le journal ne fait que
  // croître).
  async waitEngine(pred, ms, from = 0) {
    const end = Date.now() + ms;
    for (;;) {
      await this.refresh();
      const hit = this.state.log.slice(from).find((e) => e.fromEngine && e.plain && pred(e.msg));
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

  // Témoin « banc » : le sabotage `tag` de l'inventaire. Un sabotage « message » poste au parent,
  // depuis la fenêtre du moteur, le message littéral `script` (sans id : un ULID neuf lui est donné) ;
  // un sabotage « stockage » exécute `script` dans le document moteur juste avant son contrôle.
  async sabotage(tag, what, script) {
    if (SABOTAGE !== 'banc') return;
    const spec = BANC_SABOTAGES.find((s) => s.tag === `ehp:${tag}`);
    if (!spec) throw new Error(`sabotage hors inventaire : ${tag}`);
    let id = null;
    if (spec.kind === 'message') {
      id = await this.evaluate(this.frame(), `(() => {
        const A = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
        let t = Date.now(), id = '';
        for (let i = 0; i < 10; i++) { id = A[t % 32] + id; t = Math.floor(t / 32); }
        for (const b of crypto.getRandomValues(new Uint8Array(16))) id += A[b % 32];
        const m = ${script};
        m.id = id;
        parent.postMessage(m, ${JSON.stringify(this.parentOrigin)});
        return id;
      })()`);
      this.injectedIds.push(id);
    } else {
      await this.evaluate(this.frame(), script);
    }
    this.sabotaged.push({ tag: spec.tag, kind: spec.kind, at: spec.at || null, what, id });
    this.log(`[ehp] sabordage ${spec.tag}${id ? ` (message ${id})` : ''} : ${what}`);
  }

  // Témoin « banc » : remise en état du stockage après le contrôle visé (aucun débordement).
  async restore(script) {
    if (SABOTAGE !== 'banc') return;
    await this.evaluate(this.frame(), script);
  }

  // Un contrôle nommé du stockage : les étiquettes qu'il a fait rougir, pour l'attribution des témoins.
  async checkpoint(label, fn) {
    const before = this.problems.length;
    await fn();
    this.checks.push({ label, tags: [...new Set(this.problems.slice(before).map(tagOf).filter(Boolean))] });
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
    this.readyVersion = ready.msg.payload && ready.msg.payload.version;
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
    await this.sabotage('ready-count', 'second evora:ready', `{ v: 1, type: 'evora:ready', payload: ${JSON.stringify(ready.msg.payload)} }`);
    await this.sabotage('answer-v2', 'réponse au evora:init de version 2', `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(this.ids.v2)}, payload: { ok: true } }`);
    await this.sabotage('answer-envelope', 'réponse au evora:init à champ en trop', `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(this.ids.extraField)}, payload: { ok: true } }`);
    await this.sabotage('answer-foreign', 'réponse au témoin d\'origine non admise', `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(this.ids.foreign)}, payload: { ok: true } }`);
    await this.sabotage('answer-source', 'réponse au témoin de même origine, autre fenêtre', `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(this.ids.sibling)}, payload: { ok: true } }`);
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
    return this.pressExpr(shortcut(key), `Ctrl+${key.toUpperCase()}`, pred, what);
  }

  // Une touche (expression évaluée dans le document moteur), renouvelée tant que pred n'est pas
  // observé chez un message du moteur (jamais un message fabriqué par le témoin « banc ») arrivé
  // APRÈS la touche : un message plus ancien qui satisfait pred (un evora:dirty true d'une étape
  // précédente) ferait partir la suite avant l'effet de la touche (run 38984687150 : « modification
  // tardive » répondue « clean », la touche prise en compte après la demande).
  async pressExpr(expr, label, pred, what) {
    const mark = (await this.refresh()).log.length;
    for (let attempt = 1; attempt <= 3; attempt++) {
      const focused = await this.evaluate(this.frame(), expr, true);
      const hit = await this.waitEngine((m) => pred(m) && !this.injectedIds.includes(m.id), 5000, mark);
      if (hit) return hit;
      this.log(`[ehp] ${label} (essai ${attempt}, canvas focalisé : ${focused}) : ${what} non observé`);
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
    // LUM-06 r2, LUM-14 r2 et LUM-15 : le thème clair à l'accent de la coquille, et ce que le moteur a
    // pris des préférences de evora:init (jugés par verify()).
    const first = await this.themeCheck('init', { kind: 'evoraLight', accent: INIT_ACCENT }, true);
    if (first && first.prefs) {
      await this.sabotage('prefs-init', 'diag.stats fabriqué, préférences de evora:init ignorées',
        `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(this.ids.prefsStats)}, payload: ${JSON.stringify({ ok: true, result: { ...first, prefs: { ...first.prefs, canvasColor: 'default', applied: [], refused: [] } } })} }`);
    }
    await this.sabotage('envelope', 'evora:dirty à champ d\'enveloppe en trop', `{ v: 1, type: 'evora:dirty', payload: { dirty: false, docRef: ${JSON.stringify(DOC_REF)} }, extra: true }`);
    await this.sabotage('save-shape', 'evora:save aux octets en vue typée', `{ v: 1, type: 'evora:save', payload: { docRef: ${JSON.stringify(DOC_REF)}, name: 'evora-smoke.pcraft', saveAs: false, bytes: new Uint8Array(4) } }`);
    const dirty = await this.press('i', (m) => m.type === 'evora:dirty' && m.payload.docRef === DOC_REF && m.payload.dirty === true, 'evora:dirty true');
    if (!dirty) {
      this.fail('dirty', 'evora:dirty {dirty: true} absent après une modification (Ctrl+I, Négatif)');
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
    // LUM-04 : en vol jusqu'à evora:saved (aucun dirty false avant), puis enregistré.
    if (save) {
      await this.sabotage('saved-ack-ok', 'evora:dirty false pendant l\'enregistrement en vol',
        `{ v: 1, type: 'evora:dirty', payload: { dirty: false, docRef: ${JSON.stringify(DOC_REF)} } }`);
      await sleep(1500);
      const saved = await this.saved(save.msg.id, { ok: true, version: 1 });
      this.windows.push({ tag: 'saved-ack-ok', what: 'enregistrement en vol', from: save.msg.id, to: saved, docRef: DOC_REF });
      if (!(await this.until(() => this.dirtyAfter(saved, false), 10000))) {
        this.fail('saved-ack-ok', 'evora:dirty {dirty: false} absent après evora:saved {ok: true} d\'un document inchangé depuis l\'émission');
      }
    }
    this.ids.clean = await this.send('evora:request-save', `{ reason: 'autosave' }`);
    const cleanAck = await this.waitEngine(this.response('evora:ack', this.ids.clean), 15000);
    if (!cleanAck || cleanAck.msg.payload.ok !== true || cleanAck.msg.payload.code !== 'clean') {
      this.fail('clean', `evora:request-save sur un document enregistré : attendu evora:ack {ok: true, code: "clean"}, reçu ${JSON.stringify(cleanAck && cleanAck.msg)}`);
    }
    // Un evora:save arrivé APRÈS le raccourci : jamais un enregistrement antérieur sans re.
    const savesBefore = new Set(this.engineMessages().filter((e) => e.msg.type === 'evora:save').map((e) => e.msg.id));
    const action = await this.press('s', (m) => m.type === 'evora:save' && !m.re && !savesBefore.has(m.id), 'evora:save (Ctrl+S)');
    if (!action) this.fail('save-action', 'aucun evora:save après Ctrl+S (Fichier › Enregistrer)');
    else if ((action.msg.payload || {}).docRef !== DOC_REF || !((action.msg.payload.bytes || {}).$arrayBuffer > 0)) {
      this.fail('save-action', `evora:save de Ctrl+S inattendu : ${JSON.stringify(action.msg.payload)}`);
    }
    if (action) await this.saved(action.msg.id, { ok: true, version: 2 });
    this.ids.theme = await this.send('evora:theme', `{ theme: 'dark', accent: '#4F46E5' }`);
    this.ids.lang = await this.send('evora:lang', `{ lang: 'de', engineLang: 'de' }`);
    for (const step of ['theme', 'lang']) {
      const ack = await this.waitEngine(this.response('evora:ack', this.ids[step]), 15000);
      if (!ack || ack.msg.payload.ok !== true) this.fail(step, `accusé de evora:${step} absent ou négatif : ${JSON.stringify(ack && ack.msg)}`);
    }
    // Préférences enregistrées par le moteur, sous l'espace de noms de evora:init.
    let st = null;
    const end = Date.now() + 10000;
    do {
      await sleep(500);
      st = await this.storage();
    } while (Date.now() < end && (st.theme !== 'evora' || st.language !== 'de'));
    this.storageLog.push({ phase: 'session', ...st });
    if (st.theme !== 'evora') this.fail('theme', `thème enregistré après evora:theme dark : ${st.theme}, attendu evora (${STORAGE_NS.slice(0, 8)}…:${PREFS})`);
    if (st.language !== 'de') this.fail('lang', `langue enregistrée après evora:lang de : ${st.language}, attendue de (${STORAGE_NS.slice(0, 8)}…:${PREFS})`);
    await this.themes();
    await this.savedAck();
    await this.exports();
    await this.openNew();
    await this.commands();
    await this.menusMasked();
    await this.fonts();
    await this.focusOut();
    await this.exportNoGps();
    // En dernier : le sélecteur ouvre un onglet du moteur, qui devient le document actif.
    await this.pick();
    await this.storagePhases(st);
  }

  // LUM-06 r2 : un diag.stats relevé pour le thème (et, à l'init, pour les préférences de evora:init),
  // jugé par verify() ; rend le résultat du moteur, ou null.
  async themeCheck(step, want, withPrefs = false) {
    const ack = await this.command('diag.stats');
    const id = this.ids.lastCommand;
    this.ids.themeChecks.push({ id, step, ...want });
    if (withPrefs) this.ids.prefsStats = id;
    return ack && ack.ok === true ? ack.result : null;
  }

  // LUM-06 r2 : le sombre en indigo (evora:theme de session()), puis le clair et le sombre en
  // d'autres accents, enfin retour au sombre en indigo pour la suite du banc. Le témoin « banc »
  // fabrique une réponse fausse à deux de ces relevés.
  async themes() {
    const dark = await this.themeCheck('sombre', { kind: 'evora', accent: EVORA_ACCENT });
    const results = [];
    for (const step of THEME_STEPS) {
      const id = await this.send('evora:theme', `{ theme: ${JSON.stringify(step.theme)}, accent: ${JSON.stringify(step.accent)} }`);
      const ack = await this.waitEngine((m) => m.type === 'evora:ack' && m.re === id && !this.injectedIds.includes(m.id), 15000);
      if (!ack || ack.msg.payload.ok !== true) this.fail('theme-accent', `accusé de evora:theme ${step.theme} ${step.accent} absent ou négatif : ${JSON.stringify(ack && ack.msg)}`);
      results.push(await this.themeCheck(`${step.theme} ${step.accent}`, { kind: step.kind, accent: step.accent }));
    }
    const back = await this.send('evora:theme', `{ theme: 'dark', accent: ${JSON.stringify(EVORA_ACCENT)} }`);
    await this.waitEngine((m) => m.type === 'evora:ack' && m.re === back && !this.injectedIds.includes(m.id), 15000);
    const sombre = this.ids.themeChecks.find((c) => c.step === 'sombre');
    if (dark && dark.theme) {
      await this.sabotage('theme-r2', 'diag.stats fabriqué, bord de champ de l\'E2 d\'origine (#3F3F46, 1,5:1)',
        `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(sombre.id)}, payload: ${JSON.stringify({ ok: true, result: { ...dark, theme: { ...dark.theme, fieldBorder: '#3F3F46FF', inactiveBorder: '#3F3F46FF' } } })} }`);
    }
    const light = this.ids.themeChecks.find((c) => c.step === `${THEME_STEPS[0].theme} ${THEME_STEPS[0].accent}`);
    if (results[0] && results[0].theme) {
      await this.sabotage('theme-accent', 'diag.stats fabriqué, accent de la coquille ignoré (indigo)',
        `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(light.id)}, payload: ${JSON.stringify({ ok: true, result: { ...results[0], theme: { ...results[0].theme, primaryBg: '#4F46E5FF', accentSoft: '#4F46E528' } } })} }`);
    }
  }

  // LUM-14 r2 (scénario pont-politique, build de fumée) : la photo géolocalisée, deux calques à XMP
  // propre (Layer::xmp et bloc shmd brut), puis un export par format par le service d'export du
  // moteur ; les octets reçus sont jugés par verify() (politiqueVerdict). D'abord l'étalonnage du
  // relevé : la photo d'origine doit s'y lire localisée, avec la marque et les droits.
  async politique() {
    this.politiqueMode = true;
    this.ids.policyExports = {};
    const cal = await this.evaluate(this.page, '__ehp.scanGeo()');
    if (!cal || !cal.latitude || !cal.place || !cal.make || !cal.rights) {
      this.fail('politique-temoin', `étalonnage : la photo ne se lit pas localisée, avec marque et droits : ${JSON.stringify(cal)}`);
    }
    const open = await this.send('evora:open', `{ docRef: ${JSON.stringify(POLICY_REF)}, name: 'evora-politique.png', bytes: __ehp.geo(${JSON.stringify(GPS_REF)}) }`);
    const opened = await this.waitEngine(this.response('evora:ack', open), 30000);
    if (!opened || opened.msg.payload.ok !== true) {
      this.fail('politique-temoin', `ouverture de la photo : ${JSON.stringify(opened && opened.msg)}`);
      return;
    }
    const layers = await this.send(SMOKE_LAYER_XMP, `{ xmp: ${JSON.stringify(LAYER_XMP_FIXTURE)} }`);
    const added = await this.waitEngine(this.response('evora:ack', layers), 15000);
    if (!added || added.msg.payload.ok !== true) {
      this.fail('politique-temoin', `calques à XMP propre : ${JSON.stringify(added && added.msg)}`);
      return;
    }
    for (const ext of POLICY_FORMATS) {
      const mark = (await this.refresh()).log.length;
      const id = await this.send(SMOKE_EXPORT, `{ ext: ${JSON.stringify(ext)} }`);
      const out = await this.waitEngine((m) => m.type === 'evora:export', 60000, mark);
      const done = await this.waitEngine(this.response('evora:ack', id), 15000, mark);
      if (!out || !done || done.msg.payload.ok !== true) {
        this.fail('politique-temoin', `export ${ext} : evora:export ${out ? out.msg.id : 'absent'}, accusé ${JSON.stringify(done && done.msg)}`);
        continue;
      }
      this.ids.policyExports[ext] = out.msg.id;
      await this.send('evora:ack', '{ ok: true }', { re: out.msg.id });
    }
  }

  // LUM-14 r2 : verdict des exports du scénario pont-politique, sur les octets relevés par le harnais.
  politiqueVerdict(msgs) {
    for (const ext of POLICY_FORMATS) {
      const id = (this.ids.policyExports || {})[ext];
      if (!id) {
        this.fail('politique-temoin', `export ${ext} non joué`);
        continue;
      }
      const e = msgs.find((x) => x.msg && x.msg.id === id);
      const sc = e && e.scan;
      const where = `evora:export id ${id} (${ext}, politique ${POLICY})`;
      if (!sc || typeof sc !== 'object') {
        this.fail('politique-temoin', `${where} : octets illisibles (${JSON.stringify(sc)})`);
        continue;
      }
      this.log(`[politique] ${POLICY} ${ext} : ${JSON.stringify(sc)}`);
      const strict = POLICY_STRICT.includes(ext);
      if (POLICY !== 'all') {
        if (sc.latitude || sc.gpsName || sc.place) this.fail('politique-lieu', `${where} : localisation présente (latitude ${sc.latitude}, nom GPS ${sc.gpsName}, lieu ${sc.place})`);
        if (sc.gps16 || sc.place16) this.fail('politique-calque', `${where} : localisation dans un XMP de calque (GPS ${sc.gps16}, lieu ${sc.place16})`);
      }
      if (POLICY === 'rights') {
        if (sc.make) this.fail('politique-droits', `${where} : marque de l'appareil (EXIF) présente sous « droits seuls »`);
        if (sc.rights16) this.fail('politique-droits', `${where} : XMP de calque présent sous « droits seuls »`);
        if (strict && !sc.rights) this.fail('politique-temoin', `${where} : droits absents : la recette ne prouve rien`);
      }
      if (POLICY === 'none' && strict) {
        if (!sc.make || !sc.rights) this.fail('politique-temoin', `${where} : marque (${sc.make}) ou droits (${sc.rights}) absents : la recette ne prouve rien`);
        if (POLICY_LAYERED.includes(ext) && !sc.rights16) this.fail('politique-temoin', `${where} : XMP de calque absent (droits en UTF-16) : la recette ne prouve rien`);
      }
      if (POLICY === 'all' && strict) {
        if (!(sc.latitude && sc.place && sc.make && sc.rights)) {
          this.fail('politique-toutes', `${where} : sous « toutes », latitude ${sc.latitude}, lieu ${sc.place}, marque ${sc.make}, droits ${sc.rights} (attendus présents)`);
        }
        if (POLICY_LAYERED.includes(ext) && !(sc.gps16 && sc.place16)) this.fail('politique-toutes', `${where} : XMP de calque sans sa localisation sous « toutes »`);
      }
    }
  }

  // LUM-09 : diag.menus (relevé par commands()) liste les articles masqués, jugés par verify() ;
  // Ctrl+P (Fichier › Imprimer, masqué) n'ouvre aucun dialogue.
  async menusMasked() {
    const before = await this.stats();
    for (let attempt = 1; attempt <= 2; attempt++) {
      await this.evaluate(this.frame(), shortcut('p'), true);
      await sleep(1000);
    }
    const after = await this.stats();
    if (!before || !after || after.dialogs !== before.dialogs) {
      this.fail('menus-masked', `Ctrl+P (Fichier › Imprimer, masqué) : dialogues ${before && before.dialogs} puis ${after && after.dialogs}`);
      // Le dialogue ouvert à tort est refermé (Échap), pour que les étapes suivantes partent du même
      // état : un défaut de LUM-09 ne doit rougir qu'à son détecteur.
      if (before) {
        for (let attempt = 1; attempt <= 3 && !(await this.dialogsReach(before.dialogs, () => this.stats(), 2000)); attempt++) {
          await this.evaluate(this.frame(), keyPress('Escape', 'Escape'), true);
        }
      }
    }
    if (this.ids.menus) {
      await this.sabotage('menus-masked', 'diag.menus fabriqué, Fichier › Imprimer vivant et rien de masqué',
        `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(this.ids.menus)}, payload: { ok: true, result: { live: ['file.print'], enabled: [], masked: [] } } }`);
    }
  }

  // LUM-11 : une police servie, puis des octets qui n'en sont pas une ; les accusés sont jugés par
  // verify() ; diag.stats liste les familles enregistrées.
  async fonts() {
    const info = await this.evaluate(this.page, '__ehp.fontInfo()');
    if (!info) {
      this.fail('font-inject', 'aucune police TrueType ou OpenType lisible dans le dossier des polices servies (fonts/manifest.txt)');
      return;
    }
    this.ids.font = await this.send('evora:font', `{ family: ${JSON.stringify(info.family)}, format: ${JSON.stringify(info.format)}, bytes: __ehp.font() }`);
    this.ids.fontBad = await this.send('evora:font', `{ family: 'Evora Banc Illisible', format: 'ttf', bytes: __ehp.png() }`);
    for (const key of ['font', 'fontBad']) {
      await this.waitEngine((m) => m.type === 'evora:ack' && m.re === this.ids[key] && !this.injectedIds.includes(m.id), 15000);
    }
    const r = await this.stats();
    this.log(`[ehp] police ${info.family} (${info.format}) : familles enregistrées ${JSON.stringify(r && r.fonts)}`);
    if (!r || !Array.isArray(r.fonts) || r.fonts.length < 1) this.fail('font-inject', `familles enregistrées (diag.stats) après evora:font : ${JSON.stringify(r && r.fonts)}`);
    await this.sabotage('font-inject', 'accusé positif fabriqué de la police illisible',
      `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(this.ids.fontBad)}, payload: { ok: true } }`);
  }

  // LUM-13 : F6 retenue, un seul evora:focus-out ; Ctrl+F6 (sans raccourci dans le moteur, quand Maj+F6
  // ouvrirait Contour progressif) laissée passer, sans message.
  async focusOut() {
    const mark = (await this.refresh()).log.length;
    const held = await this.evaluate(this.frame(), keyDown('F6', 'F6'), true);
    const out = await this.waitEngine((m) => m.type === 'evora:focus-out' && !this.injectedIds.includes(m.id), 5000, mark);
    this.ids.focusOut = out ? out.msg.id : null;
    if (!out) this.fail('focus-out', 'F6 avec ui.focus-out effective : aucun evora:focus-out');
    if (!held) this.fail('focus-out', 'F6 non retenue par le moteur : le canevas et le navigateur la reçoivent');
    const mark2 = (await this.refresh()).log.length;
    const shifted = await this.evaluate(this.frame(), keyDown('F6', 'F6', { ctrl: true }), true);
    await sleep(1500);
    await this.refresh();
    const extra = this.state.log.slice(mark2).filter((e) => e.fromEngine && e.plain && e.msg.type === 'evora:focus-out' && !this.injectedIds.includes(e.msg.id));
    if (shifted || extra.length) this.fail('focus-out', `Ctrl+F6 retenue (${shifted}) ou suivie d'un evora:focus-out (${extra.map((e) => e.msg.id).join(', ') || 'aucun'})`);
    await this.sabotage('focus-out', 'evora:focus-out à charge utile non vide', `{ v: 1, type: 'evora:focus-out', payload: { touche: 'F6' } }`);
  }

  // LUM-14 : les photos géolocalisées (la première, puis les trois du constat de la revue du
  // 2026-10-23 : profil brut EXIF en texte, deux pointeurs GPS, noms de lieu sous un autre préfixe),
  // ouvertes par la coquille puis exportées (Exporter sous, Entrée) ; les métadonnées des PNG reçus
  // sont jugées par verify(). D'abord l'étalonnage de l'instrument : chaque photo, avant export, doit
  // se lire localisée (sinon le banc ne prouve rien).
  async exportNoGps() {
    const cal = await this.evaluate(this.page, '__ehp.calibrate()');
    const located = {
      [GPS_REF]: (g) => g.gpsTag && g.latitude && g.location,
      [RAW_REF]: (g) => g.raw && g.rawGps && g.rawLatitude,
      [DUAL_REF]: (g) => g.gpsTag && g.latitude,
      [PLACE_REF]: (g) => g.location && g.rights,
      [CASE_REF]: (g) => g.location && g.rights,
      [SUB_REF]: (g) => g.latitude && !g.gpsTag,
      [TEXT_REF]: (g) => g.textLocation && g.textRights,
    };
    for (const f of GEO_FIXTURES) {
      const g = cal && cal[f.ref];
      if (!g || !located[f.ref](g)) this.fail('bench', `étalonnage : ${f.name} ne se lit pas localisée avant export : ${JSON.stringify(g)}`);
    }
    this.ids.gpsExports = {};
    for (const f of GEO_FIXTURES) {
      const id = await this.send('evora:open', `{ docRef: ${JSON.stringify(f.ref)}, name: ${JSON.stringify(f.name)}, bytes: __ehp.geo(${JSON.stringify(f.ref)}) }`);
      const ack = await this.waitEngine((m) => m.type === 'evora:ack' && m.re === id && !this.injectedIds.includes(m.id), 30000);
      if (!ack || ack.msg.payload.ok !== true) {
        this.fail('export-no-gps', `ouverture de ${f.name} : ${JSON.stringify(ack && ack.msg)}`);
        continue;
      }
      this.ids.lastGeoRef = f.ref;
      const out = await this.exportAs('export-no-gps');
      if (out) {
        this.ids.gpsExports[f.ref] = out.msg.id;
        await this.send('evora:ack', '{ ok: true }', { re: out.msg.id });
      }
    }
    await this.sabotage('export-no-gps', 'evora:export d\'une photo au profil brut EXIF encore géolocalisé',
      `{ v: 1, type: 'evora:export', payload: { docRef: ${JSON.stringify(RAW_REF)}, name: 'evora-brut.png', mime: 'image/png', bytes: Uint8Array.from(atob('${rawPng().toString('base64')}'), (c) => c.charCodeAt(0)).buffer } }`);
  }

  // LUM-02 : le sélecteur de la coquille, pour Placer (import), Ouvrir (open), une annulation, et
  // un nouveau Ouvrir après l'annulation (aucun sélecteur resté ouvert). Les evora:pick sont jugés
  // par verify() (schéma) ; ici, leur objet et l'effet des fichiers remis.
  async pick() {
    const picked = new Set();
    const nextPick = (m) => m.type === 'evora:pick' && !picked.has(m.id) && !this.injectedIds.includes(m.id);
    // Placer : evora:command file.placeEmbedded, puis le fichier de la coquille placé sur la photo.
    const mark = (await this.refresh()).log.length;
    const placeAck = await this.command('file.placeEmbedded');
    const imp = await this.waitEngine(nextPick, 8000, mark);
    if (!placeAck || placeAck.ok !== true || !imp) {
      this.fail('pick', `Fichier › Placer : accusé ${JSON.stringify(placeAck)}, evora:pick ${JSON.stringify(imp && imp.msg)}`);
    } else {
      picked.add(imp.msg.id);
      const p = imp.msg.payload || {};
      if (p.purpose !== 'import' || p.multiple !== false || !Array.isArray(p.accept) || !p.accept.includes('image/*')) {
        this.fail('pick', `evora:pick de Fichier › Placer inattendu (import, un fichier, image/*) : ${JSON.stringify(p)}`);
      }
      const at = (await this.refresh()).log.length;
      await this.send('evora:file', `{ path: '/evora/${PICK_UID}/evora-place.png', name: 'evora-place.png', bytes: __ehp.png() }`, { re: imp.msg.id });
      await this.send('evora:ack', '{ ok: true }', { re: imp.msg.id });
      const target = this.ids.lastGeoRef || GPS_REF;
      const placed = await this.waitEngine((m) => m.type === 'evora:dirty' && m.payload.docRef === target && m.payload.dirty === true && !this.injectedIds.includes(m.id), 10000, at);
      if (!placed) this.fail('pick', 'le fichier remis par la coquille n\'a pas été placé (aucun evora:dirty true du document actif)');
      // Transformation manuelle du calque placé : validée.
      await this.evaluate(this.frame(), keyPress('Enter', 'Enter'), true);
      await sleep(800);
    }
    // Ouvrir : Ctrl+O, puis le fichier de la coquille ouvert dans un onglet du moteur.
    const before = await this.stats();
    const open = await this.press('o', nextPick, 'evora:pick (Ctrl+O)');
    if (!open) this.fail('pick', 'aucun evora:pick après Ctrl+O (Fichier › Ouvrir)');
    else {
      picked.add(open.msg.id);
      const p = open.msg.payload || {};
      if (p.purpose !== 'open' || p.multiple !== true || !Array.isArray(p.accept) || !p.accept.includes('.pcraft') || !p.accept.includes('image/*')) {
        this.fail('pick', `evora:pick de Fichier › Ouvrir inattendu (open, plusieurs, .pcraft et image/*) : ${JSON.stringify(p)}`);
      }
      await this.send('evora:file', `{ path: '/evora/${PICK_UID}/evora-ouvert.png', name: 'evora-ouvert.png', bytes: __ehp.png() }`, { re: open.msg.id });
      await this.send('evora:ack', '{ ok: true }', { re: open.msg.id });
      let r = null;
      const end = Date.now() + 10000;
      do {
        await sleep(500);
        r = await this.stats();
      } while (Date.now() < end && !(r && before && r.documents === before.documents + 1));
      if (!before || !r || r.documents !== before.documents + 1) {
        this.fail('pick', `le fichier remis par la coquille ne s'est pas ouvert : ${before && before.documents} document(s) puis ${r && r.documents}`);
      }
    }
    // Annulation, puis un nouveau sélecteur : rien ne s'ouvre, et rien n'est resté ouvert.
    for (const step of ['annulé', 'après l\'annulation']) {
      const was = await this.stats();
      const again = await this.press('o', nextPick, `evora:pick (Ctrl+O, ${step})`);
      if (!again) {
        this.fail('pick', `aucun evora:pick après Ctrl+O (${step})`);
        continue;
      }
      picked.add(again.msg.id);
      await this.send('evora:ack', `{ ok: false, code: 'cancelled' }`, { re: again.msg.id });
      await sleep(1500);
      const now = await this.stats();
      if (!was || !now || now.documents !== was.documents || now.dialogs !== was.dialogs) {
        this.fail('pick', `après un sélecteur annulé (${step}), diag.stats : ${JSON.stringify(now)} (avant : ${was && was.documents} document(s), ${was && was.dialogs} dialogue(s))`);
      }
    }
    await this.sabotage('pick', 'evora:pick hors schéma (objet inconnu, accept vide)', `{ v: 1, type: 'evora:pick', payload: { purpose: 'ouvrir', accept: [], multiple: true } }`);
  }

  // LUM-04 : accusé d'enregistrement (doc.saved-ack). Après un refus (conflit) et après un accusé
  // positif d'un enregistrement que le document a dépassé, il reste modifié : aucun dirty false
  // (jugé par verify() sur la fenêtre du journal), et la demande suivante rend un evora:save. La garde
  // beforeunload du moteur se tait sous le pont. evora:open du même docRef remplace le document
  // modifié (S-ACK-6). Chaque étape repart d'un document enregistré (accusé positif, cleanAfter).
  async savedAck() {
    const dirtyTrue = (m) => m.type === 'evora:dirty' && m.payload.docRef === DOC_REF && m.payload.dirty === true;
    const fake = `{ v: 1, type: 'evora:dirty', payload: { dirty: false, docRef: ${JSON.stringify(DOC_REF)} } }`;
    // Conflit : Ctrl+I (Image › Réglages › Négatif) modifie le document ; l'enregistrement est refusé.
    // Depuis v0.15.1, une sélection seule (Ctrl+A, Ctrl+D) laisse propre un document enregistré.
    await this.press('i', dirtyTrue, 'evora:dirty true (Ctrl+I)');
    const conflict = await this.requestSave('conflit');
    if (conflict) {
      const refused = await this.saved(conflict, { ok: false, error: 'conflict' });
      await this.sabotage('saved-ack-conflict', 'evora:dirty false après un refus (conflit)', fake);
      await sleep(1500);
      const next = await this.requestSave('demande après le conflit', 'saved-ack-conflict');
      this.windows.push({ tag: 'saved-ack-conflict', what: 'après evora:saved {ok: false, error: "conflict"}', from: refused, to: this.ids.lastRequest, docRef: DOC_REF });
      if (next) await this.cleanAfter(next);
    }
    // Modification tardive : Ctrl+I, demande, Ctrl+I après l'émission (révision lue par diag.stats),
    // puis accusé positif de l'enregistrement dépassé.
    await this.press('i', dirtyTrue, 'evora:dirty true (Ctrl+I)');
    const late = await this.requestSave('modification tardive');
    if (late) {
      const before = await this.revision();
      let after = before;
      for (let attempt = 1; attempt <= 3 && !(after > before); attempt++) {
        await this.evaluate(this.frame(), shortcut('i'), true);
        await sleep(700);
        after = await this.revision();
      }
      if (!(Number.isInteger(before) && after > before)) this.fail('bench', `modification tardive non jouée : révision ${before} puis ${after} (diag.stats)`);
      const accepted = await this.saved(late, { ok: true, version: this.nextVersion() });
      await this.sabotage('saved-ack-late-edit', 'evora:dirty false après l\'accusé positif d\'un enregistrement dépassé', fake);
      await sleep(1500);
      const next = await this.requestSave('demande après l\'accusé positif dépassé', 'saved-ack-late-edit');
      this.windows.push({ tag: 'saved-ack-late-edit', what: 'après evora:saved {ok: true} d\'un enregistrement dépassé', from: accepted, to: this.ids.lastRequest, docRef: DOC_REF });
      if (next) await this.cleanAfter(next);
    }
    // Garde beforeunload, document modifié : sous le pont, le moteur ne retient pas la page.
    await this.press('i', dirtyTrue, 'evora:dirty true (Ctrl+I)');
    const stats = await this.stats();
    const modified = !!stats && stats.open.some((d) => d.docRef === DOC_REF && d.dirty === true);
    await this.sabotage('beforeunload', 'garde beforeunload posée dans le document moteur',
      `(() => { window.__evoraGuard = (e) => e.preventDefault(); addEventListener('beforeunload', window.__evoraGuard); return true; })()`);
    const prevented = await this.evaluate(this.frame(), `(() => { const e = new Event('beforeunload', { cancelable: true }); dispatchEvent(e); return e.defaultPrevented; })()`);
    await this.checkpoint('départ:garde', () => {
      if (!modified) this.fail('bench', `document non modifié (diag.stats ${JSON.stringify(stats && stats.open)}) : garde beforeunload non éprouvée`);
      else if (prevented) this.fail('beforeunload', 'la garde beforeunload du moteur retient la page sous le pont (seconde invite après celle de la coquille)');
    });
    await this.restore(`(() => { removeEventListener('beforeunload', window.__evoraGuard); return true; })()`);
    // S-ACK-6 : evora:open du même docRef, document modifié : remplacé par la version de la coquille.
    this.ids.reload = await this.send('evora:open', `{ docRef: ${JSON.stringify(DOC_REF)}, name: 'evora-smoke.png', bytes: __ehp.png() }`);
    const reload = await this.waitEngine((m) => m.type === 'evora:ack' && m.re === this.ids.reload && !this.injectedIds.includes(m.id), 30000);
    await this.sabotage('reload-same-docref', 'refus fabriqué du rechargement', `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(this.ids.reload)}, payload: { ok: false, code: 'busy' } }`);
    if (reload && reload.msg.payload.ok === true) {
      if (!(await this.until(() => this.dirtyAfter(this.ids.reload, false), 10000))) this.fail('reload-same-docref', 'evora:dirty {dirty: false} absent après le rechargement');
      const r = await this.stats();
      const mine = r ? r.open.filter((d) => d.docRef === DOC_REF) : [];
      if (mine.length !== 1 || mine[0].dirty !== false) this.fail('reload-same-docref', `après le rechargement, documents de la coquille (diag.stats) : ${JSON.stringify(r && r.open)}`);
      const id = await this.send('evora:request-save', `{ reason: 'user', docRef: ${JSON.stringify(DOC_REF)} }`);
      const ack = await this.waitEngine((m) => m.re === id && !this.injectedIds.includes(m.id), 15000);
      if (!ack || ack.msg.type !== 'evora:ack' || ack.msg.payload.code !== 'clean') {
        this.fail('reload-same-docref', `demande après le rechargement : attendu evora:ack « clean », reçu ${JSON.stringify(ack && ack.msg)}`);
      }
    }
  }

  // LUM-03 : export.drive, Enregistrer sous et premier enregistrement d'un document du moteur, sans
  // aucun téléchargement. Les fenêtres du journal (verify) : aucun evora:export après un refus ;
  // plus aucun message pour l'ancien docRef après l'adoption.
  async exports() {
    const zip = (p) => p && p.bytes && p.bytes.$arrayBuffer > 0 && p.bytes.head === '504b0304';
    // export-png : Export As du document de la coquille.
    const png = await this.exportAs('export-png');
    if (png) {
      const p = png.msg.payload || {};
      if (p.docRef !== DOC_REF || p.name !== 'evora-smoke.png' || p.mime !== 'image/png' || !p.bytes || p.bytes.head !== '89504e47') {
        this.fail('export-png', `evora:export inattendu (docRef, nom, MIME, octets PNG) : ${JSON.stringify(p)}`);
      }
      await this.send('evora:ack', '{ ok: true }', { re: png.msg.id });
    }
    await this.sabotage('export-png', 'evora:export aux octets en vue typée',
      `{ v: 1, type: 'evora:export', payload: { docRef: ${JSON.stringify(DOC_REF)}, name: 'evora-smoke.png', mime: 'image/png', bytes: new Uint8Array(4) } }`);
    // export-refused : un second export, refusé par la coquille (quota).
    const second = await this.exportAs('export-refused');
    if (second) {
      const refusal = await this.send('evora:ack', `{ ok: false, code: 'quota' }`, { re: second.msg.id });
      await this.sabotage('export-refused', 'evora:export renvoyé après le refus',
        `{ v: 1, type: 'evora:export', payload: { docRef: ${JSON.stringify(DOC_REF)}, name: 'evora-smoke.png', mime: 'image/png', bytes: new ArrayBuffer(8) } }`);
      await sleep(3000);
      const r = await this.stats();
      this.windows.push({ kind: 'export', tag: 'export-refused', what: 'après le refus de l\'export', from: refusal, to: this.ids.lastCommand });
      if (!r) this.fail('export-refused', 'le moteur ne répond plus (diag.stats) après le refus de l\'export');
    }
    // save-as-adopt : Enregistrer sous (Ctrl+Maj+S), puis adoption du nouveau docRef.
    const savesBefore = new Set(this.engineMessages().filter((e) => e.msg.type === 'evora:save').map((e) => e.msg.id));
    const saveAs = await this.pressExpr(keyPress('S', 'KeyS', { ctrl: true, shift: true }), 'Ctrl+Maj+S',
      (m) => m.type === 'evora:save' && !savesBefore.has(m.id), 'evora:save (Enregistrer sous)');
    if (!saveAs) this.fail('save-as-adopt', 'aucun evora:save après Ctrl+Maj+S (Fichier › Enregistrer sous)');
    else {
      const p = saveAs.msg.payload || {};
      if (p.saveAs !== true || p.docRef !== DOC_REF || p.name !== 'evora-smoke.pcraft' || !zip(p)) {
        this.fail('save-as-adopt', `evora:save d'Enregistrer sous inattendu (saveAs, docRef d'origine, nom .pcraft, octets zip) : ${JSON.stringify(p)}`);
      }
      const adopted = await this.saved(saveAs.msg.id, { ok: true, version: 1, docRef: ADOPT_REF });
      this.windows.push({ kind: 'ref', tag: 'save-as-adopt', what: 'après l\'adoption du nouveau docRef', from: adopted, docRef: DOC_REF });
      await this.sabotage('save-as-adopt', 'evora:dirty de l\'ancien docRef après l\'adoption',
        `{ v: 1, type: 'evora:dirty', payload: { dirty: true, docRef: ${JSON.stringify(DOC_REF)} } }`);
      if (!(await this.until(() => this.dirtyAfter(adopted, false, ADOPT_REF), 10000))) this.fail('save-as-adopt', 'evora:dirty {dirty: false} du nouveau docRef absent après l\'adoption');
      for (const [ref, want] of [[ADOPT_REF, 'clean'], [DOC_REF, 'not_found']]) {
        const id = await this.send('evora:request-save', `{ reason: 'user', docRef: ${JSON.stringify(ref)} }`);
        const ack = await this.waitEngine((m) => m.re === id && !this.injectedIds.includes(m.id), 15000);
        if (!ack || ack.msg.type !== 'evora:ack' || ack.msg.payload.code !== want) {
          this.fail('save-as-adopt', `demande d'enregistrement de ${ref === ADOPT_REF ? 'l\'adopté' : 'l\'ancien'} docRef : attendu « ${want} », reçu ${JSON.stringify(ack && ack.msg)}`);
        }
      }
    }
    // local-doc-first-save : un document du moteur (file.new, Entrée), puis Ctrl+S.
    const local = await this.newLocalDocument();
    if (!local) {
      this.fail('local-doc-first-save', 'document du moteur non créé (file.new, Entrée)');
    } else {
      const before = new Set(this.engineMessages().filter((e) => e.msg.type === 'evora:save').map((e) => e.msg.id));
      const first = await this.press('s', (m) => m.type === 'evora:save' && !before.has(m.id), 'evora:save (Ctrl+S, document du moteur)');
      if (!first) this.fail('local-doc-first-save', 'aucun evora:save après Ctrl+S sur un document du moteur');
      else {
        const p = first.msg.payload || {};
        if (p.saveAs !== true || !ULID_RE.test(p.docRef || '') || !/\.pcraft$/.test(p.name || '') || !zip(p)) {
          this.fail('local-doc-first-save', `premier enregistrement d'un document du moteur inattendu (saveAs, .pcraft, octets zip) : ${JSON.stringify(p)}`);
        }
        const adopted = await this.saved(first.msg.id, { ok: true, version: 1, docRef: LOCAL_REF });
        await this.sabotage('local-doc-first-save', 'evora:save {saveAs: true} hors .pcraft',
          `{ v: 1, type: 'evora:save', payload: { docRef: ${JSON.stringify(ADOPT_REF)}, name: 'Untitled-1.psd', saveAs: true, bytes: new ArrayBuffer(8) } }`);
        if (!(await this.until(() => this.dirtyAfter(adopted, false, LOCAL_REF), 10000))) this.fail('local-doc-first-save', 'evora:dirty {dirty: false} du nouveau document absent');
        const r = await this.stats();
        if (!r || !r.open.some((d) => d.docRef === LOCAL_REF)) this.fail('local-doc-first-save', `document adopté absent de diag.stats : ${JSON.stringify(r && r.open)}`);
      }
    }
  }

  // LUM-12 : evora:open {new: true}. Le dialogue Nouveau document du moteur s'ouvre (diag.stats) ;
  // Entrée crée le document de la coquille (accusé, dirty false, diag.stats, demande « clean ») ;
  // Échap, sur un second, l'annule (accusé cancelled, aucun document). Les accusés sont jugés par
  // verify() : un seul, conforme, par demande.
  async openNew() {
    for (const [tag, ref, key] of [['open-new', NEW_REF, 'Enter'], ['open-new-cancel', CANCEL_REF, 'Escape']]) {
      const before = await this.stats();
      const id = await this.send('evora:open', `{ docRef: ${JSON.stringify(ref)}, name: 'Sans titre.pcraft', new: true }`);
      this.ids[tag] = id;
      if (!before || !(await this.dialogsReach(before.dialogs + 1, () => this.stats(), 8000))) {
        this.fail(tag, 'evora:open {new: true} sans dialogue Nouveau document dans le moteur');
        continue;
      }
      let answer = null;
      for (let attempt = 1; attempt <= 3 && !answer; attempt++) {
        await this.evaluate(this.frame(), keyPress(key, key), true);
        answer = await this.waitEngine((m) => m.type === 'evora:ack' && m.re === id && !this.injectedIds.includes(m.id), 4000);
      }
      const after = await this.stats();
      if (tag === 'open-new') {
        if (answer && answer.msg.payload.ok === true) {
          if (!(await this.until(() => this.dirtyAfter(id, false, ref), 10000))) this.fail(tag, 'evora:dirty {dirty: false} absent après la création du document neuf');
          if (!after || !after.open.some((d) => d.docRef === ref && d.dirty === false)) this.fail(tag, `document neuf absent de diag.stats : ${JSON.stringify(after && after.open)}`);
          const rq = await this.send('evora:request-save', `{ reason: 'user', docRef: ${JSON.stringify(ref)} }`);
          const ack = await this.waitEngine((m) => m.re === rq && !this.injectedIds.includes(m.id), 15000);
          if (!ack || ack.msg.type !== 'evora:ack' || ack.msg.payload.code !== 'clean') {
            this.fail(tag, `demande d'enregistrement du document neuf : attendu « clean », reçu ${JSON.stringify(ack && ack.msg)}`);
          }
        }
        await this.sabotage('open-new', 'annulation fabriquée du document neuf', `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(id)}, payload: { ok: false, code: 'cancelled' } }`);
      } else {
        if (!after || after.open.some((d) => d.docRef === ref) || after.documents !== before.documents || after.dialogs !== before.dialogs) {
          this.fail(tag, `après l'annulation, diag.stats : ${JSON.stringify(after)} (avant : ${before.documents} document(s), ${before.dialogs} dialogue(s))`);
        }
        await this.sabotage('open-new-cancel', 'accusé positif fabriqué du document annulé', `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(id)}, payload: { ok: true } }`);
      }
    }
  }

  // Export As du document actif (commande file.export.exportAs, puis Entrée) ; rend l'evora:export
  // reçu, ou null (le dialogue resté ouvert est refermé).
  async exportAs(tag) {
    const before = new Set(this.engineMessages().filter((e) => e.msg.type === 'evora:export').map((e) => e.msg.id));
    const ack = await this.command('file.export.exportAs');
    if (!ack || ack.ok !== true) {
      this.fail(tag, `Export As non ouvert : ${JSON.stringify(ack)}`);
      return null;
    }
    await sleep(800);
    const hit = await this.pressExpr(keyPress('Enter', 'Enter'), 'Entrée', (m) => m.type === 'evora:export' && !before.has(m.id), 'evora:export (Export As)');
    if (hit) return hit;
    this.fail(tag, 'aucun evora:export après Export As (Entrée)');
    const r = await this.stats();
    if (r && r.dialogs > 0) await this.evaluate(this.frame(), keyPress('Escape', 'Escape'), true);
    return null;
  }

  // Un document du moteur : file.new, puis Entrée dans son dialogue ; vrai s'il est créé.
  async newLocalDocument() {
    const before = await this.stats();
    const ack = await this.command('file.new');
    if (!before || !ack || ack.ok !== true) return false;
    await sleep(800);
    for (let attempt = 1; attempt <= 3; attempt++) {
      await this.evaluate(this.frame(), keyPress('Enter', 'Enter'), true);
      await sleep(800);
      const r = await this.stats();
      if (r && r.documents === before.documents + 1 && r.dialogs === before.dialogs) return true;
    }
    return false;
  }

  // evora:request-save du document ; rend l'id de l'evora:save qui y répond, ou null. Le document
  // doit être modifié : une autre réponse fait rougir `tag` (sinon : banc inopérant).
  async requestSave(what, tag) {
    const id = await this.send('evora:request-save', `{ reason: 'user', docRef: ${JSON.stringify(DOC_REF)} }`);
    this.ids.lastRequest = id;
    const r = await this.waitEngine((m) => m.re === id && !this.injectedIds.includes(m.id), 15000);
    if (r && r.msg.type === 'evora:save') return r.msg.id;
    this.fail(tag || 'bench', `${what} : evora:request-save d'un document modifié, attendu un evora:save, reçu ${JSON.stringify(r && r.msg)}`);
    return null;
  }

  // Accusé positif d'un enregistrement ; l'état enregistré qui suit est attendu, non jugé ici.
  async cleanAfter(saveId) {
    const sent = await this.saved(saveId, { ok: true, version: this.nextVersion() });
    if (!(await this.until(() => this.dirtyAfter(sent, false), 10000))) this.log('[ehp] état enregistré non revu après un accusé positif');
  }

  nextVersion() {
    this.version = (this.version || 10) + 1;
    return this.version;
  }

  // Résultat de diag.stats (LUM-10), ou null.
  async stats() {
    const ack = await this.command('diag.stats');
    const r = ack && ack.ok === true ? ack.result : null;
    return r && Number.isInteger(r.dialogs) && Array.isArray(r.open) ? r : null;
  }

  // Révision du document de la coquille (diag.stats), ou null.
  async revision() {
    const r = await this.stats();
    const d = r && r.open.find((x) => x.docRef === DOC_REF);
    return d ? d.revision : null;
  }

  // Scénario pont-panique (build de fumée) : sur demande de la coquille admise, le moteur panique ; le
  // crochet de LUM-04 doit envoyer un seul evora:failed {code: "panic"}. `onTrigger` est appelé juste
  // avant la demande (les erreurs de page qui suivent sont celles de la panique).
  async panic(onTrigger) {
    this.panicMode = true;
    await this.refresh();
    const early = this.engineMessages().filter((e) => e.msg.type === 'evora:failed');
    if (early.length) this.fail('panique', `evora:failed avant la demande de panique : ${early.map((e) => e.msg.id).join(', ')}`);
    onTrigger();
    this.ids.panic = await this.send(PANIC_TRIGGER, '{}');
    const failed = await this.waitEngine((m) => m.type === 'evora:failed', 15000);
    if (!failed) this.fail('panique', 'aucun evora:failed après la demande de panique');
    await sleep(2000);
  }

  // LUM-10 : evora:command. Chaque commande de la liste blanche est accusée positivement (diag.* avec
  // leur résultat) et produit son effet : un dialogue du moteur s'ouvre (compté par diag.stats), puis
  // se referme sur Échap. Une commande hors liste est refusée sans effet. Les accusés sont jugés dans
  // verify() : un seul par commande, conforme.
  async commands() {
    const stats = async () => {
      const ack = await this.command('diag.stats');
      const r = ack && ack.ok === true ? ack.result : null;
      return r && Number.isInteger(r.dialogs) ? r : null;
    };
    const first = await stats();
    if (!first) {
      this.fail('command-allowed', 'diag.stats sans résultat exploitable : effets des commandes non vérifiables');
      return;
    }
    await this.command('diag.menus');
    this.ids.menus = this.ids.lastCommand;
    for (const command of DIALOG_COMMANDS) {
      const ack = await this.command(command);
      // Accusé absent ou négatif : verify() le dit.
      if (!ack || ack.ok !== true) continue;
      if (!(await this.dialogsReach(first.dialogs + 1, stats, 8000))) {
        this.fail('command-allowed', `${command} accusé sans qu'un dialogue s'ouvre dans le moteur`);
        continue;
      }
      let closed = false;
      for (let attempt = 1; attempt <= 3 && !closed; attempt++) {
        await this.evaluate(this.frame(), keyPress('Escape', 'Escape'), true);
        closed = await this.dialogsReach(first.dialogs, stats, 3000);
      }
      if (!closed) this.fail('command-allowed', `le dialogue ouvert par ${command} ne s'est pas refermé sur Échap`);
    }
    for (const command of REFUSED_COMMANDS) await this.command(command, true);
    const after = await stats();
    if (!after || after.dialogs !== first.dialogs) this.fail('command-refused', `dialogues après les commandes hors liste : ${JSON.stringify(after)}`);
    const stat = this.ids.commands.find((c) => c.command === 'diag.stats');
    const refused = this.ids.commands.find((c) => c.refused);
    await this.sabotage('command-allowed', 'refus fabriqué de diag.stats', `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(stat.id)}, payload: { ok: false, code: 'capability' } }`);
    await this.sabotage('command-refused', 'accusé positif fabriqué d\'une commande hors liste', `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(refused.id)}, payload: { ok: true } }`);
  }

  // Un evora:command, journalisé pour verify() ; rend la charge utile de la réponse du moteur (jamais
  // celle d'un message fabriqué par le témoin « banc »).
  async command(command, refused = false) {
    const id = await this.send('evora:command', `{ command: ${JSON.stringify(command)}, params: {} }`);
    this.ids.commands.push({ id, command, refused });
    this.ids.lastCommand = id;
    const ack = await this.waitEngine((m) => m.type === 'evora:ack' && m.re === id && !this.injectedIds.includes(m.id), 15000);
    return ack ? ack.msg.payload : null;
  }

  // Le moteur compte-t-il n dialogues ouverts (diag.stats), au plus ms millisecondes ?
  async dialogsReach(n, stats, ms) {
    const end = Date.now() + ms;
    for (;;) {
      const r = await stats();
      if (r && r.dialogs === n) return true;
      if (Date.now() > end) return false;
      await sleep(300);
    }
  }

  // Réponse conforme à un evora:command : refus `capability` hors liste ; accusé positif sinon, avec
  // le résultat attendu pour diag.stats et diag.menus, sans résultat pour une commande du moteur.
  commandAnswered(c, p) {
    if (c.refused) return p.ok === false && p.code === 'capability' && p.result === undefined;
    if (p.ok !== true || p.code !== undefined) return false;
    const r = p.result;
    if (c.command === 'diag.stats') {
      return !!r && r.version === this.readyVersion && typeof r.renderer === 'string' && r.renderer.length > 0 && r.wasmMemoryBytes > 0
        && Number.isInteger(r.documents) && r.documents >= 1 && Number.isInteger(r.dialogs) && r.dialogs >= 0 && Array.isArray(r.open)
        && r.open.length >= 1 && r.open.every((d) => ULID_RE.test(d.docRef) && typeof d.dirty === 'boolean' && Number.isInteger(d.revision))
        && Array.isArray(r.fonts) && r.fonts.every((f) => typeof f === 'string' && f.length > 0);
    }
    if (c.command === 'diag.menus') {
      return !!r && Array.isArray(r.live) && Array.isArray(r.enabled) && DIALOG_COMMANDS.every((x) => r.live.includes(x))
        && r.enabled.every((x) => r.live.includes(x));
    }
    return r === undefined;
  }

  async storage() {
    return JSON.parse(await this.evaluate(this.frame(), STORAGE_ALL));
  }

  // Noms de ce moteur (nus ou sous un espace de noms) parmi des clés ou des bases.
  static engineNames(list, name) {
    return (list || []).filter((k) => k === name || (k.endsWith(`:${name}`) && k.length > name.length + 1));
  }

  // E5 corrigé : espace de noms, purge à l'init, témoins de purge, purge réelle, plus d'écriture.
  // Chaque contrôle est nommé (checkpoint) : un témoin sabordé y vérifie que chaque défaut rougit à
  // sa place et seulement là. Les sabotages du témoin « banc » sont posés juste avant leur contrôle
  // et retirés juste après (restore), pour ne déborder sur aucun autre.
  async storagePhases(sessionState) {
    const mine = { keys: [`${STORAGE_NS}:${PREFS}`], dbs: [`${STORAGE_NS}:${PRESETS}`] };
    // keys : préférences ; markers : marqueurs de base (clé localStorage nommée comme la base) ;
    // dbs : bases IndexedDB.
    const names = (st) => ({ keys: EhpBench.engineNames(st.keys, PREFS), markers: EhpBench.engineNames(st.keys, PRESETS),
      dbs: EhpBench.engineNames(st.dbs, PRESETS) });
    const foreignMarkers = (n) => n.markers.filter((k) => k !== `${STORAGE_NS}:${PRESETS}`);
    const same = (a, b) => JSON.stringify([...a].sort()) === JSON.stringify([...b].sort());
    const nsKey = `${STORAGE_NS}:${PREFS}`;
    const foreignKept = (st, phase) => {
      if (!st.keys.includes(FOREIGN_KEY) || (st.dbs && !st.dbs.includes(FOREIGN_DB))) {
        this.fail('foreign-names', `${phase} : les noms d'un autre moteur de la même origine ont été effacés : ${JSON.stringify({ keys: st.keys, dbs: st.dbs })}`);
      }
    };
    const untouched = async (st, phase, label) => {
      await this.checkpoint(`${label}:opfs`, () => {
        if (Array.isArray(st.opfs) && st.opfs.length) this.fail('opfs', `${phase} : OPFS non vide : ${st.opfs.join(', ')}`);
      });
      await this.checkpoint(`${label}:caches`, () => {
        if (Array.isArray(st.caches) && st.caches.length) this.fail('caches', `${phase} : Cache Storage non vide : ${st.caches.join(', ')}`);
      });
      await this.checkpoint(`${label}:session`, () => {
        if (Array.isArray(st.session) && st.session.length) this.fail('session-storage', `${phase} : sessionStorage non vide : ${st.session.join(', ')}`);
      });
    };
    // 1. Après l'init et la séance : tout sous l'espace de noms ; autres comptes et noms nus purgés ;
    //    noms d'un autre moteur de la même origine conservés.
    await this.sabotage('namespace', 'nom nu de l\'amont réécrit après l\'init', `localStorage.setItem('${PREFS}', '{}'); true`);
    await this.sabotage('foreign-names', 'nom d\'un autre moteur de la même origine effacé', `localStorage.removeItem('${FOREIGN_KEY}'); true`);
    const session = SABOTAGE === 'banc' ? await this.storage() : sessionState;
    await this.checkpoint('init:espace', () => {
      const n1 = names(session);
      if (!same(n1.keys, mine.keys) || foreignMarkers(n1).length || (session.dbs && !same(n1.dbs, mine.dbs))) {
        this.fail('namespace', `stockage du moteur hors de l'espace de noms ou restes non purgés à l'init : ${JSON.stringify(n1)}, attendu ${JSON.stringify(mine)}`);
      }
    });
    await this.checkpoint('init:banc', () => {
      if (session.dbs === null) this.fail('bench', 'indexedDB.databases() indisponible : bases non vérifiables');
    });
    await this.checkpoint('init:autre-moteur', () => foreignKept(session, 'après l\'init'));
    await untouched(session, 'après l\'init', 'init');
    await this.restore(`localStorage.removeItem('${PREFS}'); localStorage.setItem('${FOREIGN_KEY}', 'garde'); true`);
    // 2. Témoins de purge : version 2, champ d'enveloppe en trop, origine non admise, autre fenêtre.
    this.ids.purgeV2 = await this.send('evora:purge', `{ scope: 'all' }`, { v: 2 });
    this.ids.purgeExtraField = await this.send('evora:purge', `{ scope: 'all' }`, { extraField: true });
    this.ids.purgeForeign = await this.witness('foreign', 'evora:purge', { scope: 'all' });
    this.ids.purgeSibling = await this.witness('sibling', 'evora:purge', { scope: 'all' });
    await sleep(2500);
    await this.sabotage('purge-witness', 'clé du compte effacée comme par un témoin de purge accepté',
      `window.__evoraSaved = localStorage.getItem('${nsKey}'); localStorage.removeItem('${nsKey}'); true`);
    const witness = await this.storage();
    this.storageLog.push({ phase: 'témoins de purge', ...witness });
    await this.checkpoint('témoins:effacement', () => {
      const n2 = names(witness);
      if (!same(n2.keys, mine.keys) || (witness.dbs && !same(n2.dbs, mine.dbs))) {
        this.fail('purge-witness', `un message de purge invalide a effacé le stockage : ${JSON.stringify(n2)}`);
      }
    });
    await this.restore(`if (typeof window.__evoraSaved === 'string') localStorage.setItem('${nsKey}', window.__evoraSaved); true`);
    // 3. Restes hors espace de noms reposés juste avant la purge, et une connexion qui bloque la
    //    suppression de la base du compte (comme un autre onglet).
    const blocker = await this.evaluate(this.frame(), `(async () => {
      localStorage.setItem('${PREFS}', '{}');
      await new Promise((r) => { const q = indexedDB.open('${PRESETS}', 1); q.onupgradeneeded = () => q.result.createObjectStore('files'); q.onsuccess = () => { q.result.close(); r(); }; q.onerror = () => r(); });
      window.__evoraBlocker = await new Promise((r) => { const q = indexedDB.open('${STORAGE_NS}:${PRESETS}'); q.onsuccess = () => r(q.result); q.onerror = () => r(null); });
      return window.__evoraBlocker !== null;
    })()`);
    await this.checkpoint('blocage:banc', () => {
      if (!blocker) this.fail('bench', 'connexion de blocage non ouverte : ordre accusé/suppression non vérifiable');
    });
    // 4. La vraie purge : clés effacées d'abord, pas d'accusé tant que la base n'est pas supprimée.
    this.ids.purge = await this.send('evora:purge', `{ scope: 'current' }`);
    await sleep(500);
    await this.sabotage('purge-early-ack', 'accusé de purge avant la suppression de la base',
      `{ v: 1, type: 'evora:ack', re: ${JSON.stringify(this.ids.purge)}, payload: { ok: true } }`);
    await this.sabotage('purge-order', 'clé du compte réécrite pendant la suppression bloquée', `localStorage.setItem('${nsKey}', '{}'); true`);
    await sleep(2000);
    await this.refresh();
    const early = this.engineMessages().find((e) => e.msg.re === this.ids.purge);
    const blocked = await this.storage();
    this.storageLog.push({ phase: 'purge bloquée', ...blocked });
    await this.checkpoint('purge-bloquée:accusé', () => {
      if (early) this.fail('purge-early-ack', `evora:purge accusé alors que la suppression de la base est encore bloquée : ${JSON.stringify(early.msg)}`);
    });
    await this.checkpoint('purge-bloquée:clés', () => {
      if (names(blocked).keys.length) this.fail('purge-order', `clés du moteur encore présentes pendant la suppression de la base : ${names(blocked).keys.join(', ')}`);
    });
    await this.restore(`localStorage.removeItem('${nsKey}'); true`);
    await this.evaluate(this.frame(), 'window.__evoraBlocker && window.__evoraBlocker.close(); true');
    // L'accusé du moteur, jamais celui que le témoin « banc » a fabriqué.
    const ack = await this.waitEngine((m) => m.type === 'evora:ack' && m.re === this.ids.purge && !this.injectedIds.includes(m.id), 20000);
    await this.sabotage('purge-left', 'clé du compte réécrite juste après l\'accusé de purge', `localStorage.setItem('${nsKey}', '{}'); true`);
    const after = await this.storage();
    this.storageLog.push({ phase: 'après l\'accusé de purge', ...after });
    await this.checkpoint('après-accusé:accusé', () => {
      if (!ack || ack.msg.payload.ok !== true) this.fail('purge', `accusé de evora:purge absent ou négatif : ${JSON.stringify(ack && ack.msg)}`);
    });
    await this.checkpoint('après-accusé:restes', () => {
      const n4 = names(after);
      if (n4.keys.length || n4.markers.length || (after.dbs && n4.dbs.length)) this.fail('purge-left', `stockage du moteur encore présent juste après l'accusé de purge : ${JSON.stringify(n4)}`);
    });
    await this.checkpoint('après-accusé:autre-moteur', () => foreignKept(after, 'après la purge'));
    await untouched(after, 'après la purge', 'après-accusé');
    await this.restore(`localStorage.removeItem('${nsKey}'); true`);
    // 5. Plus aucune écriture : un changement de thème après la purge n'écrit rien.
    this.ids.themeAfterPurge = await this.send('evora:theme', `{ theme: 'light', accent: '#4F46E5' }`);
    const themeAck = await this.waitEngine(this.response('evora:ack', this.ids.themeAfterPurge), 15000);
    await this.checkpoint('après-écriture:thème', () => {
      if (!themeAck || themeAck.msg.payload.ok !== true) this.fail('theme', `accusé de evora:theme après la purge absent ou négatif : ${JSON.stringify(themeAck && themeAck.msg)}`);
    });
    // Derniers contrôles : les sabotages qui les visent ne sont pas retirés (plus rien ne suit).
    await this.sabotage('purge-rewrite', 'clé du compte réécrite après la purge', `localStorage.setItem('${nsKey}', '{}'); true`);
    await this.sabotage('session-storage', 'clé posée dans sessionStorage', `sessionStorage.setItem('evora-sabotage', '1'); true`);
    await this.sabotage('caches', 'cache posé dans Cache Storage', `caches.open('evora-sabotage').then(() => true)`);
    await this.sabotage('opfs', 'fichier posé dans OPFS',
      `navigator.storage.getDirectory().then((d) => d.getFileHandle('evora-sabotage', { create: true })).then(() => true)`);
    await sleep(2500);
    const later = await this.storage();
    this.storageLog.push({ phase: 'après une écriture tentée', ...later });
    await this.checkpoint('après-écriture:réécriture', () => {
      const n5 = names(later);
      if (n5.keys.length || n5.markers.length || (later.dbs && n5.dbs.length)) this.fail('purge-rewrite', `le moteur a de nouveau écrit après la purge : ${JSON.stringify(n5)}`);
    });
    await untouched(later, 'après une écriture tentée', 'après-écriture');
  }

  // Position d'un message du moteur ou d'un envoi du banc dans le journal (-1 : absent).
  logIndex(id) {
    return this.state.log.findIndex((e) => (e.msg && e.msg.id === id) || (e.bench && e.bench.id === id));
  }

  // Un evora:dirty {dirty} du document, venu du moteur après l'envoi `marker` du banc (jamais un
  // message fabriqué par le témoin « banc »).
  dirtyAfter(marker, dirty, docRef = DOC_REF) {
    const at = this.logIndex(marker);
    return at >= 0 && this.state.log.slice(at + 1).some((e) => e.fromEngine && e.plain && e.msg.type === 'evora:dirty'
      && e.msg.payload && e.msg.payload.dirty === dirty && e.msg.payload.docRef === docRef && !this.injectedIds.includes(e.msg.id));
  }

  // evora:saved en réponse à l'evora:save `re` ; rend l'id de l'envoi.
  async saved(re, payload) {
    return this.send('evora:saved', JSON.stringify(payload), { re });
  }

  // Conformité de chaque enveloppe reçue du moteur, et des réponses.
  verify() {
    const msgs = this.engineMessages();
    const ids = new Set();
    for (const e of msgs) {
      const m = e.msg;
      // L'id en tête : un témoin sabordé crédite un rouge au message qu'il a fabriqué.
      const where = `${(m && m.type) || '?'} id ${(m && m.id) || '?'} ${JSON.stringify(m).slice(0, 200)}`;
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
          // LUM-03 : un nouveau document de la coquille est toujours un .pcraft.
          if (p && p.saveAs === true && !/\.pcraft$/.test(p.name || '')) this.fail('local-doc-first-save', `evora:save {saveAs: true} hors .pcraft : ${where}`);
          break;
        }
        case 'evora:export': {
          const b = (p && p.bytes) || {};
          if (m.re !== undefined || !only(['docRef', 'name', 'mime', 'bytes']) || (p.docRef !== undefined && !ULID_RE.test(p.docRef)) || typeof p.name !== 'string'
            || !p.name.length || /[\\/\u0000-\u001f\u007f]/.test(p.name) || typeof p.mime !== 'string' || p.mime.length > 120 || !MIME_RE.test(p.mime)
            || !(b.$arrayBuffer >= 1) || b.$shared !== undefined || b.$view !== undefined) {
            this.fail('export-png', `evora:export non conforme : ${where}`);
          }
          break;
        }
        case 'evora:pick':
          // LUM-02 : schéma du §4.4 (accept de 1 élément au moins, distincts, au motif du §4.4).
          if (m.re !== undefined || !only(['purpose', 'accept', 'multiple', 'forPath']) || !['open', 'import', 'relink'].includes(p.purpose)
            || typeof p.multiple !== 'boolean' || !Array.isArray(p.accept) || !p.accept.length || new Set(p.accept).size !== p.accept.length
            || !p.accept.every((a) => typeof a === 'string' && ACCEPT_RE.test(a)) || (p.forPath !== undefined && p.purpose !== 'relink')) {
            this.fail('pick', `evora:pick non conforme : ${where}`);
          }
          break;
        case 'evora:focus-out':
          // LUM-13 : sans réponse, charge utile vide (A-19).
          if (m.re !== undefined || !only([]) || Object.keys(p || {}).length) this.fail('focus-out', `evora:focus-out non conforme : ${where}`);
          break;
        case 'evora:failed':
          if (this.panicMode) {
            if (m.re !== undefined || !only(['code']) || p.code !== 'panic') this.fail('panique', `evora:failed non conforme à la panique : ${where}`);
            break;
          }
          this.fail('failed', `le moteur a signalé un échec : ${where}`);
          if (!only(['code']) || !FAIL_CODES.includes(p.code)) this.fail('envelope', `evora:failed non conforme : ${where}`);
          break;
        default:
          this.fail('envelope', `type inattendu pour ce moteur : ${where}`);
      }
    }
    const readies = msgs.filter((e) => e.msg && e.msg.type === 'evora:ready');
    if (readies.length !== 1) this.fail('ready-count', `evora:ready reçu ${readies.length} fois (attendu : 1), ids ${readies.map((e) => e.msg.id).join(', ') || 'aucun'}`);
    const answered = (id) => msgs.filter((e) => e.msg && e.msg.re === id);
    for (const [key, tag, label] of [['v2', 'answer-v2', 'un evora:init de version 2'],
      ['extraField', 'answer-envelope', 'un evora:init à l\'enveloppe augmentée d\'un champ'],
      ['foreign', 'answer-foreign', 'un message d\'une origine non admise'],
      ['sibling', 'answer-source', 'un message de l\'origine admise venu d\'une autre fenêtre'],
      ['purgeV2', 'answer-v2', 'un evora:purge de version 2'],
      ['purgeExtraField', 'answer-envelope', 'un evora:purge à l\'enveloppe augmentée d\'un champ'],
      ['purgeForeign', 'answer-foreign', 'un evora:purge d\'une origine non admise'],
      ['purgeSibling', 'answer-source', 'un evora:purge de l\'origine admise venu d\'une autre fenêtre']]) {
      if (this.ids[key] && answered(this.ids[key]).length) this.fail(tag, `le moteur a répondu à ${label} : ${JSON.stringify(answered(this.ids[key])[0].msg)}`);
    }
    // LUM-04 (e) : une panique, un seul evora:failed.
    if (this.panicMode) {
      const failures = msgs.filter((e) => e.msg && e.msg.type === 'evora:failed');
      if (failures.length !== 1) this.fail('panique', `evora:failed reçu ${failures.length} fois (attendu : 1), ids ${failures.map((e) => e.msg.id).join(', ') || 'aucun'}`);
    }
    // LUM-04 (b, l.480) : aucun evora:dirty {dirty: false} du document dans les fenêtres où il doit
    // rester modifié (enregistrement en vol, après un refus, après un accusé positif dépassé).
    for (const w of this.windows) {
      const from = this.logIndex(w.from);
      if (from < 0) continue;
      const to = w.to ? this.logIndex(w.to) : -1;
      // Fenêtres de LUM-03 : aucun evora:export après un refus ('export') ; plus aucun message pour
      // l'ancien docRef après une adoption ('ref', jusqu'à la fin du journal).
      const bad = {
        export: (e) => e.msg.type === 'evora:export',
        ref: (e) => !!e.msg.payload && e.msg.payload.docRef === w.docRef,
      }[w.kind] || ((e) => e.msg.type === 'evora:dirty' && e.msg.payload && e.msg.payload.dirty === false && e.msg.payload.docRef === w.docRef);
      const wrong = this.state.log.slice(from + 1, to < 0 ? undefined : to).filter((e) => e.fromEngine && e.plain && e.msg && bad(e));
      if (wrong.length) this.fail(w.tag, `${w.what} : ${w.kind === 'export' ? 'evora:export' : w.kind === 'ref' ? `message pour ${w.docRef}` : 'evora:dirty {dirty: false}'} reçu (${wrong.map((e) => e.msg.id).join(', ')})`);
    }
    // LUM-12 : un seul accusé par evora:open {new: true}, positif après Entrée, cancelled après Échap.
    for (const [tag, want] of [['open-new', (p) => p.ok === true && p.code === undefined], ['open-new-cancel', (p) => p.ok === false && p.code === 'cancelled']]) {
      if (!this.ids[tag]) continue;
      const rs = answered(this.ids[tag]);
      const bad = rs.filter((e) => e.msg.type !== 'evora:ack' || !want(e.msg.payload || {}));
      if (rs.length !== 1 || bad.length) {
        this.fail(tag, `evora:open {new: true} (id ${this.ids[tag]}) : ${rs.length} réponse(s) [${rs.map((e) => e.msg.id).join(', ')}], `
          + `non conformes : ${bad.map((e) => `${e.msg.id} ${JSON.stringify(e.msg.payload)}`).join(' | ') || 'aucune'}`);
      }
    }
    // LUM-04 (d, S-ACK-6) : un seul accusé, positif, du rechargement du même docRef.
    if (this.ids.reload) {
      const rs = answered(this.ids.reload);
      const bad = rs.filter((e) => e.msg.type !== 'evora:ack' || !e.msg.payload || e.msg.payload.ok !== true);
      if (rs.length !== 1 || bad.length) {
        this.fail('reload-same-docref', `evora:open du même docRef (id ${this.ids.reload}) : ${rs.length} réponse(s) [${rs.map((e) => e.msg.id).join(', ')}], `
          + `non conformes : ${bad.map((e) => `${e.msg.id} ${JSON.stringify(e.msg.payload)}`).join(' | ') || 'aucune'}`);
      }
    }
    // LUM-10 : un seul accusé par evora:command, conforme (commandAnswered) ; chaque réponse citée.
    for (const c of this.ids.commands || []) {
      const rs = answered(c.id);
      const bad = rs.filter((e) => !this.commandAnswered(c, e.msg.payload || {}));
      if (rs.length !== 1 || bad.length) {
        this.fail(c.refused ? 'command-refused' : 'command-allowed', `evora:command ${c.command} (id ${c.id}) : ${rs.length} réponse(s) `
          + `[${rs.map((e) => e.msg.id).join(', ')}], non conformes : ${bad.map((e) => `${e.msg.id} ${JSON.stringify(e.msg.payload).slice(0, 160)}`).join(' | ') || 'aucune'}`);
      }
    }
    // LUM-09 : chaque réponse à diag.menus masque l'échantillon attendu, et rien de masqué n'est vivant.
    if (this.ids.menus) {
      for (const e of answered(this.ids.menus)) {
        const r = (e.msg.payload || {}).result || {};
        const masked = Array.isArray(r.masked) ? r.masked : null;
        const live = Array.isArray(r.live) ? r.live : [];
        const missing = masked ? MASKED_SAMPLE.filter((x) => !masked.includes(x)) : MASKED_SAMPLE;
        const shown = MASKED_SAMPLE.filter((x) => live.includes(x)).concat((masked || []).filter((x) => live.includes(x)));
        if (!masked || missing.length || shown.length) {
          this.fail('menus-masked', `diag.menus id ${e.msg.id} : masqués absents ${missing.join(', ') || 'aucun'}, vivants à tort ${[...new Set(shown)].join(', ') || 'aucun'}`);
        }
      }
    }
    // LUM-11 : un seul accusé par evora:font, positif pour la police, invalid_content pour le reste.
    for (const [key, want, what] of [['font', (q) => q.ok === true && q.code === undefined, 'la police servie'],
      ['fontBad', (q) => q.ok === false && q.code === 'invalid_content', 'des octets qui ne sont pas une police']]) {
      if (!this.ids[key]) continue;
      const rs = answered(this.ids[key]);
      const bad = rs.filter((e) => e.msg.type !== 'evora:ack' || !want(e.msg.payload || {}));
      if (rs.length !== 1 || bad.length) {
        this.fail('font-inject', `evora:font de ${what} (id ${this.ids[key]}) : ${rs.length} réponse(s) [${rs.map((e) => e.msg.id).join(', ')}], `
          + `non conformes : ${bad.map((e) => `${e.msg.id} ${JSON.stringify(e.msg.payload)}`).join(' | ') || 'aucune'}`);
      }
    }
    // LUM-13 : un seul evora:focus-out du moteur (F6) ; ceux du témoin « banc » sont jugés plus haut.
    if (Object.prototype.hasOwnProperty.call(this.ids, 'focusOut')) {
      const outs = msgs.filter((e) => e.msg && e.msg.type === 'evora:focus-out' && !this.injectedIds.includes(e.msg.id));
      if (outs.length !== 1) this.fail('focus-out', `evora:focus-out reçu ${outs.length} fois (attendu : 1, après F6), ids ${outs.map((e) => e.msg.id).join(', ') || 'aucun'}`);
    }
    // LUM-14 : chaque evora:export d'une photo géolocalisée sort sans sa localisation. Témoins intacts
    // (les métadonnées ont été lues et réécrites, pas perdues) : la marque de l'appareil et les droits
    // de la première photo, la marque dans le profil brut, les droits de la photo aux noms de lieu.
    // Un EXIF à deux pointeurs GPS peut être laissé de côté en entier.
    const geoVerdict = {
      [GPS_REF]: (g) => [(!g.exif || !g.make) && 'EXIF ou marque de l\'appareil absents : la recette ne prouve rien', g.gpsTag && 'pointeur GPS dans IFD0',
        g.latitude && 'valeurs de latitude présentes', (!g.xmp || g.compressed || !g.rights) && `XMP absent, compressé ou sans les droits (xmp ${g.xmp}, compressé ${g.compressed}, droits ${g.rights})`,
        g.location && 'propriété GPS ou ville dans le XMP', g.textLocation && 'GPS ou lieu dans une entrée de texte'],
      [RAW_REF]: (g) => [(!g.raw || !g.rawMake) && 'profil brut EXIF absent ou sans la marque : la recette ne prouve rien', g.rawGps && 'pointeur GPS dans le profil brut EXIF',
        g.rawLatitude && 'valeurs de latitude dans le profil brut EXIF', g.textCompressed && 'texte compressé, illisible pour le banc', g.textLocation && 'GPS ou lieu dans une entrée de texte'],
      [DUAL_REF]: (g) => [g.exif && g.gpsTag && 'pointeur GPS restant dans IFD0 (deux pointeurs)', g.exif && g.latitude && 'valeurs de latitude restantes (deux répertoires)'],
      [PLACE_REF]: (g) => [(!g.xmp || g.compressed || !g.rights) && `XMP absent, compressé ou sans les droits (xmp ${g.xmp}, compressé ${g.compressed}, droits ${g.rights})`,
        g.location && 'nom de lieu (ps2:City, Sublocation) dans le XMP'],
      [CASE_REF]: (g) => [(!g.xmp || g.compressed || !g.rights) && `XMP absent, compressé ou sans les droits (xmp ${g.xmp}, compressé ${g.compressed}, droits ${g.rights})`,
        g.location && 'GPS ou lieu restant malgré la casse (exif:gpsLatitude, ps2:CITY, xmpDM:shotLocation)'],
      [SUB_REF]: (g) => [g.exif && g.gpsTag && 'pointeur GPS dans IFD0', g.exif && g.latitude && 'latitude restante (pointeur GPS du sous-répertoire Exif)'],
      [TEXT_REF]: (g) => [!g.textRights && 'droits absents du texte : la recette ne prouve rien', g.textCompressed && 'texte compressé, illisible pour le banc',
        g.textLocation && 'GPS ou lieu restant dans le texte (XMP sous autre préfixe, clé Place)'],
    };
    for (const e of msgs.filter((x) => x.msg && x.msg.type === 'evora:export' && geoVerdict[(x.msg.payload || {}).docRef])) {
      const g = e.png;
      const why = !g || typeof g !== 'object' ? [`PNG illisible (${JSON.stringify(g)})`] : geoVerdict[e.msg.payload.docRef](g).filter(Boolean);
      const f = GEO_FIXTURES.find((x) => x.ref === e.msg.payload.docRef);
      if (why.length) this.fail('export-no-gps', `evora:export id ${e.msg.id} de ${f.name} : ${why.join(' ; ')}`);
    }
    if (this.ids.gpsExports) {
      for (const f of GEO_FIXTURES) if (!this.ids.gpsExports[f.ref]) this.fail('export-no-gps', `export de ${f.name} non joué`);
    } else if (this.ids.menus) this.fail('export-no-gps', 'exports des photos géolocalisées non joués');
    // LUM-06 r2 : chaque réponse aux relevés du thème est jugée ; une réponse fabriquée par le témoin
    // « banc » rougit à son détecteur, citée par son id.
    for (const c of this.ids.themeChecks || []) {
      for (const e of answered(c.id)) {
        const r = ((e.msg.payload || {}).result) || {};
        const r2 = themeProblems(r.theme);
        if (r2.length) this.fail('theme-r2', `diag.stats id ${e.msg.id} (${c.step}) : ${r2.join(' ; ')}`);
        const ac = accentProblems(r.theme, c);
        if (ac.length) this.fail('theme-accent', `diag.stats id ${e.msg.id} (${c.step}) : ${ac.join(' ; ')}`);
      }
    }
    // LUM-14 r2 et LUM-15 : les préférences de evora:init, relevées après l'ouverture.
    if (this.ids.prefsStats) {
      for (const e of answered(this.ids.prefsStats)) {
        const why = prefsProblems((((e.msg.payload || {}).result) || {}).prefs);
        if (why.length) this.fail('prefs-init', `diag.stats id ${e.msg.id} (init) : ${why.join(' ; ')}`);
      }
    }
    if (this.ids.menus) {
      if (this.ids.themeChecks.length !== 2 + THEME_STEPS.length) this.fail('theme-r2', `relevés du thème joués : ${this.ids.themeChecks.length}, attendus ${2 + THEME_STEPS.length}`);
      if (!this.ids.prefsStats) this.fail('prefs-init', 'relevé des préférences de evora:init non joué');
    }
    if (this.politiqueMode) this.politiqueVerdict(msgs);
    const announced = this.readyCaps;
    const extra = announced.filter((x) => !EXERCISED.includes(x));
    if (extra.length) this.fail('caps', `capacités annoncées non exercées par ce scénario : ${extra.join(', ')}`);
    const missing = EXERCISED.filter((x) => !announced.includes(x));
    if (missing.length) this.fail('caps', `capacités attendues absentes de evora:ready : ${missing.join(', ')}`);
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
