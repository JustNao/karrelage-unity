"""Recompute the data pack's message names (what the app's developer "Noms" tab runs).

  python pipeline/update_names.py               # names only: match_protos.py + export_pack.py (protos already built for this game build)
  python pipeline/update_names.py --from-game   # whole chain after a game update: copy the game files, dump them with
                                                # Il2CppInspectorRedux, protodec -> protos, build_protos, game data, match, export

The game folder is only READ (two files are copied into work/dump_input); nothing there is ever modified.
Restart Karrelage afterwards so it loads the new datapack/.
"""
import os, pathlib, shutil, subprocess, sys, time

root = pathlib.Path(__file__).resolve().parent.parent
py = sys.executable
env = {**os.environ, "PYTHONUTF8": "1", "PYTHONUNBUFFERED": "1"}
GAME = pathlib.Path(os.environ.get("LOUTIL_GAME") or pathlib.Path(os.environ.get("LOCALAPPDATA", "")) / "Ankama" / "Dofus-dofus3")


FULL = "--from-game" in sys.argv
TOTAL = 7 if FULL else 2   # steps, reported to the app's progress bar as "@@STEP k n title" lines
_k = 0


def step(title):
    global _k
    _k += 1
    print(f"@@STEP {_k} {TOTAL} {title}", flush=True)
    print(f"\n== {title} ==", flush=True)


def run(cmd, extra_env=None):
    print("$ " + " ".join(str(c) for c in cmd), flush=True)
    t = time.time()
    r = subprocess.run([str(c) for c in cmd], cwd=root, env={**env, **(extra_env or {})})
    if r.returncode:
        print(f"ECHEC (code {r.returncode})", flush=True)
        sys.exit(r.returncode)
    print(f"ok ({time.time() - t:.0f}s)", flush=True)


def from_game():
    dump_input = root / "work" / "dump_input"
    dump_input.mkdir(parents=True, exist_ok=True)
    step("Copie des fichiers du jeu (lecture seule)")
    for src in (GAME / "GameAssembly.dll", GAME / "Dofus_Data" / "il2cpp_data" / "Metadata" / "global-metadata.dat"):
        if not src.exists():
            sys.exit(f"fichier introuvable : {src} (variable LOUTIL_GAME pour changer le dossier du jeu)")
        shutil.copy2(src, dump_input / src.name)
        print("copié", src.name, flush=True)

    step("Dump IL2CPP (Il2CppInspectorRedux)")
    dotnet = root / "tools" / "dotnet10"
    inspector = root / "tools" / "inspector" / "Il2CppInspectorRedux.CLI-win-x64" / "Il2CppInspector.Redux.CLI.exe"
    run([inspector, "process", dump_input / "GameAssembly.dll", dump_input / "global-metadata.dat",
         "-o", root / "work" / "dump", "-d", "--unity-version", "6000.0.0"],
        {"DOTNET_ROOT": str(dotnet), "DOTNET_MULTILEVEL_LOOKUP": "0"})

    step("DummyDll -> .proto (protodec)")
    (root / "work" / "proto").mkdir(parents=True, exist_ok=True)
    protodec = root / "tools" / "protodec" / "protodec.exe"
    for dll, out in (("Ankama.Dofus.Protocol.Game.dll", "game.proto"), ("Ankama.Dofus.Protocol.Connection.dll", "connection.proto")):
        run([protodec, root / "work" / "dump" / "dll" / dll, root / "work" / "proto" / out])

    step("Compilation des protos")
    run([py, "pipeline/build_protos.py"])

    step("Données du jeu (objets, effets, textes)")
    run([py, "pipeline/extract_gamedata.py", "--refresh"])


if "--from-game" in sys.argv:
    from_game()

step("Calcul des noms (correspondance structurelle avec les protos beta)")
run([py, "pipeline/match_protos.py"])

step("Export du pack de données")
run([py, "pipeline/export_pack.py"])

print(f"@@STEP {TOTAL} {TOTAL} fin", flush=True)
print("\nTERMINE : redémarrez Karrelage pour charger le nouveau pack.", flush=True)
