# MineTrace

MineTrace is a private, local-first Minecraft Java Edition play-history viewer for macOS and Windows. It reads existing client logs, reconstructs play sessions, and stores the resulting archive locally without modifying Minecraft files.

## Download

Download MineTrace for Apple silicon macOS, Windows x64, or Windows ARM64:

**[minetrace download page](https://enverdev.github.io/minetrace/)**

Version 0.1.0 installers are also stored in [`website/site/downloads`](website/site/downloads). These preview builds are not notarized or code-signed, so the operating system may show a security warning.

## Development

Requirements: Node.js 22+, pnpm 10+, Rust 1.97+, and the platform tools required by Tauri 2.

```bash
pnpm install
pnpm tauri:dev
```

Useful checks:

```bash
pnpm typecheck
pnpm lint
pnpm test
pnpm build
```

The React interface lives in `src`, the Rust/Tauri backend in `src-tauri`, and the GitHub Pages download site in `website/site`.

MineTrace is an independent project and is not affiliated with Mojang Studios or Microsoft.
