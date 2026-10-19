#!/usr/bin/env python3
"""Write the in-client updater's files for one platform of a release.

From a built GameData folder this writes, into --output:

  JoF-update-<platform>.zip   its files, paths relative to GameData
  JoF-update-<platform>.json  the manifest /update reads (codemp/client/cl_update.cpp)

publish-update-files.sh signs the manifest (JoF-update-<platform>.json.sig) and
attaches all three to the release; docs/updater.md has the whole picture.
"""

import argparse
import hashlib
import json
import re
import sys
import zipfile
from pathlib import Path

# The files the client agrees to install (Update_ValidPath in cl_update.cpp):
# programs and libraries beside it, and pk3s in base/ and EternalJK/.
INSTALLABLE = re.compile(r"(?:(?:base|EternalJK)/[A-Za-z0-9_.-]+\.pk3|[A-Za-z0-9_.-]+\.(?:exe|dll))", re.IGNORECASE)
# /update offers stable and beta releases only, never alpha ones.
RELEASE_TAG = re.compile(r"\d+\.\d+\.\d+(?:-beta\.\d+)?")
# Every entry gets the same time, so the same files give the same archive.
ENTRY_TIME = (2020, 1, 1, 0, 0, 0)


def sha512(path):
    digest = hashlib.sha512()
    with open(path, "rb") as file:
        for chunk in iter(lambda: file.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def pk3_intact(path):
    try:
        with zipfile.ZipFile(path) as pk3:
            return pk3.testzip() is None
    except (zipfile.BadZipFile, OSError):
        return False


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--gamedata", type=Path, required=True, help="the built GameData folder")
    parser.add_argument("--platform", required=True, choices=("windows-x86", "windows-x86_64"))
    parser.add_argument("--tag", required=True, help="the release tag, 1.6.2 or 1.6.2-beta.4")
    parser.add_argument("--channel", required=True, choices=("stable", "beta"))
    parser.add_argument("--commit", default="", help="the source commit, recorded in the manifest")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    if not RELEASE_TAG.fullmatch(args.tag):
        sys.exit(f"{args.tag} is not a stable or beta release tag")
    if ("-beta." in args.tag) != (args.channel == "beta"):
        sys.exit(f"--channel {args.channel} does not match --tag {args.tag}")

    files = sorted(path for path in args.gamedata.rglob("*") if path.is_file())
    names = [path.relative_to(args.gamedata).as_posix() for path in files]
    if not files:
        sys.exit(f"{args.gamedata} holds no files")
    refused = [name for name in names if not INSTALLABLE.fullmatch(name) or len(name) > 128]
    if refused:
        sys.exit("the client would refuse to install " + ", ".join(refused)
                 + " (widen Update_ValidPath and INSTALLABLE together)")
    if len({name.lower() for name in names}) != len(names):
        sys.exit("two files differ only in case")
    # A download that failed into an error page must never be signed as a pk3.
    broken = [name for path, name in zip(files, names)
              if name.lower().endswith(".pk3") and not pk3_intact(path)]
    if broken:
        sys.exit("not valid pk3s: " + ", ".join(broken))

    args.output.mkdir(parents=True, exist_ok=True)
    stem = f"JoF-update-{args.platform}"
    archive = args.output / f"{stem}.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as output:
        for path, name in zip(files, names):
            entry = zipfile.ZipInfo(name, ENTRY_TIME)
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o644 << 16
            output.writestr(entry, path.read_bytes())

    manifest = {
        "format": 1,
        "tag": args.tag,
        "channel": args.channel,
        "platform": args.platform,
        "commit": args.commit,
        "archive": {"name": archive.name, "size": archive.stat().st_size, "sha512": sha512(archive)},
        "files": [{"path": name, "size": path.stat().st_size, "sha512": sha512(path)}
                  for path, name in zip(files, names)],
    }
    with open(args.output / f"{stem}.json", "w", encoding="utf-8", newline="\n") as file:
        file.write(json.dumps(manifest, indent=2) + "\n")
    print(f"{stem}: {len(files)} files, {archive.stat().st_size} bytes")


if __name__ == "__main__":
    main()
