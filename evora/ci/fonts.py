"""Polices servies à part (Evora, patch E6) pour le build web de Lumbra.

Usage : python3 -I evora/ci/fonts.py <copie de craft-fonts> <dossier de sortie>

Lit fonts/manifest.txt de craft-fonts (format « family | style | file | scripts | licence |
licence file | sha256 | source »), vérifie que chaque chemin reste dans le dépôt, que l'empreinte
SHA-256 de chaque police correspond au manifeste (la révision de craft-fonts est épinglée par la
CI), puis copie le manifeste, les polices et leurs licences dans <sortie>/fonts/, aux mêmes chemins
relatifs à la racine du site (packaging/web/README.md › Fonts). Les polices restent hors du wasm :
le moteur les télécharge à la demande (fetch avec intégrité SRI). Sort en erreur au moindre écart.
"""
from __future__ import annotations

import hashlib
import pathlib
import shutil
import sys


def safe(rel: str) -> bool:
    return bool(rel) and not rel.startswith("/") and ".." not in rel.split("/") and "\\" not in rel and "://" not in rel and rel.startswith("fonts/")


def main() -> int:
    src, out = pathlib.Path(sys.argv[1]).resolve(), pathlib.Path(sys.argv[2]).resolve()
    manifest = src / "fonts/manifest.txt"
    lines = manifest.read_text(encoding="utf-8").split("\n")
    copied: list[str] = []
    errors: list[str] = []
    families: set[str] = set()
    for n, line in enumerate(lines, 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        cols = [c.strip() for c in line.split(" | ")]
        if len(cols) < 3:
            errors.append(f"ligne {n} : moins de trois champs")
            continue
        family, _style, font = cols[0], cols[1], cols[2]
        licence = cols[5] if len(cols) > 5 else ""
        digest = cols[6] if len(cols) > 6 else ""
        for rel in [font] + ([licence] if licence else []):
            if not safe(rel):
                errors.append(f"ligne {n} : chemin refusé {rel!r}")
        if not safe(font):
            continue
        data = (src / font).read_bytes()
        actual = hashlib.sha256(data).hexdigest()
        if not digest:
            errors.append(f"ligne {n} : {font} sans empreinte sha256 dans le manifeste")
        elif actual != digest:
            errors.append(f"ligne {n} : {font} sha256 {actual} au lieu de {digest}")
        families.add(family)
        for rel in [font] + ([licence] if licence and safe(licence) else []):
            dest = out / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            if not dest.exists():
                shutil.copyfile(src / rel, dest)
                copied.append(rel)
    (out / "fonts").mkdir(parents=True, exist_ok=True)
    shutil.copyfile(manifest, out / "fonts/manifest.txt")
    total = sum((out / rel).stat().st_size for rel in copied)
    print(f"{len(families)} familles, {len(copied)} fichiers copiés (polices et licences), {total} octets")
    for rel in sorted(copied):
        print(f"  {rel} {(out / rel).stat().st_size}")
    for e in errors:
        print(f"::error::fonts/manifest.txt : {e}")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
