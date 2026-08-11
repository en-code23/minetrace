<p align="center">
  <img src="website/site/assets/minetrace-icon.png" width="96" height="96" alt="MineTrace icon">
</p>

<h1 align="center">MineTrace</h1>

<p align="center">
  A private, local-first Minecraft Java Edition play-history viewer for macOS and Windows.
</p>

<p align="center">
  <a href="https://en-code23.github.io/minetrace/"><strong>Download MineTrace</strong></a>
  ·
  <a href="https://github.com/en-code23/minetrace/releases">Releases</a>
  ·
  <a href="LICENSE">MIT license</a>
</p>

MineTrace reads existing Minecraft client logs, reconstructs play sessions, and stores the resulting archive in a local SQLite database. It does not modify Minecraft files or upload play data.

## What it shows

- Playtime, sessions, active days, versions, instances, worlds, and multiplayer destinations
- A local player profile with an attributable cached skin, head avatar, and previous cached account skins
- Single-player statistics from player stats files in local saves
- Clients and launchers observed in retained session evidence
- Most-played worlds, locally available saves, missing save links, and ZIP backups
- A selectable profile card exported as a PNG for sharing
- Automatic read-only statistic refreshes, with manual mode available
- Signed in-app updates from GitHub Releases, with manual mode available

Minecraft normally stores in-game statistics inside each local world. Multiplayer servers usually keep those statistics server-side, so MineTrace does not invent unavailable totals. A save marked “not found” may have been deleted, moved, renamed, or placed outside an approved location.

## Downloads

The [download site](https://en-code23.github.io/minetrace/) provides installers for:

| Platform | Architecture | Installer |
| --- | --- | --- |
| macOS 12+ | Apple silicon | DMG |
| Windows 10/11 | x64 | NSIS setup |
| Windows 11 | ARM64 | NSIS setup |

Preview installers are mirrored in [`website/site/downloads`](website/site/downloads). Releases that are not notarized or platform code-signed may trigger an operating-system warning. In-app packages are separately verified with MineTrace’s embedded Tauri updater public key.

## Privacy and safety

- Source logs, saves, statistics, and launcher profiles stay on the device.
- Scanning is read-only and restricted to detected or user-approved locations.
- Account tokens, email addresses, and unrelated cached player skins are never returned to the interface.
- Server addresses can be masked throughout the app.
- Profile sharing creates a local PNG; MineTrace never posts to a social network.
- Parsing, reconstruction, and UI payloads use explicit resource limits for predictable performance.

## Development

Requirements: Node.js 22+, pnpm 10+, Rust 1.97+, and the [Tauri 2 platform prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
pnpm install
pnpm tauri:dev
```

Run the release gates:

```bash
pnpm typecheck
pnpm lint
pnpm test
pnpm build
cd src-tauri
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

The React interface lives in [`src`](src), the Rust/Tauri backend in [`src-tauri`](src-tauri), architecture and release notes in [`docs`](docs), and the GitHub Pages download site in [`website/site`](website/site).

## Signed releases and automatic updates

Pushing a `v*` tag runs [the release workflow](.github/workflows/release.yml) for Apple silicon macOS, Windows x64, and Windows ARM64. The workflow creates updater signatures and `latest.json` for the endpoint embedded in the app.

Repository maintainers must add the updater private key as the Actions secret `TAURI_SIGNING_PRIVATE_KEY`. The matching private key must be backed up securely; it is intentionally ignored by Git and must never be committed.

## License

MineTrace is available under the [MIT License](LICENSE). Copyright © 2026 en-code23.

MineTrace is an independent project and is not affiliated with Mojang Studios or Microsoft.
