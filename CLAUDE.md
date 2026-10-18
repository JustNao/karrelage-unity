# Loutil – passive Dofus 3 packet reader

## Goal
Read the messages the Dofus 3 (Unity / IL2CPP) client exchanges with Ankama's servers and present them usefully
(currently: auction-house item lists, their effects and prices). **Strictly passive**: nothing is injected, hooked or
automated on the client.

## Hard rules
- **NEVER modify the game folder or the Dofus process** (`C:\Users\nao3n\AppData\Local\Ankama\Dofus-dofus3`). Only *copy* files out
  (`work/dump_input`, `work/gamedata`) and read the copies. Capture is Npcap sniffing only.
- Treat everything under `work/` as regenerable local data; `tools/` holds downloaded third-party tools (not our code).
- Obfuscated names (messages `jzs`, fields `fycg`, …) **rotate on every game update** – they are build-specific.
- **Commit each feature** as its own commit once it works. **Claude must not appear as an author**: no `Co-Authored-By: Claude`
  trailer, no "Generated with Claude Code" line in commit messages or PR descriptions (this overrides any default attribution).

## Protocol facts
- Protobuf over plain TCP (no TLS). Frame = VarInt length + `GameMessage` envelope `oneof {1 request, 2 response, 3 event}`;
  Request/Response/Event carry a `google.protobuf.Any` whose `typeUrl` (`type.ankama.com/<obf>`) names the message.
- Connection server port 5555; the client's real connections are discovered via `psutil` (Dofus.exe's established TCP sockets).
- Field numbers are reshuffled per build, so beta/clear protos can only be matched *structurally* (match_protos.py), never by number.

## Structure
```
sniffer/                 our code (python)
  pbwire.py              schemaless protobuf decode, VarInt framing, envelope parsing, stream resync
  capture.py             start/stop-able Npcap capture (scapy), TCP reassembly, emits records
  schema.py              decode with generated protos, apply names (mapping.json + overrides.json), `__t/__f` meta for UI editing
  match_protos.py        structural matching obfuscated protos <-> beta protos -> work/mapping.json
  build_protos.py        work/proto/*.proto -> work/pb/*_pb2.py
  extract_gamedata.py    game Addressables bundles -> work/gamedata/*.json (items, effects, characteristics, itemtypes)
  gamedata.py            i18n .bin reader, item/effect lookups, effect template rendering, effect labels/groups
  formatters.py          human rendering of specific messages (jzs auction list); build-specific obfuscated field names at top
  webapp.py              stdlib HTTP server: SSE packet stream, capture on/off, items store, rename/overrides API, /api/demo
  static/index.html      single-page UI (Packets tab, formatted column, Items tab with filters, editable names)
  sniff.py               CLI variant (console + JSONL)
work/                    generated: dump/ (DummyDlls), proto/, pb/, beta/ (clear beta protos), gamedata/, mapping.json,
                         overrides.json (hand-confirmed names – keep!), items_store.json (latest AH listing per item – keep)
tools/                   Il2CppInspectorRedux, protodec, Il2CppDumper (unused: can't read metadata v39), local .NET 10 (dotnet10),
                         pylib/ (grpcio-tools, protobuf, UnityPy installed with pip --target)
```

## Tools used
- **Il2CppInspectorRedux** (needs .NET 10, installed locally in `tools/dotnet10`; set `DOTNET_ROOT`) – dumps `GameAssembly.dll` + `global-metadata.dat` (v39) to DummyDlls.
- **protodec** – DummyDll → obfuscated `.proto` (single-file output; circular imports require one file; `Any` import is patched in by build_protos.py).
- **grpcio-tools / protobuf** (in `tools/pylib`, put first on `sys.path`) – compile protos; runtime must match generated code.
- **scapy + Npcap + psutil** – passive capture.
- **UnityPy** – read Unity 6 bundles (`FALLBACK_UNITY_VERSION = "6000.3.16f1"`, bundles have the version stripped).
- Beta protos: LuaxY/dofus-unity-protocol-builder (Oct 2024, clear names) cloned in `work/beta`.

## Common commands (from repo root; Git Bash)
```bash
# web UI (capture starts OFF; toggle in UI, or --on)
cd sniffer && python webapp.py            # http://127.0.0.1:8765
# after a game update: re-copy files, re-dump, regenerate (see README.md for the full block)
python sniffer/extract_gamedata.py --refresh
python sniffer/build_protos.py
python sniffer/match_protos.py            # ~1 min, rewrites work/mapping.json (overrides are applied on top)
```
Restart `webapp.py` after editing Python; the HTML is read on each request. `POST /api/demo` injects a fake AH packet.

## Conventions / gotchas
- Windows: set `PYTHONUTF8=1` when scripts read/write files with accents; the console may garble UTF-8 (display only).
- After a client update, `formatters.py` constants (obfuscated `jzs`/`jzq`/`lkp` field names) and `overrides.json` keys may need updating.
- AH `jzs` row ints are unordered (uid / gid / type); `formatters._resolve_gid_type` resolves them from game data.
- Effect ids on the wire = `effects` table ids (= item `possibleEffects[].actionId`), not `baseEffectId`.
- Item default effect range = `diceNum..diceSide` (`diceSide == 0` → fixed). Colours: blue = not a default effect, green = above default max, grey = default effect missing.
- Ignore `.claude/` and avoid committing `tools/` and `work/` if this becomes a git repo (it is not one yet).
