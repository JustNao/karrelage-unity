"""Import a message-name list (obfuscated type -> clear name) into pipeline/reference/message_names.json.

The source is a JSON-ish text such as {"type.ankama.com/kkc": "MessageType.ContextCreationRequest", ...} (it may contain
stray double commas). Usage: python import_reference_names.py <file.txt>
The reference is applied by schema.mapping(): over the auto beta matches, under work/overrides.json."""
import json, pathlib, re, sys

PAIR = re.compile(r'"type\.ankama\.com/([^"]+)"\s*:\s*"([^"]+)"')


def parse(text):
    return {obf: name.removeprefix("MessageType.") for obf, name in PAIR.findall(text)}


if __name__ == "__main__":
    src = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
    names = parse(src)
    out = pathlib.Path(__file__).with_name("reference") / "message_names.json"
    out.parent.mkdir(exist_ok=True)
    out.write_text(json.dumps(dict(sorted(names.items())), indent=1, ensure_ascii=False), encoding="utf-8")
    print(f"{len(names)} names -> {out}")
