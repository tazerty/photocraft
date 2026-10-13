"""Portes Evora du build web de Lumbra (fork tazerty/photocraft).

Usage : python3 -I evora/ci/gates.py <dossier du site empaqueté> <rapport.json>

Lancé par .github/workflows/evora-web.yml après l'empaquetage, depuis la racine du dépôt. Écrit un
résumé Markdown (GITHUB_STEP_SUMMARY) et un rapport JSON ; sort en erreur si une porte bloquante
échoue. Aucune dépendance hors de la bibliothèque standard ; aucun code de l'amont n'est exécuté
(les catalogues et les polices sont lus comme des données).

Portes :
  1. marques (E1, bloquante) : ArtCraft, getartcraft, discord.gg absents du site servi (tout le
     dossier sauf LICENSES/), des catalogues décompressés du build et des catalogues source ;
     ni manifeste web ni service worker ;
  2. hébergement (bloquante) : aucun .htaccess, _headers, *.conf, *.php, fichier caché ni lien
     symbolique dans l'artefact ;
  3. licences (bloquante) : LICENSES/ complet (fichiers de l'amont, mentions, cargo-about) ;
  4. émojis (E10) : chaque symbole des catalogues affichés est résolu dans la pile de polices
     d'egui (comme au rendu) ; un point de code émoji (table de Noto Emoji, Emoji_Presentation)
     n'est admis que dessiné par une police de texte ; bloquante pour les catalogues ; pour les
     littéraux du code d'ui-egui, bloquante si EVORA_GATE_CODE_EMOJI=block, sinon mesure ;
  5. termes sensibles (E9) : Lumetri, Lightroom, Acrobat, Premiere, Adobe, Cycore dans les
     catalogues affichés après le catalogue de surcharge (i18n/overlay.tsv s'il existe) ; bloquante
     si EVORA_GATE_TERMS=block, sinon mesure ;
  6. précompression et CSP (bloquante) : .br et .gz pour chaque .wasm, .js, .html ; csp-hashes.txt ;
  7. taille (mesure) : .wasm brut, .gz, .br.
"""
from __future__ import annotations

import glob
import json
import os
import pathlib
import re
import struct
import sys
import unicodedata
import zlib

ROOT = pathlib.Path.cwd()
I18N = ROOT / "crates/ui-egui/src/i18n"
BRAND_TERMS = ["ArtCraft", "getartcraft", "discord.gg"]
BRAND_INFO = ["artcraft", "discord", "PhotoCraft"]  # mesures (insensibles à la casse sauf PhotoCraft)
SENSITIVE = ["Lumetri", "Lightroom", "Acrobat", "Premiere Pro Compatible", "Premiere", "Adobe", "Cycore"]
HOSTING = re.compile(r"(^|/)(\.htaccess|_headers|[^/]*\.conf|[^/]*\.php[0-9]?|[^/]*\.phtml)$", re.I)
# Emoji_Presentation=Yes dans le plan multilingue de base (emoji-data.txt d'Unicode) ; au-delà de
# U+1F000, tout pictogramme est traité comme émoji.
EMOJI_BMP = set([0x231A, 0x231B, 0x23E9, 0x23EA, 0x23EB, 0x23EC, 0x23F0, 0x23F3, 0x25FD, 0x25FE, 0x2614, 0x2615,
                 *range(0x2648, 0x2654), 0x267F, 0x2693, 0x26A1, 0x26AA, 0x26AB, 0x26BD, 0x26BE, 0x26C4, 0x26C5, 0x26CE,
                 0x26D4, 0x26EA, 0x26F2, 0x26F3, 0x26F5, 0x26FA, 0x26FD, 0x2705, 0x270A, 0x270B, 0x2728, 0x274C, 0x274E,
                 0x2753, 0x2754, 0x2755, 0x2757, 0x2795, 0x2796, 0x2797, 0x27B0, 0x27BF, 0x2B1B, 0x2B1C, 0x2B50, 0x2B55])
