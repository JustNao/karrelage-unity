"""Version of the installed Dofus 3 client ("3.7.12.12"), as the Ankama launcher records it (read only):
%APPDATA%/zaap/repositories/production/dofus/dofus3/release.json -> "version": "6.0_3.7.12.12".

update_names.py stores it in work/dump_input/version.txt when it copies the game files, so the pack is labelled with the
build it was dumped from (`dumped_version()`), even if the game updates afterwards."""
import json, os, pathlib

root = pathlib.Path(__file__).resolve().parent.parent
STAMP = root / "work" / "dump_input" / "version.txt"


def installed_version():
    p = pathlib.Path(os.environ.get("APPDATA", "")) / "zaap" / "repositories" / "production" / "dofus" / "dofus3" / "release.json"
    try:
        v = json.loads(p.read_text(encoding="utf-8")).get("version") or ""
    except (OSError, ValueError):
        return None
    return v.rsplit("_", 1)[-1] or None


def dumped_version():
    """Version of the build in work/dump_input (stamped at copy time), else the installed one."""
    try:
        return STAMP.read_text(encoding="utf-8").strip() or installed_version()
    except OSError:
        return installed_version()


if __name__ == "__main__":
    print("installed:", installed_version(), "· dumped:", dumped_version())
