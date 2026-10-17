# Loutil – passive Dofus 3 packet reader

Strictly read-only: the game folder is never modified; files are *copied* to `work/dump_input`, and traffic is only
sniffed with Npcap. Nothing is injected or hooked.

## Layout
- `tools/` – Il2CppInspectorRedux (needs .NET 10, installed locally in `tools/dotnet10`; Il2CppDumper can't read metadata v39), protodec, grpcio-tools (`tools/pylib`).
- `work/dump_input/` – copies of `GameAssembly.dll` + `global-metadata.dat` (re-copy after each game update).
- `work/dump/dll/` – DummyDlls; `work/proto/*.proto` – obfuscated protos; `work/pb/` – generated python.
- `sniffer/sniff.py` – capture → TCP reassembly → VarInt framing → envelope → schema decode → `work/capture.jsonl`.

## Refresh after a game update
```
export DOTNET_ROOT=$PWD/tools/dotnet10 DOTNET_MULTILEVEL_LOOKUP=0
cp <game>/GameAssembly.dll <game>/Dofus_Data/il2cpp_data/Metadata/global-metadata.dat work/dump_input/
tools/inspector/Il2CppInspectorRedux.CLI-win-x64/Il2CppInspector.Redux.CLI.exe process work/dump_input/GameAssembly.dll work/dump_input/global-metadata.dat -o work/dump -d --unity-version 6000.0.0
tools/protodec/protodec.exe work/dump/dll/Ankama.Dofus.Protocol.Game.dll work/proto/game.proto
tools/protodec/protodec.exe work/dump/dll/Ankama.Dofus.Protocol.Connection.dll work/proto/connection.proto
python sniffer/build_protos.py
```
## Run (start before logging in for a clean stream; mid-stream joins auto-resync)
`cd sniffer && python sniff.py` (add `--raw` to store payload hex)

## Web UI
`cd sniffer && python webapp.py` → http://127.0.0.1:8765 (capture starts OFF; use the switch, or `--on`).
Filter by text/direction/kind; click a type in the left list to isolate it, shift-click to hide it.

## Clear names (beta protos)
`work/beta/` holds the old LuaxY beta protos (clear names, Oct 2024). `python sniffer/match_protos.py` matches them structurally
(field numbers are reshuffled per build, so they are ignored) and writes `work/mapping.json`. Conservative: only unambiguous matches
are kept (86 of ~2400 so far, e.g. MapMovementEvent, ChatChannelMessageEvent). Ambiguous fields show as `obf (a|b?)`.
Pin confirmed names in `work/overrides.json`: `{"kpy": {"name": "...", "fields": {"gaio": "sender_name"}}}`, then re-run + "Reload map".

## Game data (items, effects, names) – read from the client's own files
`python sniffer/extract_gamedata.py --refresh` copies `Content/Data/*dataroot.asset.bundle` + `Content/I18n/{fr,en}.bin` from the game
folder into `work/gamedata/` (copy only), parses the Unity ScriptableObject bundles with UnityPy (Unity 6000.3.16f1; bundles ship with a
stripped version so it is set explicitly) and writes `items/effects/characteristics/itemtypes.json`. `gamedata.py` reads names from the
i18n .bin (id → offset table + varint-length UTF-8) and renders effect templates (`#1{{~1~2 à }}#2 Force`). Re-run after each game update.
`sniffer/formatters.py` turns `jzs` (AH item list) into the "formatted" column of the web UI; obfuscated field names live at its top.
`POST /api/demo` injects a synthetic AH packet to preview the view without opening the AH.
