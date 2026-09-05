# sshxx Godot client

This is the native Godot client experiment for Android and future spatial UI. It
uses Godot 4.7.2 with GDScript. The first milestone validates the 3D scene,
basic single-touch panning, native controls, and Android export path. Terminal,
note, and browser panels are static placeholders, not functional components.
Touch zoom and the protocol/browser integration remain unimplemented.

The locally licensed Godot MCP Pro addon is intentionally ignored by Git and
must not be redistributed with this repository or exported application.

```sh
mise exec -- godot --editor --path clients/godot
```

After configuring Godot's Android SDK path, build the ARM64 debug APK with:

```sh
mise exec -- godot --headless --path clients/godot \
  --export-debug Android build/sshxx-client-godot-0.1.0-android-arm64.apk
```

This prototype does not yet connect to the sshxx protocol. GDExtension is a
candidate for reusing Rust code, not a validated integration. APK export alone
does not validate terminal emulation, independent browsers, or collaboration.

Run the gesture-release regression check with:

```sh
mise exec -- godot --headless --path clients/godot --script tests/input_test.gd
```
