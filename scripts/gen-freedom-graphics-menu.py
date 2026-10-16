#!/usr/bin/env python3
"""Generate the Freedom renderer's graphics settings menu from the renderer source.

The rows, ranges, choice names and help texts are the hosted renderer's own settings
catalog (lib/sjk/crates/sjk-viewer/src/settings/), so the menu offers exactly what the
renderer reads. Usage: gen-freedom-graphics-menu.py [renderer source dir] [output]
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "lib", "sjk")
OUT = sys.argv[2] if len(sys.argv) > 2 else os.path.join(ROOT, "assets", "japro", "ui", "jamp", "freedom_graphics.menu")
VIEWER = os.path.join(SRC, "crates", "sjk-viewer", "src")


def read(path):
    with open(os.path.join(VIEWER, path), encoding="utf-8") as f:
        return f.read()


def tokens(text):
    """Rust-ish tokens: string literals, paths, numbers and punctuation."""
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c.isspace():
            i += 1
        elif text.startswith("//", i):
            i = text.find("\n", i)
            i = n if i < 0 else i
        elif c == '"':
            j, out = i + 1, []
            while text[j] != '"':
                if text[j] == "\\":
                    nxt = text[j + 1]
                    if nxt == "\n":
                        j += 2
                        while text[j] in " \t\n":
                            j += 1
                        continue
                    out.append({"n": "\n", "t": "\t", '"': '"', "\\": "\\"}.get(nxt, nxt))
                    j += 2
                    continue
                out.append(text[j])
                j += 1
            yield ("str", "".join(out))
            i = j + 1
        elif c.isalnum() or c == "_":
            j = i
            while j < n:
                if text[j].isalnum() or text[j] in "_.":
                    j += 1
                elif text.startswith("::", j):
                    j += 2
                else:
                    break
            yield ("id", text[i:j])
            i = j
        elif c == "-" and i + 1 < n and text[i + 1].isdigit():
            j = i + 1
            while j < n and (text[j].isalnum() or text[j] in "_."):
                j += 1
            yield ("id", text[i:j])
            i = j
        else:
            yield ("p", c)
            i += 1


def constants():
    """`const NAME: &str = "value";` across the viewer, by NAME and file stem."""
    found = {}
    for folder, _, files in os.walk(VIEWER):
        for name in files:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(folder, name)
            stem = os.path.relpath(path, VIEWER)[:-3].replace(os.sep, "::")
            with open(path, encoding="utf-8") as f:
                for m in re.finditer(r'const ([A-Z_0-9]+): &str = "([^"]*)";', f.read()):
                    found.setdefault(m.group(1), []).append((stem, m.group(2)))
    return found


CONSTANTS = constants()


def resolve(expr):
    if not expr.startswith("crate::"):
        return expr
    parts = expr[len("crate::"):].split("::")
    name, modules = parts[-1], parts[:-1]
    candidates = CONSTANTS.get(name, [])
    # The path's modules name the file, or a #[path] module renamed from one.
    for stem, value in candidates:
        tail = stem.split("::")
        if tail[-len(modules):] == modules or tail[-1] == modules[-1] or tail[-1] == "_".join(modules[-2:]):
            return value
    aliases = {"frame_target::aa::exposure": "post_exposure", "weather": "weather_settings"}
    stem_wanted = aliases.get("::".join(modules))
    for stem, value in candidates:
        if stem == stem_wanted:
            return value
    return expr  # not a settings row this menu offers


def array(text, name):
    start = text.index(name)
    start = text.index("= &[", start) + 4
    depth, i = 1, start
    while depth:
        if text[i] == "[":
            depth += 1
        elif text[i] == "]":
            depth -= 1
        elif text.startswith("//", i):
            i = text.index("\n", i)
        elif text[i] == '"':
            i += 1
            while text[i] != '"':
                i += 2 if text[i] == "\\" else 1
        i += 1
    return text[start:i - 1]


def settings(body):
    rows, toks = [], list(tokens(body))
    i = 0
    while i < len(toks):
        if toks[i] == ("id", "Setting"):
            j, depth, fields = i + 2, 1, {}
            key = None
            kind = []
            while depth:
                t = toks[j]
                if t == ("p", "{"):
                    depth += 1
                elif t == ("p", "}"):
                    depth -= 1
                if depth == 1 and t[0] == "id" and j + 1 < len(toks) and toks[j + 1] == ("p", ":") and t[1] in ("label", "cvar", "kind"):
                    key = t[1]
                    j += 2
                    continue
                if key == "kind" and depth >= 1:
                    kind.append(t)
                elif key and t[0] in ("str", "id") and key not in fields:
                    fields[key] = t
                j += 1
            rows.append((fields["label"][1], resolve(fields["cvar"][1]), kind))
            i = j
        else:
            i += 1
    return rows


def kind_of(kind):
    words = [t[1] for t in kind if t[0] in ("id", "str")]
    head = words[0].split("::")[-1]
    if head == "Bool":
        return ("bool",)
    if head == "Choice":
        return ("choice", [w for t, w in kind if t == "str"])
    values = dict(zip(words[1::2], words[2::2]))
    if head == "Integer":
        return ("int", int(values["min"]), int(values["max"]), int(values["step"]))
    if head == "Float":
        return ("float", float(values["min"]), float(values["max"]), float(values["step"]))
    raise SystemExit(f"unknown kind {words}")


def pairs(body, width):
    out, current = [], []
    for t in tokens(body):
        if t == ("p", "("):
            current = []
        elif t == ("p", ")"):
            if len(current) == width:
                out.append(tuple(current))
        elif t[0] in ("str", "id"):
            current.append(resolve(t[1]) if t[0] == "id" else t[1])
    return out


catalog = read("settings/catalog.rs")
help_rs = read("settings/help.rs")
HELP = {name.lower(): text for name, text in pairs(array(help_rs, "const HELP:"), 2)}
CHOICES = {}
for cvar, value, shown in pairs(array(help_rs, "const CHOICE_NAMES:"), 3):
    CHOICES.setdefault(cvar.lower(), {})[value] = shown

TABS = [
    ("Image", "RENDER_IMAGE"),
    ("Lighting", "RENDER_LIGHTING"),
    ("Shadows", "RENDER_SHADOWS"),
    ("Weather", "RENDER_WEATHER"),
]
LEVELS = ["Vanilla", "Performance", "Balanced", "High", "Ultra"]
ROWS_PER_PAGE = 12


def clean(text):
    """No product names in what the player reads; the menu's quotes stay balanced."""
    text = text.replace("SJK's", "this renderer's").replace("SJK", "the renderer")
    return text.replace('"', "'")


