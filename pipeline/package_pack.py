"""Package datapack/ for distribution -> dist/datapack.zip + dist/pack.json (committed; the app checks and downloads them from GitHub).

  python pipeline/package_pack.py

pack.json = {"pack", "generated", "size", "sha256", "file"}: the app compares `generated` with its own pack's manifest to know
whether an update exists, and checks size/sha256 after downloading. Run after export_pack.py (update_names.py does it)."""
import hashlib, json, pathlib, sys, zipfile

root = pathlib.Path(__file__).resolve().parent.parent
src = root / "datapack"
out = root / "dist"

manifest = json.loads((src / "manifest.json").read_text(encoding="utf-8"))
out.mkdir(exist_ok=True)
zip_path = out / "datapack.zip"
with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for f in sorted(src.iterdir()):
        if f.is_file():
            z.write(f, f.name)

data = zip_path.read_bytes()
meta = {"pack": manifest["pack"], "generated": manifest["generated"], "size": len(data),
        "sha256": hashlib.sha256(data).hexdigest(), "file": "datapack.zip"}
(out / "pack.json").write_text(json.dumps(meta, indent=1) + "\n", encoding="utf-8")
print(f"dist/datapack.zip: {len(data) / 1e6:.2f} MB (datapack/ is {sum(f.stat().st_size for f in src.iterdir()) / 1e6:.1f} MB), generated {manifest['generated']}")
