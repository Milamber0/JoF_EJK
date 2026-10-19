#!/usr/bin/env python3
"""Write the in-client updater's files for one platform of a release.

From a built GameData folder this writes, into --output:

  JoF-update-<platform>.zip   its files, paths relative to GameData
  JoF-update-<platform>.json  the manifest /update reads (codemp/client/cl_update.cpp)

publish-update-files.sh signs the manifest (JoF-update-<platform>.json.sig) and
attaches all three to the release; docs/updater.md has the whole picture.

The manifest's remove list names every file JoF shipped before that this release
no longer has, with every version of it: --previous gives earlier releases'
manifests (signatures already checked), --retired the files shipped before
releases had manifests.
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
# Retail game files the client never removes (Update_RetailFile).
RETAIL = {"base/assets0.pk3", "base/assets1.pk3", "base/assets2.pk3", "base/assets3.pk3", "jamp.exe", "jasp.exe"}
# /update offers stable and beta releases only, never alpha ones.
RELEASE_TAG = re.compile(r"\d+\.\d+\.\d+(?:-beta\.\d+)?")
SHA512_HEX = re.compile(r"[0-9a-f]{128}")
# Every entry gets the same time, so the same files give the same archive.
ENTRY_TIME = (2020, 1, 1, 0, 0, 0)
# The client's limits (cl_update.cpp).
MAX_MANIFEST_BYTES = 1024 * 1024
MAX_REMOVALS = 256
MAX_REMOVAL_VERSIONS = 1024


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


def shipped_before(previous, retired, platform):
    """Every version of every file JoF shipped before: {lowercase path: (path, {(size, sha512)})}.

    previous holds earlier releases' manifests as <tag>/JoF-update-<platform>.json.
    Their files and their own remove lists both count, so a file dropped long ago
    is still removed for a player who skipped the releases in between.
    """
    known = {}

    # A bad entry in retired-files.json is a mistake to fix; in an earlier release's
    # manifest it is skipped, so rules tightened since never block a release.
    def add(path, size, digest, source, strict):
        problem = None
        if not isinstance(path, str) or not INSTALLABLE.fullmatch(path) or len(path) > 128 or path.lower() in RETAIL:
            problem = f"{source} names {path!r}, which the client may not remove"
        elif not isinstance(size, int) or isinstance(size, bool) or size < 0 or not isinstance(digest, str) \
                or not SHA512_HEX.fullmatch(digest.lower()):
            problem = f"{source} gives {path} no valid size and SHA-512"
        if problem and strict:
            sys.exit(problem)
        if problem:
            print(f"skipping: {problem}", file=sys.stderr)
            return
        known.setdefault(path.lower(), (path, set()))[1].add((size, digest.lower()))

    if previous:
        for manifest_path in sorted(previous.glob(f"*/JoF-update-{platform}.json")):
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            source = f"the {manifest_path.parent.name} manifest"
            if manifest.get("format") != 1 or manifest.get("platform") != platform:
                print(f"skipping {source}: format {manifest.get('format')}, platform {manifest.get('platform')}", file=sys.stderr)
                continue
            for file in manifest.get("files", []):
                add(file.get("path"), file.get("size"), file.get("sha512"), source, strict=False)
            for old in manifest.get("remove", []):
                for version in old.get("versions", []):
                    add(old.get("path"), version.get("size"), version.get("sha512"), source, strict=False)
    if retired:
        for old in json.loads(retired.read_text(encoding="utf-8")).get("files", []):
            for version in old.get("versions", []):
                add(old.get("path"), version.get("size"), version.get("sha512"), retired.name, strict=True)
    return known


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--gamedata", type=Path, required=True, help="the built GameData folder")
    parser.add_argument("--platform", required=True, choices=("windows-x86", "windows-x86_64"))
    parser.add_argument("--tag", required=True, help="the release tag, 1.6.2 or 1.6.2-beta.4")
    parser.add_argument("--channel", required=True, choices=("stable", "beta"))
    parser.add_argument("--commit", default="", help="the source commit, recorded in the manifest")
    parser.add_argument("--previous", type=Path, help="earlier releases' signature-checked manifests, <tag>/JoF-update-<platform>.json")
    parser.add_argument("--retired", type=Path, help="files shipped before releases had manifests (retired-files.json)")
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

    shipping = {name.lower() for name in names}
    remove = [{"path": path, "versions": [{"size": size, "sha512": digest} for size, digest in sorted(versions)]}
              for key, (path, versions) in sorted(shipped_before(args.previous, args.retired, args.platform).items())
              if key not in shipping]
    if len(remove) > MAX_REMOVALS:
        sys.exit(f"{len(remove)} files to remove; the client takes at most {MAX_REMOVALS}")
    crowded = [old["path"] for old in remove if len(old["versions"]) > MAX_REMOVAL_VERSIONS]
    if crowded:
        sys.exit(f"more than {MAX_REMOVAL_VERSIONS} versions to remove of " + ", ".join(crowded))

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
        "remove": remove,
    }
    text = json.dumps(manifest, indent=2) + "\n"
    if len(text.encode("utf-8")) > MAX_MANIFEST_BYTES:
        sys.exit(f"the manifest would be {len(text)} bytes; the client reads at most {MAX_MANIFEST_BYTES}")
    with open(args.output / f"{stem}.json", "w", encoding="utf-8", newline="\n") as file:
        file.write(text)
    print(f"{stem}: {len(files)} files, {len(remove)} to remove, {archive.stat().st_size} bytes")


if __name__ == "__main__":
    main()