def label(text):
    note = ""
    for mark, shown in ((" (reload)", " *"), (" (next map)", " +")):
        if text.endswith(mark):
            text, note = text[: -len(mark)], shown
    return clean(text) + note


def number(value):
    return ("%g" % value) if isinstance(value, float) else str(value)


pages = []
for tab, const in TABS:
    rows = settings(array(catalog, f"pub(super) const {const}:"))
    for start in range(0, len(rows), ROWS_PER_PAGE):
        part = rows[start:start + ROWS_PER_PAGE]
        name = tab if len(rows) <= ROWS_PER_PAGE else f"{tab} {start // ROWS_PER_PAGE + 1}"
        pages.append((name, part))

X, Y, W = 40, 92, 480
ROW_H = 18
out = []
w = out.append
w("// Generated by scripts/gen-freedom-graphics-menu.py from the Freedom renderer's")
w("// settings catalog; do not edit by hand. The renderer is built on SJK")
w("// (github.com/Sol-Vulpes/SJK, GPL-2.0); see lib/sjk/NOTICE.")
w("//")
w("// * applies after Apply (vid_restart); + applies on the next map.")
w("{")
w("\tmenuDef")
w("\t{")
w("\t\tname\t\t\t\t\tfreedomgraphics")
w("\t\tvisible\t\t\t\t\t0")
w("\t\tfullscreen\t\t\t\t0")
w("\t\toutOfBoundsClick")
w("\t\trect\t\t\t\t\t40 30 560 420")
w("\t\tfocusColor\t\t\t\t1 1 1 1")
w("\t\tdescX\t\t\t\t\t280")
w("\t\tdescY\t\t\t\t\t372")
w("\t\tdescScale\t\t\t\t0.8")
w("\t\tdescColor\t\t\t\t1 .682 0 1")
w("\t\tdescAlignment\t\t\tITEM_ALIGN_CENTER")
w("\t\tonOpen\t\t\t\t\t{ " + " ".join(f"hide fg_page{i} ;" for i in range(len(pages))) + " show fg_page0 ; }")
w("\t\tonESC\t\t\t\t\t{ play \"sound/interface/button1.wav\" ; close freedomgraphics }")


