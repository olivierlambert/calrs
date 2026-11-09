#!/usr/bin/env python3
"""Rebuild templates/partials/phosphor.html from assets/phosphor/regular/*.svg.

Run from the repository root after adding or updating an icon:
    python3 assets/phosphor/gen_partial.py
Each macro branch is the inner markup of one SVG, unchanged.
"""
import os
import re

SRC = "assets/phosphor/regular"
OUT = "templates/partials/phosphor.html"


def inner(path):
    text = open(path).read()
    body = re.search(r"<svg[^>]*>(.*)</svg>", text, re.S).group(1)
    return re.sub(r"\s*\n\s*", "", body).strip()


names = sorted(f[:-4] for f in os.listdir(SRC) if f.endswith(".svg"))
lines = [
    "{#- Phosphor Icons 2.1.1, regular weight (MIT, see assets/phosphor/).",
    "    Each branch is the inner markup of assets/phosphor/regular/<name>.svg,",
    "    unchanged; a test checks they stay in step. Import with",
    '    {% import "partials/phosphor.html" as ph %} and call {{ ph.icon("house") }}.',
    "    They take the text colour, so they follow every theme. -#}",
    "{% macro icon(name, size=16) -%}",
    '<svg class="nav-ic" width="{{ size }}" height="{{ size }}" viewBox="0 0 256 256" '
    'fill="currentColor" aria-hidden="true" focusable="false">',
]
for i, name in enumerate(names):
    keyword = "if" if i == 0 else "elif"
    lines.append("{%- " + keyword + ' name == "' + name + '" -%}' + inner(f"{SRC}/{name}.svg"))
lines += ["{%- endif -%}", "</svg>", "{%- endmacro %}"]
open(OUT, "w").write("\n".join(lines) + "\n")
print(f"{OUT}: {len(names)} icons")
