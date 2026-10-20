# In-client updater

The Windows client can update itself from the GitHub releases, without the player
downloading anything by hand.

## What players see

- At start-up the client looks for a newer release. When there is one, the console
  says `An update is available (JoF 1.6.2). Type /update to update.`, and says it
  again the first time the player is in a match, where the notify lines show it.
- `/update` downloads the release in the background, checks it and stages it, then
  says `JoF 1.6.2 is downloaded. Type /update to restart and install it now, or it
  installs the next time you start JoF.` The second `/update` restarts the client.
- By default only stable (master) releases are offered. `/betaupdates 1` offers
  the newest beta as well, and looks for it at once; `/betaupdates 0` goes back.
  **Alpha builds are never offered**, whatever the setting.
- `cl_updateCheck 0` stops the start-up check, so the client makes no request to
  GitHub until the player types `/update`.
- Linux and macOS do not update themselves yet: `/update` there points to the
  releases page.

## How it works

The code is [cl_update.cpp](../codemp/client/cl_update.cpp). A worker thread does
the network and file work; the main thread only prints what it did.

**Release files.** A release the updater can install carries, for `windows-x86`
and `windows-x86_64`:

| file | what it is |
|------|------------|
| `JoF-update-<platform>.zip` | the GameData files, paths relative to GameData |
| `JoF-update-<platform>.json` | the manifest: tag, channel, platform, the size and SHA-512 of the zip and of every file, and the files to remove |
| `JoF-update-<platform>.json.sig` | the Ed25519 signature of the manifest |

The player-facing `JoF-windows-*.zip` downloads are unchanged.
[make_update_package.py](../scripts/update/make_update_package.py) writes the zip
and the manifest, and [publish-update-files.sh](../scripts/update/publish-update-files.sh)
signs them and attaches them to the release. The `update-files-beta` and
`update-files-master` jobs in [build.yml](../.github/workflows/build.yml) run it
after the release is published, so a failure there never holds a release back.

**Finding a release.**
- Stable: `https://github.com/JediofFreedom/JoF_EJK/releases/latest/download/JoF-update-<platform>.json`.
  GitHub's "latest" release is never a prerelease, and this needs no API call, so
  there is no rate limit.
- Beta (`betaupdates 1`): one API call,
  `GET /repos/JediofFreedom/JoF_EJK/git/matching-refs/tags/<stable>-beta.`, lists
  the beta tags on top of the newest stable release. The client takes the highest
  one whose release has good update files, trying up to eight: a tag is pushed
  before its release is published, and anyone who can push may push a bogus one. It then offers whichever is newer, that beta or
  the stable release.
- Versions compare as `major.minor.patch`, then a beta counts as newer than the
  stable release it is built on: `1.6.2-beta.4` is newer than `1.6.2` and older
  than `1.6.3`.

**What is checked.** Anyone with write access can replace a release's files, so
the client trusts only what the release signing key signed:
- the manifest's signature, against the public keys built into the client from
  [update-signing-key.pub](../scripts/update/update-signing-key.pub);
- the manifest's tag (never `-alpha.`), its channel matching the tag, and its platform;
- that the release is newer than the installed one;
- the SHA-512 and size of the downloaded zip, and of every file unpacked from it;
- every path: only `.exe` and `.dll` files beside the client, and `.pk3` files in
  `base/` and `EternalJK/`. Nothing else is ever written, so not even a signed
  manifest can drop a config or reach outside the install folder.

**Installing.** `/update` stages the files that differ from the installed ones
in `GameData/jofupdate/pending/`. The swap happens at the next start, first thing
in `main()`, before anything has a pk3 open: Windows cannot replace a file that is
in use. Each file is renamed to `<name>.old` and the new one moved into place. If
any file fails, every one is put back and the old version starts; it is tried again
at the next start, and dropped after three failures (typically another JoF window
holding the pk3s). After a swap the client starts the new program with the same
command line, and the `.old` files are deleted at the following start. A staged
update older than the running build (one installed by hand meanwhile) is dropped.
`jofupdate/lock` keeps two JoF windows from staging, swapping or cleaning up at the
same time. The `.old` files stay while an update is pending, because a swap that
was stopped needs them.

A swap stopped halfway, by a crash or the power going, leaves
`jofupdate/pending/applying.txt`. The next start finishes it, even when the new
program went in first and is the one running. Before the swap moves a file aside,
it notes `aside <path>` in that file's journal and writes it to the disk. So a
resumed swap knows which `.old` files are its own backups, even for a file stopped
between its two renames, and never takes an older `.old` for one. Each record goes
on a line of its own. One the power cut tore while it was being written is passed
over, since its file was not moved aside yet. The intact records before and after
it still count. If the rest still cannot go in,
everything goes back, including what the stopped swap had moved, so the old
release is whole again. When that puts the old program back under a new one that
is running, the client says so in a message box and exits, rather than run with
half of each release.

