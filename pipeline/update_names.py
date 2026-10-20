"""Recompute the data pack's message names (what the app's developer "Noms" tab runs).

  python pipeline/update_names.py               # names only: script_names.py + match_protos.py + export_pack.py (protos already built for this game build)
  python pipeline/update_names.py --repack      # only export_pack.py + package_pack.py (no matching, no dump): refresh dist/datapack.zip
  python pipeline/update_names.py --from-game   # whole chain after a game update: copy the game files, dump them with
                                                # Il2CppInspectorRedux, protodec -> protos, build_protos, game data, match, export

The game folder is only READ (two files are copied into work/dump_input); nothing there is ever modified.
Restart Karrelage afterwards so it loads the new datapack/.
"""
import os, pathlib, queue, shutil, subprocess, sys, threading, time

root = pathlib.Path(__file__).resolve().parent.parent
py = sys.executable
env = {**os.environ, "PYTHONUTF8": "1", "PYTHONUNBUFFERED": "1"}
GAME = pathlib.Path(os.environ.get("LOUTIL_GAME") or pathlib.Path(os.environ.get("LOCALAPPDATA", "")) / "Ankama" / "Dofus-dofus3")


FULL = "--from-game" in sys.argv
REPACK = "--repack" in sys.argv
TOTAL = 7 if FULL else 1 if REPACK else 2   # steps, reported to the app's progress bar as "@@STEP k n title" lines
_k = 0


def step(title):
    global _k
    _k += 1
    print(f"@@STEP {_k} {TOTAL} {title}", flush=True)
    print(f"\n== {title} ==", flush=True)


def kill_tree(pid):
    subprocess.run(["taskkill", "/T", "/F", "/PID", str(pid)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def run(cmd, extra_env=None, done_marker=None, grace=30, limit=None):
    """Run a tool, relaying its output line by line. `done_marker`: a tool that prints this line has finished its work;
    if it has not exited `grace` s later it is stuck (Il2CppInspectorRedux sometimes never returns after its export) and is
    killed with its children. `limit`: hard timeout in s."""
    print("$ " + " ".join(str(c) for c in cmd), flush=True)
    t = time.time()
    # no stdin: some tools wait for a keypress when done
    p = subprocess.Popen([str(c) for c in cmd], cwd=root, env={**env, **(extra_env or {})}, stdin=subprocess.DEVNULL,
                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    done_at = exited_at = None
    killed = eof = False
    lines = queue.Queue()
    threading.Thread(target=lambda: ([lines.put(l) for l in p.stdout], lines.put(None)), daemon=True).start()
    while not eof:
        try:
            l = lines.get(timeout=1)
        except queue.Empty:
            l = False
        if l is None:
            eof = True
        elif l:
            text = l.decode("utf-8", "replace").rstrip()
            print(text, flush=True)
            if done_marker and done_marker in text and done_at is None:
                done_at = time.time()
        if p.poll() is not None:
            exited_at = exited_at or time.time()
            if time.time() - exited_at > 3:
                break   # exited, but a child it left behind still holds the pipe: do not wait for it
        elif done_at is not None and time.time() - done_at > grace:
            print(f"(le programme ne s'est pas arrêté {grace}s après la fin de son travail : arrêt forcé)", flush=True)
            kill_tree(p.pid)
            killed = True
            break
        elif limit and time.time() - t > limit:
            print(f"ECHEC : délai dépassé ({limit}s), arrêt forcé", flush=True)
            kill_tree(p.pid)
            sys.exit(1)
    code = 0 if killed else p.wait()
    if code:
        print(f"ECHEC (code {code})", flush=True)
        sys.exit(code)
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
        {"DOTNET_ROOT": str(dotnet), "DOTNET_MULTILEVEL_LOOKUP": "0"}, done_marker="Export finished", limit=1800)

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

if not REPACK:
    step("Calcul des noms (correspondance structurelle avec les protos beta)")
    run([py, "pipeline/script_names.py"])   # clear names / name lengths from the game's Unity script table
    run([py, "pipeline/match_protos.py"])

step("Export du pack de données")
run([py, "pipeline/export_pack.py"])
run([py, "pipeline/package_pack.py"])   # dist/: what the app downloads from GitHub

print(f"@@STEP {TOTAL} {TOTAL} fin", flush=True)
print("\nTERMINE : redémarrez Karrelage pour charger le nouveau pack.", flush=True)
