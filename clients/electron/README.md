# sshxx Electron client

This is the Windows-focused Electron shell for the shared Svelte client at the
repository root. It does not own session, terminal, or workspace state. It is an
experimental shell, not an independent-browser implementation: custom components
still use the shared client's sandboxed iframes. Building a portable executable
does not replace Windows functional testing.

```sh
npm install
npm run dev
npm run pack:win
```

`pack:win` writes a single-file, no-install Windows executable to `dist/`. The
portable executable extracts its bundled Chromium runtime when launched.

## Ubuntu build

Reuse the shared Web build, then generate a Debian package with the existing
builder:

```sh
npm run build:web
npx --no-install electron-builder --linux deb --x64 --publish never \
  --config.linux.category=Development \
  --config.linux.maintainer='sshxx contributors <glight2000@users.noreply.github.com>' \
  --config.extraMetadata.homepage=https://github.com/glight2000/sshxx
```

Install the resulting local `.deb` with `sudo apt install ./<package>.deb`, then
run `sshxx-electron-client`. The builder's standard installation scripts
configure Chromium sandbox support, including Ubuntu 24+ AppArmor handling. An
unpacked archive may not launch on restricted Ubuntu systems; do not bypass that
failure with `--no-sandbox`.

The packaged Web features follow the shared client build. This shell still does
not implement independent embedded browsers, and Electron has no Android build
target.