**Removing files.** A file an earlier release shipped that a newer one no longer
has, such as a renamed pk3, would otherwise stay in GameData and keep loading. The
manifest's `remove` list names each one, with the size and SHA-512 of every version
JoF shipped:
- The client removes a file only if it is still byte for byte one of those
  versions. A file the player changed or replaced stays, and the console says so.
  So do the running program and the retail game's files (`base/assets0-3.pk3`,
  `jamp.exe`, `jasp.exe`), whatever a manifest says, and the same path rules as
  for installing apply.
- The file is checked when `/update` downloads, and removed by the start-up swap
  like any other: it is renamed to `<name>.old`, put back if the swap fails, and
  left alone if it changed since it was checked.
- `make_update_package.py` builds the list. `publish-update-files.sh` fetches the
  manifests of the newest 100 earlier releases and keeps only those signed with a
  listed key, since anyone who can push could replace a release's files and a
  forged manifest must never get a file deleted. Every file those manifests ship,
  or remove in turn, that this release lacks goes on the list, so a file stays on
  it for good once dropped. A file shipped before releases had manifests (1.6.1
  and earlier) goes in [retired-files.json](../scripts/update/retired-files.json)
  by hand, with the size and SHA-512 of each version from the old release zips.

Files a player added themselves are never touched: only what JoF itself shipped
can be removed.

The manifest is uploaded last, so a release whose files are still going up looks
like one without update files, not like a broken one.

## Setting up the signing key (once)

This was done on 2026-10-19; the steps are kept for a new key. Without a key in
`update-signing-key.pub` (a line of all zeros) and in the secret, releases get no
update files and `/update` offers nothing.

1. On a trusted machine, generate the key and print its public half:

   ```sh
   openssl genpkey -algorithm ed25519 -out jof-update-signing.pem
   openssl pkey -in jof-update-signing.pem -pubout -outform DER | tail -c 32 | od -An -tx1 | tr -d ' \n'
   ```

2. Replace the zeros in `scripts/update/update-signing-key.pub` with the 64 hex
   digits, through a pull request to `beta`.
3. In the repository settings, under **Environments**, create `release-signing`.
   Under **Deployment branches and tags**, choose **Selected branches and tags**, and
   add two rules, `master` and `beta`, each with **Ref type: Branch**. Never add a tag
   rule: everyone with write access can push tags. Optionally, add required
   reviewers too.
4. Only then add the environment secret `UPDATE_SIGNING_KEY`, holding the whole
   PEM file.
5. Keep the PEM somewhere safe offline, such as a password manager, and delete the
   other copies.

Do steps 3 and 4 in that order. The environment is what keeps the key from
workflows that run on `alpha`, where everyone can push: only jobs on the `master`
and `beta` branches can read its secret. Never use the environment in a
`pull_request_target` workflow or in any job that runs a pull request's code:
those run as `beta` or `master` too.

What the key signs is only as trustworthy as what the build puts in GameData.
That includes `JoF_Animations.pk3` and `JoF_AssetsExtra.pk3` from jofhub.eu and
`rd-vulkan` from JKSunny's latest EternalJK release. A pk3 can carry code: the
client loads cgame and ui from pk3s. `make_update_package.py` refuses a pk3 that is
not a valid zip, such as a download that failed into an error page.

## Rotating the key

Clients accept any key listed in `update-signing-key.pub`, one per line.

1. Add the new key's line and release with the old key still in the secret.
2. Once most players run that release, put the new key in `UPDATE_SIGNING_KEY`.
3. Later, remove the old line.

Players on a release older than step 1 can then no longer update in-client and
need one manual download. The same applies if the key is ever lost. A leaked key
stays trusted by the clients that list it until they update past its removal.

## Testing locally

[tests/updater](../tests/updater/README.md) checks the updater's logic without the
engine or the network: version order, allowed paths, manifests and signatures,
the zip reader against the release script's output, and the start-up swap.

To try the whole thing in a real client:

1. Build it with a throwaway key and an older release tag:
   `-DJOF_UPDATE_PUBKEY_HEX=<64 hex digits> -DJOF_RELEASE_TAG=1.6.1-beta.119`.
   A build without a tag says it cannot update itself; `cl_updateAs <tag>` makes it
   check as that release instead.
2. Make a release folder with the throwaway key, as the CI does:
   `DRY_RUN=<releases>/1.6.1-beta.121 UPDATE_SIGNING_KEY="$(cat key.pem)" scripts/update/publish-update-files.sh 1.6.1-beta.121 beta <artifacts>`,
   where `<artifacts>/windows-x86_64/GameData` (and `windows-x86`) holds the files.
   The script refuses keys not listed in `update-signing-key.pub`, so list the
   throwaway one in a local copy.
3. Serve it with [test-server.py](../scripts/update/test-server.py)
   (`python test-server.py <releases> 8765 [latest stable tag]`). Then start the
   client with `+set cl_updateURL "http://127.0.0.1:8765"`. The quotes must reach
   the game, because `//` starts a comment: in Git Bash write
   `'"http://127.0.0.1:8765"'`, in cmd `\"http://127.0.0.1:8765\"`.
   `cl_updateURL` must be `https://`, or `http://127.0.0.1:<port>` for such a test
   server, and the signature is still required.
4. Point `fs_cdpath` at a real GameData for the retail assets, so the test
   install needs only the client's own files.