EMOJI_JOINERS = {0xFE0F, 0x200D, 0x20E3} | set(range(0x1F3FB, 0x1F400)) | set(range(0x1F1E6, 0x1F200))
EMOJI_FONTS = {"NotoEmoji-Regular", "emoji-icon-font"}
TEXT_FONTS = {"Inter", "Ubuntu-Light", "Hack", "JetBrainsMono"}
# Points de code émoji : la table cmap de Noto Emoji (police d'émojis d'egui, lue comme donnée même
# quand E10 l'a retirée de la pile), plus la liste Emoji_Presentation ci-dessus.
EMOJI_CODEPOINTS: set[int] = set()
AUDITED = "✓✔✖⚠★"
CJK_RANGES = [(0x2E80, 0x2FFF), (0x3000, 0x303F), (0x3040, 0x30FF), (0x3100, 0x33FF), (0x3400, 0x9FFF),
              (0xAC00, 0xD7AF), (0xF900, 0xFAFF), (0xFE30, 0xFE4F), (0xFF00, 0xFFEF), (0x20000, 0x3FFFF)]

failures: list[str] = []
summary: list[str] = []
report: dict = {}


def fail(msg: str) -> None:
    failures.append(msg)
    print(f"::error::{msg}")


def cmap(path: pathlib.Path) -> set[int]:
    """Points de code couverts par une police TrueType/OpenType (tables cmap 4 et 12)."""
    d = path.read_bytes()
    off = struct.unpack(">I", d[12:16])[0] if d[:4] == b"ttcf" else 0
    num = struct.unpack(">H", d[off + 4:off + 6])[0]
    tables = {}
    for i in range(num):
        tag, _, o, ln = struct.unpack(">4sIII", d[off + 12 + 16 * i:off + 28 + 16 * i])
        tables[tag] = (o, ln)
    o, _ = tables[b"cmap"]
    n = struct.unpack(">H", d[o + 2:o + 4])[0]
    cps: set[int] = set()
    for i in range(n):
        _, _, so = struct.unpack(">HHI", d[o + 4 + 8 * i:o + 12 + 8 * i])
        s = o + so
        fmt = struct.unpack(">H", d[s:s + 2])[0]
        if fmt == 4:
            segx2 = struct.unpack(">H", d[s + 6:s + 8])[0]
            seg = segx2 // 2
            ends = struct.unpack(f">{seg}H", d[s + 14:s + 14 + segx2])
            starts = struct.unpack(f">{seg}H", d[s + 16 + segx2:s + 16 + 2 * segx2])
            deltas = struct.unpack(f">{seg}h", d[s + 16 + 2 * segx2:s + 16 + 3 * segx2])
            ro_pos = s + 16 + 3 * segx2
            ros = struct.unpack(f">{seg}H", d[ro_pos:ro_pos + segx2])
            for k in range(seg):
                for c in range(starts[k], ends[k] + 1):
                    if c == 0xFFFF:
                        continue
                    if ros[k] == 0:
                        g = (c + deltas[k]) & 0xFFFF
                    else:
                        gp = ro_pos + 2 * k + ros[k] + 2 * (c - starts[k])
                        g = struct.unpack(">H", d[gp:gp + 2])[0]
                        g = (g + deltas[k]) & 0xFFFF if g else 0
                    if g:
                        cps.add(c)
        elif fmt == 12:
            groups = struct.unpack(">I", d[s + 12:s + 16])[0]
            for k in range(groups):
                a, b, g = struct.unpack(">III", d[s + 16 + 12 * k:s + 28 + 12 * k])
                cps.update(c for c in range(a, b + 1) if g + (c - a))
    return cps


def catalog_entries() -> list[tuple[str, str, str]]:
    """(langue, source anglaise, traduction) de chaque entrée des catalogues compilés."""
    out = []
    for f in sorted(I18N.glob("*.tsv")):
        for line in f.read_text(encoding="utf-8").split("\n"):
            if line.startswith("#") or not line.strip():
                continue
            cols = line.split("\t")
            if len(cols) >= 3 and cols[1] and cols[2]:
                out.append((f.stem, cols[1], "\t".join(cols[2:])))
    return out


def served_files(pkg: pathlib.Path) -> list[pathlib.Path]:
    return [p for p in sorted(pkg.rglob("*")) if p.is_file() and "LICENSES" not in p.relative_to(pkg).parts
            and p.name != "EVORA-BUILD.txt" and p.suffix not in (".br", ".gz")]


def count(term: str, data: bytes, ci: bool = False) -> int:
    if ci:
        return data.lower().count(term.lower().encode("utf-8"))
    return data.count(term.encode("utf-8"))


