"""Snapshots of the game builds the pack was exported for (pipeline/reference/builds, committed), so that the next build
can be matched against the previous one instead of the 2024 beta protos (see match_protos.py `match_previous`).

A snapshot = the build's protocol.desc (obfuscated) + `<label>.json.gz`:
  {"game": "3.7.12.12", "fingerprint": sha256 of the desc, "created": unix time,
   "names": {obf msg: {"name"?, "conf"?, "fields": {obf field or oneof: clear}}},   (final names, overrides applied)
   "script": [[obf short, file, name length, clear name or null], ...]}             (script table, declaration order)
Only the two newest snapshots are kept (the current build and the one before)."""
import gzip, hashlib, json, pathlib, time

root = pathlib.Path(__file__).resolve().parent.parent
DIR = pathlib.Path(__file__).with_name("reference") / "builds"
DESC = root / "work" / "pb" / "protocol.desc"
KEEP = 2
SURE = {"script", "anchor", "manual", "ref", "previous", "high"}   # message names (and their exact fields) worth carrying


def fingerprint(path=DESC):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()[:16]


def _all():
    out = []
    for p in DIR.glob("*.json.gz"):
        try:
            out.append((json.loads(gzip.decompress(p.read_bytes())), p))
        except (OSError, ValueError):
            print(f"[!] snapshot illisible : {p.name}")
    return sorted(out, key=lambda x: -x[0]["created"])


def save(names, script, game, desc=DESC):
    """Snapshot of the build in `desc` (replaces an older snapshot of the same build), then prune to `KEEP`."""
    DIR.mkdir(parents=True, exist_ok=True)
    fp = fingerprint(desc)
    label = f"{game}-{fp[:8]}" if game else fp
    created = time.time()
    for s, p in _all():
        if s["fingerprint"] == fp:   # same build exported again: same date, so an unchanged snapshot keeps its bytes
            created = s["created"]
            p.unlink(); p.with_name(p.name[:-len(".json.gz")] + ".desc").unlink(missing_ok=True)
    compact = {}
    for k, v in names.items():
        # only names worth carrying to the next build: a weak beta match is redone against the beta there anyway
        sure = v.get("conf") in SURE
        fields = {fk: fv["name"] for fk, fv in v.get("fields", {}).items() if fv.get("name") and (fv.get("manual") or fv.get("previous") or sure)}
        e = {kk: v[kk] for kk in ("name", "conf") if sure and v.get(kk)}
        if fields:
            e["fields"] = fields
        if e:
            compact[k] = e
    order = [[k, v.get("file"), v.get("len"), v.get("name")] for k, v in script.items() if v.get("conf") != "decoy"]
    doc = {"game": game, "fingerprint": fp, "created": created,"names": compact, "script": order}
    (DIR / f"{label}.desc").write_bytes(pathlib.Path(desc).read_bytes())
    # mtime 0: the archive's bytes only depend on its content
    (DIR / f"{label}.json.gz").write_bytes(gzip.compress(json.dumps(doc, ensure_ascii=False, separators=(",", ":")).encode(), mtime=0))
    for s, p in _all()[KEEP:]:
        p.unlink(); p.with_name(p.name[:-len(".json.gz")] + ".desc").unlink(missing_ok=True)
    return fp


def previous(cur_fp):
    """Newest snapshot of another build than `cur_fp` (dict + "desc" path), or None."""
    for s, p in _all():
        if s["fingerprint"] != cur_fp:
            s["desc"] = p.with_name(p.name[:-len(".json.gz")] + ".desc")
            return s if s["desc"].exists() else None
    return None
