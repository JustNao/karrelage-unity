"""Compile work/proto/{game,connection}.proto -> work/pb/*_pb2.py + work/pb/protocol.desc (FileDescriptorSet for the app).
Needs tools/pylib grpcio-tools."""
import sys, pathlib
root = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(root / "tools" / "pylib"))
from grpc_tools import protoc
import grpc_tools

inc = str(pathlib.Path(grpc_tools.__file__).parent / "_proto")
src = root / "work" / "proto"
out = root / "work" / "pb"
out.mkdir(parents=True, exist_ok=True)

HDR = 'syntax = "proto3";'
IMPORT = 'import "google/protobuf/any.proto";'
for f in ("game.proto", "connection.proto"):
    t = (src / f).read_text(encoding="utf-8")
    if "google.protobuf.Any" in t and IMPORT not in t:
        (src / f).write_text(t.replace(HDR, HDR + "\n" + IMPORT, 1), encoding="utf-8")

rc = protoc.main(["protoc", f"-I{src}", f"-I{inc}", f"--python_out={out}", f"--descriptor_set_out={out / 'protocol.desc'}", "--include_imports", "game.proto", "connection.proto"])
sys.exit(rc)