def inflated_catalogs() -> list[bytes]:
    """Catalogues tels que build.rs les a empaquetés (OUT_DIR du build courant), décompressés."""
    found = sorted(glob.glob("target/**/photocraft-ui-egui-*/out/i18n/keys.deflate", recursive=True),
                   key=lambda p: os.path.getmtime(p), reverse=True)
    if not found:
        return []
    out = []
    for f in sorted(pathlib.Path(found[0]).parent.glob("*.deflate")):
        raw = f.read_bytes()
        for wbits in (-15, 15, 31):
            try:
                out.append(zlib.decompress(raw, wbits))
                break
            except zlib.error:
                continue
    report["inflatedCatalogDir"] = str(pathlib.Path(found[0]).parent)
    return out


def gate_brand(pkg: pathlib.Path, entries) -> None:
    files = served_files(pkg)
    served = b"".join(p.read_bytes() for p in files)
    inflated = inflated_catalogs()
    infl = b"\n".join(inflated)
    sources = b"".join(f.read_bytes() for f in sorted(I18N.glob("*.tsv")))
    rows = {}
    for term in BRAND_TERMS:
        rows[term] = [count(term, served), count(term, infl), count(term, sources)]
        if any(rows[term]):
            fail(f"marque « {term} » présente : site servi {rows[term][0]}, catalogues du build {rows[term][1]}, catalogues source {rows[term][2]}")
    for term in BRAND_INFO:
        ci = term != "PhotoCraft"
        rows[term + (" (casse ignorée)" if ci else "")] = [count(term, served, ci), count(term, infl, ci), count(term, sources, ci)]
    if not inflated:
        fail("catalogues décompressés du build introuvables (OUT_DIR de photocraft-ui-egui)")
    # Ni manifeste web ni service worker (E1 : noms de cache du service worker).
    names = [p.name.lower() for p in files]
    sw = [n for n in names if n.endswith(".webmanifest") or n in ("manifest.json", "sw.js", "service-worker.js")]
    sw_api = sum(count("serviceWorker", p.read_bytes()) for p in files if p.suffix in (".js", ".html"))
    if sw or sw_api:
        fail(f"manifeste ou service worker présent : {sw}, {sw_api} appel(s) serviceWorker")
    report["brand"] = {"files": [str(p.relative_to(pkg)) for p in files], "counts": rows, "inflatedCatalogs": len(inflated),
                       "manifestOrServiceWorker": sw, "serviceWorkerCalls": sw_api}
    summary.append("### Porte des marques (E1, bloquante)\n")
    summary.append(f"Site servi : {len(files)} fichiers (hors LICENSES/) ; catalogues décompressés du build : {len(inflated)} ; catalogues source : {len(list(I18N.glob('*.tsv')))}.\n")
    summary.append("| Terme | Site servi | Catalogues du build | Catalogues source |\n| --- | ---: | ---: | ---: |")
    for k, v in rows.items():
        summary.append(f"| `{k}` | {v[0]} | {v[1]} | {v[2]} |")
    summary.append(f"\nManifeste web ou service worker : {'aucun' if not (sw or sw_api) else sw}.\n")


def gate_hosting(pkg: pathlib.Path) -> None:
    bad = []
    for p in sorted(pkg.rglob("*")):
        rel = str(p.relative_to(pkg))
        if p.is_symlink() or HOSTING.search(rel) or any(part.startswith(".") for part in p.relative_to(pkg).parts):
            bad.append(rel)
    if bad:
        fail(f"fichiers d'hébergement ou cachés dans l'artefact : {bad}")
    report["hosting"] = bad
    summary.append("### Fichiers d'hébergement (bloquante)\n")
    summary.append("Aucun (`.htaccess`, `_headers`, `*.conf`, `*.php`, fichiers cachés, liens symboliques).\n" if not bad else f"Présents : {bad}\n")


def gate_licenses(pkg: pathlib.Path) -> None:
    need = ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "ATTRIBUTION.md", "MENTIONS.txt", "THIRD-PARTY-RUST.html",
            "OFL-Inter.txt", "OFL-JetBrainsMono.txt", "LICENSE-translations.txt"]
    have = sorted(p.name for p in (pkg / "LICENSES").glob("*")) if (pkg / "LICENSES").is_dir() else []
    missing = [n for n in need if n not in have]
    if missing:
        fail(f"LICENSES/ incomplet : manquent {missing}")
    mention = (pkg / "LICENSES/MENTIONS.txt").read_text(encoding="utf-8") if (pkg / "LICENSES/MENTIONS.txt").exists() else ""
    if "basé sur PhotoCraft par l'équipe ArtCraft" not in mention:
        fail("LICENSES/MENTIONS.txt ne porte pas la mention « basé sur PhotoCraft par l'équipe ArtCraft »")
    third = pkg / "LICENSES/THIRD-PARTY-RUST.html"
    crates = len(re.findall(r"<li>[^<]+ \d+\.\d+[^<]*</li>", third.read_text(encoding="utf-8"))) if third.exists() else 0
    report["licenses"] = {"files": have, "missing": missing, "thirdPartyCrateLines": crates}
    summary.append("### LICENSES/ (bloquante)\n")
    summary.append(f"{len(have)} fichiers ; manquants : {missing or 'aucun'} ; THIRD-PARTY-RUST.html (cargo-about) : {crates} lignes de crates.\n")


