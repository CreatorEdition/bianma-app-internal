# bianma-app Flatpak Build Guide

This directory contains the Flatpak manifest for `bianma-app`, used to convert the generated `.deb` artifact into an installable `.flatpak` package via CI or local builds.

This document is a packaging and compatibility note for maintainers. It is not the current public installation guide or the current formal distribution entry for desktop users.

> Public brand: `bianma-app`. The `com.ccswitch.desktop`, `cc-switch.deb`, and `CC-Switch-Linux.flatpak` identifiers documented below are retained only for Flatpak compatibility and existing tooling continuity.

The manifest and app ID still use the legacy `com.ccswitch.desktop` names so existing Flatpak tooling stays compatible, even though the runtime it installs is bianma-app.

If you are looking for end-user install guidance, use the public README and user manual instead of this file.

## Dependencies

- `flatpak`
- `flatpak-builder`
- Flathub remote (for installing `org.gnome.Platform//46` runtime)

For Ubuntu/Debian:

```bash
sudo apt install flatpak flatpak-builder
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install -y --user flathub org.gnome.Platform//46 org.gnome.Sdk//46
```

## Local Build (Generate .flatpak from .deb)

1) Build the deb on Linux first:

```bash
pnpm tauri build -- --bundles deb
```

2) Copy the generated deb to this directory. The pipeline still expects a `cc-switch.deb` filename so keep that legacy name on disk before packaging:

```bash
cp "$(find src-tauri/target/release/bundle -name '*.deb' | head -n 1)" flatpak/cc-switch.deb
```

3) Build the local Flatpak repository and export the `.flatpak`. The repository and bundle names remain the older `cc-switch` identifiers and the output bundle is kept as `CC-Switch-Linux.flatpak` for continuity:

```bash
flatpak-builder --force-clean --user --disable-cache --repo flatpak-repo flatpak-build flatpak/com.ccswitch.desktop.yml
flatpak build-bundle --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo flatpak-repo CC-Switch-Linux.flatpak com.ccswitch.desktop
```

4) Install and run the Flatpak bundle using the historic identifiers:

```bash
flatpak install --user ./CC-Switch-Linux.flatpak
flatpak run com.ccswitch.desktop
```

## Permissions Note

The current manifest uses `--filesystem=home` by default for “download and run” convenience, allowing the app to directly read/write CLI configuration files and app data on the host (and supporting the “directory override” feature). Legacy paths like `~/.cc-switch` are kept in the manifest so existing installations still function; the live CLI is bianma-app.

If you prefer minimal permissions (e.g., for Flathub submission or security concerns), you can replace `--filesystem=home` in `flatpak/com.ccswitch.desktop.yml` with more precise grants:

```yaml
  - --filesystem=~/.cc-switch:create
  - --filesystem=~/.claude:create
  - --filesystem=~/.claude.json
  - --filesystem=~/.codex:create
  - --filesystem=~/.gemini:create
  - --filesystem=~/.config/opencode:create
  - --filesystem=~/.openclaw:create
```

Note: Flatpak's `:create` modifier only works with directories, not files. Therefore, `~/.claude.json` cannot use `:create`. If this file doesn't exist on the user's machine, the app may not be able to create it with restricted permissions. Users should either run Claude Code once to generate it, or manually create an empty JSON file (content: `{}`).

If you plan to publish on Flathub or want stricter permission control, adjust the `finish-args` in `flatpak/com.ccswitch.desktop.yml` accordingly.
