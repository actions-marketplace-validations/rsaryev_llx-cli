#!/usr/bin/env python3
"""Copy llx's part of models.dev into packages/providers/models/data (see MODELS.md).

    git clone -b dev https://github.com/anomalyco/models.dev /tmp/models.dev
    python3 scripts/models_sync.py /tmp/models.dev

It needs Python 3.11 or later (tomllib), or `pip install tomli` before that. The files are
models.dev's own, copied verbatim with their source comments: for each provider llx lists first
(POPULAR, read from packages/providers/models/src/lib.rs), its `provider.toml`, its newest
`PER_PROVIDER` models llx can work with (type chat, tools supported, text out, not deprecated,
not `v2_only`: read as build.rs resolves it from its base model), every decision model it serves
that is not deprecated (`llx decide` takes one), and the lab files those name. Nothing in data/
is edited by hand: run this again, review the diff, commit.
"""
import pathlib
import re
import shutil
import sys

PER_PROVIDER = 24
CRATE = pathlib.Path(__file__).resolve().parent.parent / "packages/providers/models"
DATA = CRATE / "data"


def popular():
    """The providers lib.rs lists first: one list, so a provider added there is built in."""
    source = (CRATE / "src/lib.rs").read_text()
    found = re.search(r"pub const POPULAR: \[&str; \d+\] = \[(.*?)\];", source, re.S)
    if not found:
        sys.exit("POPULAR not found in packages/providers/models/src/lib.rs")
    return re.findall(r'"([^"]+)"', found.group(1))


def merged(base, over):
    """models.dev's merge: tables key by key, arrays and values replaced."""
    out = dict(base)
    for key, value in over.items():
        if isinstance(value, dict) and isinstance(out.get(key), dict):
            out[key] = merged(out[key], value)
        else:
            out[key] = value
    return out


def resolved(lab, own):
    """The model as build.rs resolves it, for what usable() reads: the lab's facts with the
    provider file's over them, less each `base_model_omit` path, its modalities the file's own."""
    model = merged(lab, own)
    for path in own.get("base_model_omit", []):
        *parents, last = path.split(".")
        at = model
        for part in parents:
            at = at.get(part) if isinstance(at, dict) else None
        if isinstance(at, dict):
            at.pop(last, None)
    if "modalities" in own:
        model["modalities"] = own["modalities"]
    return model


def usable(model):
    tools = model.get("capabilities", {}).get("tools", {})
    output = model.get("modalities", {}).get("output", [])
    return (model.get("type", "chat") == "chat" and tools.get("supported") is True
            and "text" in output and model.get("status") != "deprecated"
            and not model.get("v2_only"))


def decides(model):
    return model.get("type") == "decision" and model.get("status") != "deprecated"


def clone_given():
    """The clone the command line names. Anything else is a usage error, said before TOML is read
    (so on any Python) and before data/ is removed: a path that is not a clone would leave it
    empty."""
    usage = "usage: python3 scripts/models_sync.py MODELS_DEV_CLONE (a clone of models.dev's dev)"
    if len(sys.argv) != 2:
        print(usage, file=sys.stderr)
        sys.exit(2)
    clone = pathlib.Path(sys.argv[1])
    if not (clone / "providers").is_dir() or not (clone / "models").is_dir():
        print(f"models_sync: {clone} is not a models.dev clone (no providers/ or models/ in it)",
              file=sys.stderr)
        print(usage, file=sys.stderr)
        sys.exit(2)
    return clone


def toml():
    """tomllib, Python's own from 3.11, else tomli (`pip install tomli`)."""
    try:
        import tomllib
    except ModuleNotFoundError:  # Python before 3.11
        import tomli as tomllib
    return tomllib


def main():
    clone = clone_given()
    tomllib = toml()
    shutil.rmtree(DATA, ignore_errors=True)
    labs = set()
    for provider in popular():
        source = clone / "providers" / provider
        if not (source / "provider.toml").exists():
            print(f"skipped {provider}: not in the clone", file=sys.stderr)
            continue
        chosen, decision = [], []
        for path in sorted((source / "models").rglob("*.toml")):
            own = tomllib.loads(path.read_text())
            base = own.get("base_model")
            lab = {}
            if base:
                lab_path = clone / "models" / f"{base}.toml"
                if not lab_path.exists():
                    continue
                lab = tomllib.loads(lab_path.read_text())
            model = resolved(lab, own)
            if usable(model):
                chosen.append((model.get("release_date", ""), path, base))
            elif decides(model):
                decision.append((model.get("release_date", ""), path, base))
        chosen.sort(key=lambda item: item[0], reverse=True)
        target = DATA / "providers" / provider
        target.mkdir(parents=True)
        shutil.copy2(source / "provider.toml", target / "provider.toml")
        for _, path, base in chosen[:PER_PROVIDER] + decision:
            out = target / "models" / path.relative_to(source / "models")
            out.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, out)
            if base:
                labs.add(base)
    for base in sorted(labs):
        out = DATA / "models" / f"{base}.toml"
        out.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(clone / "models" / f"{base}.toml", out)
    print(f"copied into {DATA} ({len(labs)} lab files)", file=sys.stderr)


main()