def font_stack() -> list[tuple[str, set[int]]]:
    """Pile proportionnelle d'egui telle qu'install_fonts la construit : Inter, puis la pile par
    défaut d'egui_fonts.rs (lue dans la source), puis rien (les polices de repli CJK et autres
    écritures sont chargées à la demande et ne couvrent pas les symboles)."""
    src = (ROOT / "crates/ui-egui/src/egui_fonts.rs").read_text(encoding="utf-8")
    m = re.search(r"let proportional = stack\(&\[([^\]]*)\]\)", src)
    if not m:
        fail("pile proportionnelle introuvable dans egui_fonts.rs")
        return []
    names = ["Inter"] + re.findall(r'"([^"]+)"', m.group(1))
    lock = (ROOT / "Cargo.lock").read_text(encoding="utf-8")
    v = re.search(r'name = "epaint_default_fonts"\nversion = "([^"]+)"', lock)
    reg = sorted(glob.glob(os.path.expanduser(f"~/.cargo/registry/src/*/epaint_default_fonts-{v.group(1) if v else '*'}/fonts")))
    files = {"Inter": ROOT / "assets/fonts/Inter-Regular.ttf"}
    if reg:
        for n in ("Hack-Regular", "NotoEmoji-Regular", "Ubuntu-Light", "emoji-icon-font"):
            files[n] = pathlib.Path(reg[-1]) / f"{n}.ttf"
    stack = []
    for n in names:
        f = files.get(n)
        if not f or not f.exists():
            fail(f"police {n} de la pile introuvable ({f})")
            continue
        stack.append((n, cmap(f)))
    noto = files.get("NotoEmoji-Regular")
    if noto and noto.exists():
        EMOJI_CODEPOINTS.update(cmap(noto))
    else:
        fail("Noto Emoji introuvable : impossible de classer les points de code émoji")
    report["fontStack"] = names
    report["emojiCodepoints"] = len(EMOJI_CODEPOINTS)
    return stack


def resolve(c: int, stack) -> str | None:
    for name, cps in stack:
        if c in cps:
            return name
    return None


def emoji_verdict(ch: str, stack) -> tuple[str | None, str | None]:
    """(police qui dessine le caractère, motif de refus ou None)."""
    c = ord(ch)
    font = resolve(c, stack)
    if c in EMOJI_JOINERS:
        return font, "séquence d'émoji (VS16, ZWJ, modificateur)"
    if c in EMOJI_BMP or c >= 0x1F000 and not (0x1F100 <= c <= 0x1F1E5):
        return font, "présentation émoji par défaut"
    if c in EMOJI_CODEPOINTS and font not in TEXT_FONTS:
        # Un point de code émoji n'est admis que dessiné par une police de texte (présentation texte).
        return font, f"point de code émoji dessiné par {font or 'aucune police (tofu)'}"
    if font is None and unicodedata.category(ch).startswith("S") and not any(a <= c <= b for a, b in CJK_RANGES):
        return font, "aucune police de la pile (tofu)"
    return font, None


def symbols(text: str):
    for ch in text:
        c = ord(ch)
        if c < 0x80:
            continue
        cat = unicodedata.category(ch)
        if cat[0] in "LMNZ" and c not in EMOJI_JOINERS:
            continue
        yield ch