def item(lines):
    # "type" first: font, cvar and other keywords allocate the type's data, sized
    # for the type the item has when they are read.
    lines = sorted(lines, key=lambda line: 0 if line.startswith("type\t") else 1)
    w("\t\titemDef")
    w("\t\t{")
    for line in lines:
        w("\t\t\t" + line)
    w("\t\t}")


item([
    "name\t\t\t\tfg_background", "group\t\t\t\tnone", "style\t\t\t\tWINDOW_STYLE_FILLED",
    "rect\t\t\t\t0 0 560 420", "backcolor\t\t\t0 0 .2 .94", "forecolor\t\t\t1 1 1 1",
    "border\t\t\t\t1", "bordercolor\t\t\t0 0 .8 1", "visible\t\t\t\t1", "decoration",
])
item([
    "name\t\t\t\tfg_title", "group\t\t\t\tnone", "type\t\t\t\tITEM_TYPE_TEXT",
    "text\t\t\t\t\"Freedom Renderer Graphics\"", "rect\t\t\t\t0 6 560 20",
    "textalign\t\t\tITEM_ALIGN_CENTER", "textalignx\t\t\t280", "font\t\t\t\t3",
    "textscale\t\t\t0.9", "forecolor\t\t\t1 .682 0 1", "visible\t\t\t\t1", "decoration",
])

# Quality level buttons.
for i, level in enumerate(LEVELS):
    item([
        f"name\t\t\t\tfg_level_{level.lower()}", "group\t\t\t\tnone", "type\t\t\t\tITEM_TYPE_BUTTON",
        f"text\t\t\t\t\"{level}\"", f"rect\t\t\t\t{20 + i * 106} 32 100 16",
        "textalign\t\t\tITEM_ALIGN_CENTER", "textalignx\t\t\t50", "font\t\t\t\t4",
        "textscale\t\t\t0.9", "forecolor\t\t\t.615 .615 .956 1", "visible\t\t\t\t1",
        f"descText\t\t\t\"Quality: {level}. Sets the costly settings together; * settings apply after Apply.\"",
        f"action\t\t\t\t{{ play \"sound/interface/button1.wav\" ; exec \"graphicsquality {level.lower()}\" ; }}",
    ])

# Page tabs.
tab_w = 540 // len(pages)
for i, (name, _) in enumerate(pages):
    shows = " ".join(f"hide fg_page{j} ;" for j in range(len(pages)))
    item([
        f"name\t\t\t\tfg_tab{i}", "group\t\t\t\tnone", "type\t\t\t\tITEM_TYPE_BUTTON",
        f"text\t\t\t\t\"{name}\"", f"rect\t\t\t\t{10 + i * tab_w} 58 {tab_w} 16",
        "textalign\t\t\tITEM_ALIGN_CENTER", f"textalignx\t\t\t{tab_w // 2}", "font\t\t\t\t4",
        "textscale\t\t\t0.8", "forecolor\t\t\t1 .682 0 1", "visible\t\t\t\t1",
        f"descText\t\t\t\"{name} settings\"",
        f"action\t\t\t\t{{ play \"sound/interface/button1.wav\" ; {shows} show fg_page{i} ; }}",
    ])

