# MineTrace macOS release guide

This guide defines the macOS filesystem, security, packaging, and release contract for MineTrace. It complements the Windows-first product blueprint in `minestats.md`; it does not fork the product or its data model.

## Public release target

MineTrace v1 is intended for direct distribution as a notarized Developer ID application in a universal DMG. The locally verified artifact is currently an ad-hoc-signed Apple Silicon build; it must not be represented as a notarized public download until the release gates below pass.

- Hardened Runtime is enabled.
- App Sandbox is not enabled.
- Minecraft source folders are opened read-only by Rust backend code.
- The webview receives no broad filesystem capability.
- The application database, settings, logs, and generated thumbnails live in the Tauri app-data/cache directories, never in the application bundle or mounted DMG.
- Network behavior remains disabled by default. Optional profile lookup and updating require separate, explicit user opt-in.

Hardened Runtime and App Sandbox are different controls. Direct distribution still uses signing, notarization, Gatekeeper, and Hardened Runtime. App Sandbox is deferred because a sandboxed build cannot automatically traverse launcher data outside its container. A future Mac App Store build must use a separate Tauri configuration, explicit folder selection, and persistent security-scoped bookmarks; it must not use temporary absolute-path entitlement exceptions.

References:

- [Tauri macOS application bundles](https://v2.tauri.app/distribute/macos-application-bundle/)
- [Tauri DMG distribution](https://v2.tauri.app/distribute/dmg/)
- [Tauri macOS signing and notarization](https://v2.tauri.app/distribute/sign/macos/)
- [Apple: accessing files from the macOS App Sandbox](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox)

## Verified launcher roots

Automatic discovery probes only exact candidates. It must not recursively search all of the home directory, `~/Library`, or `~/Documents`.

| Launcher | macOS data root | Validation | Verified on this Mac |
| --- | --- | --- | --- |
| Official Minecraft Java | `~/Library/Application Support/minecraft` | At least two strong markers such as `logs/`, `versions/`, `saves/`, and `options.txt` | Yes: readable, with plain/compressed logs and local worlds |
| Prism Launcher | `~/Library/Application Support/PrismLauncher` | `prismlauncher.cfg` plus the default `instances` directory | Yes: readable; the verified installation uses `InstanceDir=instances` |
| CurseForge | User-selected root, commonly `~/Documents/curseforge/minecraft` | Multi-marker manual validation; no automatic adapter yet | Verified only through the manual-folder flow |

The local verification on 2026-08-06 found 8 Official `.log`/`.log.gz` files, 93 Official world directories, 2 Prism instance directories, 1 Prism nested log, 17 CurseForge instance directories, and 60 CurseForge nested logs. These counts establish realistic test coverage only. Never commit the user's launcher files; create minimal, redacted fixtures instead.

`/Applications/Minecraft.app`, `/Applications/PrismLauncher.app`, and similar bundles are installation hints, not game-data roots. Prism portable/custom `--dir` roots and moved CurseForge roots are discovered from saved locations, launcher configuration when documented, or a user-selected folder.

MultiMC, Modrinth, ATLauncher, Lunar, Badlion, Feather, and other launchers remain manual or low-confidence generic discoveries until an adapter has documented paths, a real fixture, validation rules, and parser tests. Folder existence alone is not verified launcher support.

## Permission handling and recovery

An unsandboxed Developer ID app is still subject to POSIX permissions and macOS privacy controls. `~/Documents/curseforge/minecraft` is the most likely standard root to require user consent.

The current log-evidence v1 distinguishes usable persisted locations from unavailable ones and offers the native **Add folder** flow. It does not yet expose per-location **Choose again** or **Remove** controls, and automatic candidates that cannot be canonicalized are not surfaced as detailed permission diagnostics. Those controls and diagnostics are public-release follow-up work, not current UI capabilities.

The target permission-recovery sequence is:

1. Probe only the exact candidate with metadata/read checks.
2. Distinguish `missing`, `permission_denied`, `invalid_layout`, and `readable`; never report a denied folder as missing.
3. Show the affected launcher and privacy-redacted path.
4. Offer **Choose folder** through the native open panel.
5. Pass the selected folder to a controlled Rust command, validate its layout, and open source files read-only.
6. Persist the approved path and retry the scan.
7. If access later fails, retain the source as unavailable and offer **Choose again** or **Remove from MineTrace** once those commands and controls ship.

Do not request Full Disk Access. Do not grant the frontend `$HOME/**/*` or another broad `@tauri-apps/plugin-fs` scope. A future export feature must use a native save panel and a backend command that validates the destination and extension. Future screenshot views must create thumbnails in MineTrace's cache and expose only that cache to the webview.

If App Sandbox is introduced later, path strings are not sufficient for access after relaunch. Store read-only security-scoped bookmark data, resolve stale bookmarks, call the security-scope access APIs only for the operation's lifetime, and make automatic candidates explicit suggestions that the user approves.

## Shared and platform-specific backend boundary

The product has one backend. `cfg(target_os)` branches belong at a narrow platform edge.

| Shared between Windows and macOS | Platform-specific boundary |
| --- | --- |
| Domain models and confidence/provenance | Standard launcher root candidates |
| Launcher layout validation and adapters | Native path identity/display helpers |
| Log, gzip, stats, NBT, server, crash, and screenshot parsers | Finder/Explorer reveal behavior |
| Session reconstruction and correction rules | Native menus, shortcut labels, reopen behavior, and optional launch-at-login |
| SQLite schema, migrations, repositories, and analytics | Bundle configuration, signing, and installers |
| Scan orchestration, cancellation, fingerprinting, and exports | Permission-specific messaging where the OS differs |
| Tauri command request/response types | Target-specific smoke tests |

Recommended modules:

```text
src-tauri/src/platform/
  mod.rs
  macos.rs
  windows.rs
src-tauri/src/discovery/
  context.rs
  candidates.rs
  official.rs
  prism.rs
  curseforge.rs
  generic.rs
```

`DiscoveryContext` supplies home, application-support/app-data, documents, saved, and user-selected roots. Platform code supplies candidates; shared adapters validate them. Tests inject temporary contexts instead of reading the developer's machine.

Use `PathBuf` and `OsStr` through the backend. Classify files with `Path::components()` rather than Windows separator strings such as `"\\stats\\"`. Do not lowercase canonical paths because APFS can be case-sensitive. Do not follow symlinks by default, and skip macOS metadata such as `.DS_Store`, `._*`, and `.Spotlight-V100`.

## Expected Tauri configuration

The shared `src-tauri/tauri.conf.json` owns the product name, version, identifier, windows, and common resources. The committed `src-tauri/tauri.macos.conf.json` is a merge overlay used only for macOS builds. It defines:

```json
{
  "bundle": {
    "targets": ["app", "dmg"],
    "category": "Utility",
    "macOS": {
      "minimumSystemVersion": "12.0",
      "hardenedRuntime": true
    }
  }
}
```

The stable bundle identifier is `com.minetrace.desktop`; changing it also changes application data locations and signing identity. Keep `signingIdentity` and notarization credentials out of committed configuration. A direct-distribution build needs no filesystem entitlement, and JIT, unsigned-executable-memory, library-validation, or temporary filesystem entitlements must not be added speculatively.

The built app must be usable when launched from `/Applications`, and it must never write into `MineTrace.app/Contents/Resources`. A user may launch an app directly from a DMG, so all mutable storage must resolve through Tauri's application directories.

## Build prerequisites

Install frontend dependencies and both macOS Rust targets through one Rust 1.97+ toolchain. Rustup is the simplest way to keep both targets on the same active toolchain:

```bash
pnpm install --frozen-lockfile
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

The release helper deliberately does not install or update tools. It checks for:

- macOS and Xcode command-line tools
- `pnpm` and the project-local Tauri CLI
- an active Rust 1.97+ `cargo`/`rustc` pair
- the Rust targets required by the selected build target
- `codesign`, `security`, `lipo`, `hdiutil`, `spctl`, `notarytool`, and `stapler` as applicable
- the shared and macOS Tauri configuration files

Run a local universal build with ad-hoc signing:

```bash
MINETRACE_MAC_BUILD_MODE=local ./scripts/build-macos.sh
```

For a quicker native Apple Silicon iteration:

```bash
MINETRACE_MAC_BUILD_MODE=local \
MINETRACE_MAC_TARGET=aarch64-apple-darwin \
./scripts/build-macos.sh
```

The helper supports these non-secret controls:

| Variable | Values | Default |
| --- | --- | --- |
| `MINETRACE_MAC_BUILD_MODE` | `local`, `release` | `local` |
| `MINETRACE_MAC_TARGET` | `universal-apple-darwin`, `aarch64-apple-darwin`, `x86_64-apple-darwin` | `universal-apple-darwin` |
| `MINETRACE_SKIP_STAPLING` | `0`, `1` | `0` |

Local mode always uses the ad-hoc identity `-` and never claims to be a distributable release.

## Release signing and notarization

A public direct-download build requires a paid Developer ID Application certificate in the build keychain. Inject its identity as `APPLE_SIGNING_IDENTITY`. Do not use an Apple Development or Apple Distribution identity for the direct DMG.

Provide one notarization credential set through the environment:

- App Store Connect API: `APPLE_API_ISSUER`, `APPLE_API_KEY`, and `APPLE_API_KEY_PATH`; or
- Apple ID: `APPLE_ID`, `APPLE_PASSWORD`, and `APPLE_TEAM_ID`.

The build script checks only whether the required variables exist. It never echoes their values, and it passes no credential on the command line. Prefer CI secret injection or a local secret manager over shell history or committed `.env` files.

With secrets already injected into the environment:

```bash
MINETRACE_MAC_BUILD_MODE=release ./scripts/build-macos.sh
```

Tauri signs the app, submits it for notarization, waits, and staples during the build. To notarize now but defer stapling:

```bash
MINETRACE_MAC_BUILD_MODE=release \
MINETRACE_SKIP_STAPLING=1 \
./scripts/build-macos.sh
```

After a successful notarization, staple and validate both artifacts without printing credentials:

```bash
xcrun stapler staple "$MINETRACE_APP_PATH"
xcrun stapler staple "$MINETRACE_DMG_PATH"
xcrun stapler validate "$MINETRACE_APP_PATH"
xcrun stapler validate "$MINETRACE_DMG_PATH"
```

If a manual submission is required, use one of these credential forms. Keep every value in an environment variable:

```bash
xcrun notarytool submit "$MINETRACE_DMG_PATH" \
  --key "$APPLE_API_KEY_PATH" \
  --key-id "$APPLE_API_KEY" \
  --issuer "$APPLE_API_ISSUER" \
  --wait
```

```bash
xcrun notarytool submit "$MINETRACE_DMG_PATH" \
  --apple-id "$APPLE_ID" \
  --password "$APPLE_PASSWORD" \
  --team-id "$APPLE_TEAM_ID" \
  --wait
```

Never enable shell tracing around signing commands, print the environment, echo credentials, or place credentials directly in command history.

## Artifacts and verification

The preferred public artifact is one universal DMG:

```text
src-tauri/target/universal-apple-darwin/release/bundle/macos/MineTrace.app
src-tauri/target/universal-apple-darwin/release/bundle/dmg/MineTrace_<version>_universal.dmg
```

Tauri controls the exact DMG filename. The build helper locates artifacts created by the current invocation, verifies them, and prints their paths and SHA-256 hashes. It does not delete previous artifacts.

The release checks are:

```bash
lipo -archs "$MINETRACE_APP_PATH/Contents/MacOS/MineTrace"
codesign --verify --deep --strict "$MINETRACE_APP_PATH"
codesign -dvv "$MINETRACE_APP_PATH"
codesign -d --entitlements :- "$MINETRACE_APP_PATH"
hdiutil verify "$MINETRACE_DMG_PATH"
spctl --assess --type execute "$MINETRACE_APP_PATH"
xcrun stapler validate "$MINETRACE_APP_PATH"
xcrun stapler validate "$MINETRACE_DMG_PATH"
shasum -a 256 "$MINETRACE_DMG_PATH"
```

Confirm that:

- a universal binary contains `arm64` and `x86_64`;
- the signature has the runtime flag;
- `com.apple.security.app-sandbox` is absent or false;
- Gatekeeper accepts the release app;
- notarization tickets are stapled to the app and DMG;
- the DMG verifies and mounts;
- the app launches after dragging it to `/Applications`;
- a quarantined, freshly downloaded copy passes first launch;
- Official, Prism, and manual-folder permission-recovery flows work;
- cancellation, incremental rescan, canonical explorer reads, and database reopening work;
- source file hashes and modification times are unchanged by a scan.

Verify that the release contains signed Tauri updater archives and a `latest.json` entry for Apple silicon. The Settings button and the optional launch-time update check must both reject payloads that do not match the public updater key embedded in the app. Updater signing does not replace Developer ID signing or Apple notarization.

## Current Mac capabilities and limitations

Observed on 2026-08-11:

- macOS 26.6.1 on Apple Silicon (`arm64`)
- Xcode 26.6, `codesign`, `notarytool`, and `stapler` available
- Node.js 22.23.1 available in the packaging shell
- pnpm 10.33.0 available
- Rosetta 2 available
- one Apple Development identity available, but no Developer ID Application identity
- the shell currently prefers Homebrew Rust 1.97.1 while rustup manages a separate toolchain
- only `aarch64-apple-darwin` is currently installed in rustup; `x86_64-apple-darwin` is missing
- the project-local Tauri CLI is installed and produced the verified `.app` and `.dmg`

Latest local artifact, built and verified on 2026-08-11:

- `MineTrace.app`: arm64 Mach-O, macOS 12.0 minimum, hardened runtime, ad-hoc signature, bundle identifier `com.minetrace.desktop`
- `MineTrace_0.1.0_aarch64.dmg`: 4,748,546 bytes; `hdiutil verify` passed
- DMG SHA-256: `7b51875105b33c29c34b990f85bdeb12463b4cbdefdd57ab01a93730d74b7653`
- App executable SHA-256: `1bb560dbb3552095147e81b443db3a3d90ff1c55adf6004b05305ed913ff4b5b`
- Packaged command audit includes dashboard, sessions, all four explorer reads, and scan start/status/cancel commands
- Launch smoke test passed and the app exited cleanly without running a scan

This machine can verify native arm64 development, automatic Official/default-layout Prism discovery, manual CurseForge-root approval, parser behavior, read-only invariants, ad-hoc `.app`/DMG creation, and an Intel slice under Rosetta after installing the target.

It cannot currently prove a Gatekeeper-clean public release because the Developer ID Application identity and notarization credentials are missing. It also cannot prove behavior on macOS 12, a clean user's first TCC prompt, Intel hardware, or Windows packaging. Cover those with a Monterey runner/real Mac, a clean macOS account and freshly downloaded artifact, Rosetta plus Intel CI where available, and a Windows CI runner.

## Release gate

A macOS release is ready only when all of the following are true:

- frontend lint, typecheck, tests, and production build pass;
- Rust format, clippy, and tests pass;
- copied/redacted fixtures cover macOS paths with spaces and Unicode;
- the universal app and DMG pass the artifact checks above;
- Developer ID signing, notarization, stapling, and Gatekeeper assessment pass;
- permission denial is recoverable without Full Disk Access;
- no source Minecraft file is changed;
- no private data or credential appears in logs or release artifacts;
- macOS and Windows use the same migrations, parsers, domain types, and analytics rules.