def gate_emoji(entries) -> None:
    stack = font_stack()
    seen: dict[str, dict] = {}
    for lang, src, tr in entries:
        for col, text in (("en", src), (lang, tr)):
            for ch in symbols(text):
                e = seen.setdefault(ch, {"count": 0, "langs": set(), "example": text[:80]})
                e["count"] += 1
                e["langs"].add(col)
    rows, bad = [], []
    for ch, e in sorted(seen.items(), key=lambda kv: -kv[1]["count"]):
        font, why = emoji_verdict(ch, stack)
        if unicodedata.category(ch)[0] == "P" and why is None and ch not in AUDITED and font in TEXT_FONTS | {None}:
            continue  # ponctuation dessinée par une police de texte (ou de repli CJK) : rien à signaler
        rows.append((ch, e, font, why))
        if why:
            bad.append(f"U+{ord(ch):04X} {unicodedata.name(ch, '?')} ({why}), ex. « {e['example']} »")
    for b in bad:
        fail(f"émoji dans les catalogues affichés : {b}")
    # Littéraux du code d'ui-egui et de photocraft-web (hors tests).
    code_bad = code_literal_audit(stack)
    strict = os.environ.get("EVORA_GATE_CODE_EMOJI", "report") == "block"
    for b in code_bad:
        if strict:
            fail(f"émoji dans un littéral du code : {b}")
        else:
            print(f"::warning::émoji dans un littéral du code (mesure, E10 à poser) : {b}")
    report["emoji"] = {"stack": [n for n, _ in stack], "catalogSymbols": [
        {"char": ch, "cp": f"U+{ord(ch):04X}", "count": e["count"], "langs": len(e["langs"]), "font": font, "refus": why}
        for ch, e, font, why in rows], "catalogFailures": bad, "codeLiteralFindings": code_bad, "codeStrict": strict}
    summary.append("### Émojis et symboles des catalogues affichés (E10, bloquante)\n")
    summary.append(f"Pile proportionnelle résolue comme au rendu : {' → '.join(n for n, _ in stack)}.\n")
    summary.append("| Car. | Point de code | Occurrences | Langues | Police au rendu | Verdict |\n| --- | --- | ---: | ---: | --- | --- |")
    for ch, e, font, why in rows:
        verdict = why or ("non émoji, police d'icônes d'egui avant les polices CJK" if font in EMOJI_FONTS else "texte")
        summary.append(f"| {ch} | U+{ord(ch):04X} | {e['count']} | {len(e['langs'])} | {font or '—'} | {verdict} |")
    summary.append(f"\nLittéraux du code (ui-egui, photocraft-web, hors tests) : {len(code_bad)} refus ({'bloquant' if strict else 'mesure'}).\n")
    for b in code_bad:
        summary.append(f"- {b}")
    summary.append("")


def rust_literals(src: str):
    """Littéraux chaîne d'un source Rust (commentaires ignorés), sans les modules de test."""
    cut = src.find("#[cfg(test)]\nmod tests")
    if cut >= 0:
        src = src[:cut]
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
            continue
        if src.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif src.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            i = j
            continue
        m = re.match(r'b?r(#*)"', src[i:]) if c in "br" else None
        if m and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            close = '"' + m.group(1)
            j = src.find(close, i + len(m.group(0)))
            j = n if j < 0 else j
            yield src[i:j + len(close)]
            i = j + len(close)
            continue
        if c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            yield src[i:j + 1]
            i = j + 1
            continue
        if c == "'":
            m = re.match(r"'(\\u\{[0-9a-fA-F]+\}|\\x[0-9a-fA-F]{2}|\\.|[^\\'])'", src[i:])
            if m:
                yield m.group(0)
                i += len(m.group(0))
                continue
        i += 1


def code_literal_audit(stack) -> list[str]:
    found = []
    files = sorted(glob.glob("crates/ui-egui/src/**/*.rs", recursive=True)) + sorted(glob.glob("apps/photocraft-web/src/*.rs"))
    for f in files:
        if f.endswith("_tests.rs") or f.endswith("/tests.rs") or "/tests/" in f:
            continue
        for lit in rust_literals(pathlib.Path(f).read_text(encoding="utf-8")):
            for ch in symbols(lit):
                font, why = emoji_verdict(ch, stack)
                if why:
                    found.append(f"{f} : U+{ord(ch):04X} {unicodedata.name(ch, '?')} ({why}) dans {lit[:60]!r}")
    return found


def overlay_rules() -> list[tuple[str, str]]:
    f = I18N / "overlay.tsv"
    if not f.exists():
        return []
    rules = []
    for line in f.read_text(encoding="utf-8").split("\n"):
        if line.startswith("#") or not line.strip():
            continue
        a, _, b = line.partition("\t")
        rules.append((a, b))
    return rules