for p, (page, rows) in enumerate(pages):
    for r, (text, cvar, kind) in enumerate(rows):
        kind = kind_of(kind)
        y = Y + r * ROW_H
        help_text = clean(HELP.get(cvar.lower(), text))
        base = [
            f"name\t\t\t\tfg_{cvar}", f"group\t\t\t\tfg_page{p}",
            f"text\t\t\t\t\"{label(text)}:\"", "textalign\t\t\tITEM_ALIGN_RIGHT",
            "textalignx\t\t\t250", "textaligny\t\t\t0", "font\t\t\t\t4", "textscale\t\t\t0.8",
            "forecolor\t\t\t.615 .615 .956 1", "visible\t\t\t\t0",
            f"descText\t\t\t\"{help_text}\"",
        ]
        restart = text.endswith("(reload)")
        action = "action\t\t\t\t{ play \"sound/interface/button1.wav\" ; " + ("show fg_apply ; " if restart else "") + "}"
        if kind[0] == "bool":
            item(base + ["type\t\t\t\tITEM_TYPE_MULTI", f"rect\t\t\t\t{X} {y} {W} 16", f"cvar\t\t\t\t\"{cvar}\"",
                         "cvarFloatList\t\t{ @MENUS_OFF 0 @MENUS_ON 1 }", action])
        elif kind[0] == "choice":
            names = CHOICES.get(cvar.lower(), {})
            values = " ".join(f"\"{clean(names.get(v, v))}\" {v}" for v in kind[1])
            item(base + ["type\t\t\t\tITEM_TYPE_MULTI", f"rect\t\t\t\t{X} {y} {W} 16", f"cvar\t\t\t\t\"{cvar}\"",
                         f"cvarFloatList\t\t{{ {values} }}", action])
        elif kind[0] == "int" and (kind[2] - kind[1]) // kind[3] <= 16:
            values = " ".join(f"\"{v}\" {v}" for v in range(kind[1], kind[2] + 1, kind[3]))
            item(base + ["type\t\t\t\tITEM_TYPE_MULTI", f"rect\t\t\t\t{X} {y} {W} 16", f"cvar\t\t\t\t\"{cvar}\"",
                         f"cvarFloatList\t\t{{ {values} }}", action])
        else:
            lo, hi = kind[1], kind[2]
            item(base + ["type\t\t\t\tITEM_TYPE_SLIDER", f"rect\t\t\t\t{X} {y} 360 16",
                         f"cvarfloat\t\t\t\"{cvar}\" {number(lo)} {number(lo)} {number(hi)}", action])
            item([
                f"name\t\t\t\tfg_{cvar}_field", f"group\t\t\t\tfg_page{p}", "type\t\t\t\tITEM_TYPE_NUMERICFIELD",
                "text\t\t\t\t\"\"", f"cvar\t\t\t\t\"{cvar}\"", f"rect\t\t\t\t{X + 370} {y} 70 16",
                "maxchars\t\t\t8", "font\t\t\t\t4", "textscale\t\t\t0.8", "forecolor\t\t\t.615 .615 .956 1",
                "visible\t\t\t\t0", f"descText\t\t\t\"{help_text}\"",
                "action\t\t\t\t{ play \"sound/interface/button1.wav\" ; " + ("show fg_apply ; " if restart else "") + "}",
            ])

item([
    "name\t\t\t\tfg_apply", "group\t\t\t\tnone", "type\t\t\t\tITEM_TYPE_BUTTON",
    "text\t\t\t\t\"Apply (restarts video)\"", "rect\t\t\t\t130 392 300 18",
    "textalign\t\t\tITEM_ALIGN_CENTER", "textalignx\t\t\t150", "font\t\t\t\t4", "textscale\t\t\t0.9",
    "forecolor\t\t\t1 .2 .2 1", "visible\t\t\t\t1",
    "descText\t\t\t\"Restart the video system so * settings apply.\"",
    "action\t\t\t\t{ play \"sound/interface/button1.wav\" ; close freedomgraphics ; exec \"vid_restart\" ; }",
])
item([
    "name\t\t\t\tfg_close", "group\t\t\t\tnone", "type\t\t\t\tITEM_TYPE_BUTTON",
    "text\t\t\t\t\"Close\"", "rect\t\t\t\t460 392 90 18",
    "textalign\t\t\tITEM_ALIGN_CENTER", "textalignx\t\t\t45", "font\t\t\t\t4", "textscale\t\t\t0.9",
    "forecolor\t\t\t1 .682 0 1", "visible\t\t\t\t1",
    "action\t\t\t\t{ play \"sound/interface/button1.wav\" ; close freedomgraphics ; }",
])
w("\t}")
w("}")
with open(OUT, "w", encoding="utf-8", newline="\r\n") as f:
    f.write("\n".join(out) + "\n")
print(f"{OUT}: {len(pages)} pages, {sum(len(r) for _, r in pages)} settings")
