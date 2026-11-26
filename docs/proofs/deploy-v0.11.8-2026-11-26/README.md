# Local deployment proof: v0.11.8 (2026-11-26)

> **Referenz.** Activation of the already published `v0.11.8` release assets
> in the local deployment folder. No new tag, no new GitHub release, no remote
> or production target, no live brain/image turn.

**Approval.** The owner approved stopping the running bridge and activating
the verified release assets with a rollback backup, and decided that
`v0.11.8` stays the delivery version — no `v0.11.9` for the crate extraction
merged after the release.

## Inventory before

- Active folder: `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\release\`.
- Active `webagent.exe`: SHA-256 `8b7877c2136c035e8e5b3d8fa51c5fe92f76948ad2cd0e2146ceed5bb9696e52`, `--version` → `webagent 0.11.7 (93d33a3f2+dirty)`.
- Active `WebView2Loader.dll`: SHA-256 `465a7ddfb3a0da4c3965daf2ad6ac7548513f42329b58aebc337311c10ea0a6f`.
- Running process: PID `17372`, binary path the active `webagent.exe`, listening on `127.0.0.1:8788`.
- A `v0.11.8\` folder existed but contained a **different local build**
  (`f5970f7d4237864ffe8aa3125b645c7c76c4204dbbc5b0953232df82d4045f0e`,
  reporting `fb7b88607`); it was not used for the activation.
- Published GitHub release `v0.11.8` (tag on `fb7b886`) has seven assets; the
  Windows EXE and DLL plus the `.sha256` files were downloaded into
  `C:\Users\storax\AppData\Local\Temp\opencode\v0118-assets\`.

## Steps

1. Copied the three active files (`webagent.exe`, `WebView2Loader.dll`,
   `webagent-windows-x86_64.exe.sha256`) to
   `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\v0.11.7-active-20261126-backup\`
   and verified both binary hashes match the originals. Older rollback folders
   `v0.11.5-backup`, `v0.11.6-backup`, `v0.11.7-backup` remain untouched.
2. Stopped PID `17372`; verified the process was gone and `127.0.0.1:8788` had
   no listener.
3. Copied the downloaded **published** assets into the active folder as
   `webagent.exe`, `WebView2Loader.dll` and
   `webagent-windows-x86_64.exe.sha256`.

## Verification after

- Active `webagent.exe` SHA-256:
  `a58d319482e8cb652a692f854d2dbad8514e20fb80d5f8669ad406c30f09719f`,
  equal to the downloaded asset and to the release `.sha256` file.
- Active `WebView2Loader.dll` SHA-256:
  `465a7ddfb3a0da4c3965daf2ad6ac7548513f42329b58aebc337311c10ea0a6f`,
  equal to the downloaded asset.
- `webagent.exe --version` → `webagent 0.11.8 (fb7b88607)`, exit 0;
  `--help` → usage, exit 0.
- No network call to a brain provider, no browser window, no login and no
  credential/profile file access. The local bridge was **not restarted**: any
  live turn remains an owner action.

## Rollback

Stop any running `webagent`, then copy `webagent.exe`, `WebView2Loader.dll`
and `webagent-windows-x86_64.exe.sha256` from
`v0.11.7-active-20261126-backup\` back into `release\` and verify the
`webagent 0.11.7 (93d33a3f2+dirty)` version string again.

## Grenze

This is a local deployment only. No remote/production target was found or
used, no tag or GitHub release was created or moved, and no new release was
published for the crate extraction merged into `master` afterwards.