def apply_overlay(text: str, rules) -> str:
    for a, b in rules:
        text = text.replace(a, b)
    return text


def gate_terms(entries) -> None:
    rules = overlay_rules()
    strict = os.environ.get("EVORA_GATE_TERMS", "report") == "block"
    before, after, left = {}, {}, []
    for lang, src, tr in entries:
        for col, text in (("en", src), (lang, tr)):
            shown = apply_overlay(text, rules)
            for t in SENSITIVE:
                before[t] = before.get(t, 0) + text.count(t)
                k = shown.count(t)
                after[t] = after.get(t, 0) + k
                if k:
                    left.append(f"{col} : « {shown[:90]} » ({t})")
    if strict and any(after.values()):
        for item in sorted(set(left))[:40]:
            fail(f"terme sensible affiché après surcharge : {item}")
    report["terms"] = {"rules": len(rules), "before": before, "after": after, "strict": strict, "left": sorted(set(left))}
    summary.append(f"### Termes sensibles des catalogues affichés (E9, {'bloquante' if strict else 'mesure'})\n")
    summary.append(f"Règles de surcharge : {len(rules)}.\n")
    summary.append("| Terme | Catalogues bruts | Après surcharge |\n| --- | ---: | ---: |")
    for t in SENSITIVE:
        summary.append(f"| `{t}` | {before.get(t, 0)} | {after.get(t, 0)} |")
    summary.append("")


def gate_precompression_and_csp(pkg: pathlib.Path) -> None:
    missing = []
    for p in sorted(pkg.rglob("*")):
        if p.is_file() and p.suffix in (".wasm", ".js", ".html") and "LICENSES" not in p.relative_to(pkg).parts:
            for ext in (".br", ".gz"):
                if not p.with_name(p.name + ext).exists():
                    missing.append(str(p.relative_to(pkg)) + ext)
    if missing:
        fail(f"fichiers précompressés manquants : {missing}")
    csp = pkg / "csp-hashes.txt"
    if not csp.exists():
        fail("csp-hashes.txt absent de l'artefact")
    report["precompressed"] = {"missing": missing}
    summary.append("### Précompression et CSP (bloquante)\n")
    summary.append(f"`.br` et `.gz` manquants : {missing or 'aucun'} ; `csp-hashes.txt` : {'présent' if csp.exists() else 'absent'}.\n")
    if csp.exists():
        summary.append("```\n" + csp.read_text(encoding="utf-8").strip() + "\n```\n")


def measure_sizes(pkg: pathlib.Path) -> None:
    rows = []
    for p in sorted(pkg.glob("*.wasm")):
        raw = p.stat().st_size
        gz = p.with_name(p.name + ".gz")
        br = p.with_name(p.name + ".br")
        rows.append({"file": p.name, "raw": raw, "gz": gz.stat().st_size if gz.exists() else None, "br": br.stat().st_size if br.exists() else None})
    total = sum(p.stat().st_size for p in pkg.rglob("*") if p.is_file() and p.suffix not in (".br", ".gz"))
    report["sizes"] = {"wasm": rows, "siteBytes": total}
    summary.append("### Taille (mesure, budget à arbitrer)\n")
    summary.append("| Fichier | Brut | gzip -9 | brotli -q 11 | MiB brut |\n| --- | ---: | ---: | ---: | ---: |")
    for r in rows:
        summary.append(f"| `{r['file']}` | {r['raw']} | {r['gz']} | {r['br']} | {r['raw'] / 1048576:.2f} |")
    summary.append(f"\nSite complet (hors .br/.gz) : {total} octets.\n")


def main() -> int:
    pkg = pathlib.Path(sys.argv[1]).resolve()
    out = pathlib.Path(sys.argv[2])
    entries = catalog_entries()
    report["catalogEntries"] = len(entries)
    gate_brand(pkg, entries)
    gate_hosting(pkg)
    gate_licenses(pkg)
    gate_emoji(entries)
    gate_terms(entries)
    gate_precompression_and_csp(pkg)
    measure_sizes(pkg)
    report["failures"] = failures
    out.write_text(json.dumps(report, ensure_ascii=False, indent=1, default=sorted), encoding="utf-8")
    head = ["## Portes Evora", "", f"Échecs bloquants : {len(failures)}.", ""]
    text = "\n".join(head + summary) + "\n"
    print(text)
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a", encoding="utf-8") as fh:
            fh.write(text)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
