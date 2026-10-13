"""Empreintes CSP du document moteur (Evora, build web de Lumbra).

Usage : python3 -I evora/ci/csp_hashes.py <index.html> <sortie csp-hashes.txt>

Calcule le sha256 (base64) de chaque <script> en ligne et de chaque <style> en ligne de index.html,
octet pour octet comme le navigateur (le contenu brut entre les balises, en UTF-8), et écrit la
politique cible d'ARCHITECTURE §3.6 avec ces empreintes. L'origine Evora autorisée à encadrer le
moteur reste un gabarit ({{EVORA_ORIGIN}}), remplacé au provisionnement et par le test de fumée.
Échoue si un gestionnaire d'événement en ligne (onload=…) ou une URL javascript: est présent :
la politique n'admet ni 'unsafe-inline' ni 'unsafe-hashes' pour les scripts.
"""
import base64
import hashlib
import pathlib
import re
import sys


def digest(text: str) -> str:
    return "'sha256-" + base64.b64encode(hashlib.sha256(text.encode("utf-8")).digest()).decode("ascii") + "'"


def main() -> int:
    src, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    html = src.read_bytes().decode("utf-8")
    scripts = re.findall(r"<script\b([^>]*)>(.*?)</script\s*>", html, flags=re.S | re.I)
    inline = [body for attrs, body in scripts if not re.search(r"\bsrc\s*=", attrs, flags=re.I)]
    styles = re.findall(r"<style\b[^>]*>(.*?)</style\s*>", html, flags=re.S | re.I)
    handlers = re.findall(r"<[^>]+\son[a-z]+\s*=", html, flags=re.I)
    js_urls = re.findall(r"""(?:href|src)\s*=\s*["']\s*javascript:""", html, flags=re.I)
    script_hashes = [digest(b) for b in inline]
    style_hashes = [digest(b) for b in styles]
    policy = "; ".join([
        "default-src 'self'",
        "script-src 'self' 'wasm-unsafe-eval'" + "".join(" " + h for h in script_hashes),
        "worker-src 'self' blob:",
        "connect-src 'self'",
        "img-src 'self' data: blob:",
        "style-src 'self' 'unsafe-inline'",
        "font-src 'self' data:",
        "frame-ancestors {{EVORA_ORIGIN}}",
        "object-src 'none'",
        "base-uri 'none'",
    ])
    lines = [
        "# Lumbra (Evora) : empreintes CSP de index.html (sha256, base64)",
        f"# {len(inline)} script(s) en ligne, {len(styles)} style(s) en ligne, {len(scripts) - len(inline)} script(s) externe(s)",
        "script-src-hashes " + " ".join(script_hashes),
        "style-src-hashes " + " ".join(style_hashes),
        f"inline-event-handlers {len(handlers)}",
        f"javascript-urls {len(js_urls)}",
        "policy " + policy,
        "",
    ]
    out.write_text("\n".join(lines), encoding="utf-8")
    print("\n".join(lines))
    if handlers or js_urls:
        print(f"::error::index.html contient {len(handlers)} gestionnaire(s) d'événement en ligne et {len(js_urls)} URL javascript:")
        return 1
    if not inline:
        print("::warning::aucun script en ligne dans index.html : rien à empreinter")
    return 0


if __name__ == "__main__":
    sys.exit(main())
