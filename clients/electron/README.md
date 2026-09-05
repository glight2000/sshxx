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
