# MineTrace Windows release guide

This guide defines the Windows x64 and ARM64 build, installer, signing, filesystem, and verification contract for MineTrace. Windows and macOS use the same React interface, Rust application services, parser rules, SQLite migrations, privacy boundary, and canonical read models. Only the operating-system paths, executable behavior, signing, and installer layers differ.

## Supported release shape

The Windows release targets x64 Windows 10/11 and native ARM64 Windows 11 with the corresponding Tier 1 MSVC Rust targets:

| Release | Rust target | Native payload |
| --- | --- | --- |
| x64 | `x86_64-pc-windows-msvc` | x86-64 PE executable |
| ARM64 | `aarch64-pc-windows-msvc` | ARM64 PE executable |

Each architecture produces:

- `minetrace.exe`, the unpacked desktop executable;
- a per-user NSIS setup executable for normal installation without administrator rights;
- an MSI package for managed deployment and standard Windows Installer tooling.

The ARM64 NSIS setup program itself uses NSIS's x86 bootstrap and runs through Windows-on-ARM emulation, as documented by Tauri. The installed `minetrace.exe` is native ARM64; the verifier checks its PE machine field rather than mistaking the installer stub's architecture for the application architecture.

The committed Windows overlay is `src-tauri/tauri.windows.conf.json`. It keeps downgrades blocked, fixes the WiX upgrade code so updates do not create duplicate products, uses a per-user NSIS install, and silently installs the WebView2 bootstrapper only when the runtime is unavailable. The MineTrace application itself does not require an account or network connection; on a machine without WebView2, the small installer may require a connection to obtain that Microsoft runtime.

Authoritative packaging references:

- [Tauri Windows installer guide](https://v2.tauri.app/distribute/windows-installer/)
- [Tauri Windows code-signing guide](https://v2.tauri.app/distribute/sign/windows/)
- [Rust Windows MSVC target support](https://doc.rust-lang.org/rustc/platform-support/windows-msvc.html)
- [GitHub-hosted Windows ARM64 runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
- [Microsoft WebView2 distribution guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)

## Windows filesystem contract

Automatic discovery is intentionally narrow:

| Launcher | Candidate |
| --- | --- |
| Official Launcher | `%APPDATA%\.minecraft` |
| Official fallback | `%USERPROFILE%\AppData\Roaming\.minecraft` when `%APPDATA%` is unavailable |
| Prism Launcher | `%APPDATA%\PrismLauncher` |

Custom and portable installations are added through the native folder picker. MineTrace validates multiple launcher/game markers before saving a root. It does not search whole drives, inspect Microsoft account tokens, execute launcher content, follow directory junctions or symbolic links, or write to Minecraft files.

Paths remain `PathBuf`/`OsStr` values in Rust. Windows persistence keys encode native UTF-16 so spaces and non-ASCII names round-trip without assuming UTF-8. Inventory rejects reparse-point descendants and revalidates canonical containment before opening a source. Read-only file handles use Windows reparse-point-aware flags.

The SQLite archive is created in Tauri's application-local data directory under the stable identifier `com.minetrace.desktop`. Changing that identifier or the WiX upgrade code would break continuity and is a release migration, not a cosmetic rename.

## Native build prerequisites

Use a matching native Windows host: x64 Windows 10/11 for the x64 smoke test, or ARM64 Windows 11 for the ARM64 smoke test. CI uses `windows-latest` and the `windows-11-arm` GitHub-hosted runner respectively. The ARM runner is currently a GitHub public preview, so its workflow result is retained as explicit release evidence rather than assumed from configuration.

Both hosts require:

- Node.js 22.23.1 or a compatible Node.js 22 release;
- pnpm 10.33.0;
- Rust 1.97+ with `rustfmt`, `clippy`, and the matching Windows target;
- Visual Studio 2022 C++ build tools and a recent Windows SDK;
- the Visual Studio C++ ARM64 build tools when producing ARM64 from a non-ARM developer machine;
- WebView2 for launch testing;
- the Windows VBSCRIPT optional feature when producing MSI packages.

Install the required Rust target if needed:

```powershell
rustup toolchain install 1.97.0 --profile minimal --component clippy,rustfmt --target x86_64-pc-windows-msvc
rustup target add aarch64-pc-windows-msvc --toolchain 1.97.0
rustup default 1.97.0
```

Install JavaScript dependencies without changing the lockfile:

```powershell
pnpm install --frozen-lockfile
```

## Local unsigned build

Run the x64 release helper from a clean PowerShell session:

```powershell
./scripts/build-windows.ps1 -Mode Local
```

Build native ARM64 with:

```powershell
./scripts/build-windows.ps1 -Mode Local -Architecture Arm64
```

This runs frontend lint/typecheck/tests, target-specific Rust formatting/check/tests/clippy, builds both installer formats, verifies the selected PE architecture, records Authenticode status, and writes an architecture-local `SHA256SUMS.txt`. Local mode passes `--no-sign` deliberately and must not be represented as a trusted public download.

An x64 Windows developer machine with the ARM64 MSVC tools can cross-build the ARM64 payload, but it cannot execute ARM64 Rust tests or perform the application smoke test. For that narrow engineering case, use `-SkipTargetTests` and omit `-SmokeInstall`; the native ARM64 CI job remains mandatory before release.

On a disposable test machine or clean CI runner, include the full install lifecycle:

```powershell
./scripts/build-windows.ps1 -Mode Local -SmokeInstall
./scripts/build-windows.ps1 -Mode Local -Architecture Arm64 -SmokeInstall
```

The smoke test refuses to overwrite an existing `%LOCALAPPDATA%\MineTrace` installation. It silently installs the NSIS package, launches the installed app, verifies that it remains live, stops that test process, silently uninstalls it, and checks that the installed executable was removed.

Expected artifact roots:

```text
src-tauri/target/x86_64-pc-windows-msvc/release/minetrace.exe
src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/*-setup.exe
src-tauri/target/x86_64-pc-windows-msvc/release/bundle/msi/*.msi
src-tauri/target/x86_64-pc-windows-msvc/release/bundle/SHA256SUMS.txt

src-tauri/target/aarch64-pc-windows-msvc/release/minetrace.exe
src-tauri/target/aarch64-pc-windows-msvc/release/bundle/nsis/*-setup.exe
src-tauri/target/aarch64-pc-windows-msvc/release/bundle/msi/*.msi
src-tauri/target/aarch64-pc-windows-msvc/release/bundle/SHA256SUMS.txt
```

## Authenticode release signing

A public download should sign the application executable and both installers with a trusted code-signing certificate and a timestamp. Install the certificate in the current Windows user's certificate store without committing the PFX or its password. Then provide only the non-secret selector and certificate-provider timestamp URL to the build process:

```powershell
$env:MINETRACE_WINDOWS_CERTIFICATE_THUMBPRINT = "CERTIFICATE_SHA1_THUMBPRINT"
$env:MINETRACE_WINDOWS_TIMESTAMP_URL = "https://YOUR_PROVIDER_RFC3161_TIMESTAMP"
./scripts/build-windows.ps1 -Mode Release -SmokeInstall
./scripts/build-windows.ps1 -Mode Release -Architecture Arm64 -SmokeInstall
```

Release mode merges an in-memory Tauri signing overlay using SHA-256 and RFC 3161 timestamping. It fails closed if either setting is missing, and artifact verification requires `Get-AuthenticodeSignature` to report `Valid` for the executable, NSIS installer, and MSI. Never echo the PFX password, enable PowerShell tracing around certificate import, store a certificate in the repository, or upload signing material as a build artifact.

For GitHub releases, store the base64-encoded PFX as `WINDOWS_CERTIFICATE` and its password as `WINDOWS_CERTIFICATE_PASSWORD` in repository Actions secrets. The release workflow imports it into the ephemeral runner certificate store, applies and verifies timestamped Authenticode signatures, and emits GitHub build-provenance attestations. `TAURI_SIGNING_PRIVATE_KEY` is separate and mandatory: it signs the Tauri update payload consumed by the in-app updater, while Authenticode establishes the Windows publisher identity.

An unsigned build can run locally, but Windows may show an Unknown Publisher or SmartScreen warning. Do not describe an unsigned artifact as production-signed.

## CI contract

`.github/workflows/windows-release.yml` uses a two-architecture matrix. The x64 job runs on `windows-latest`; the ARM64 job runs natively on `windows-11-arm`. Each installs the matching pinned Rust target, performs all source gates, builds unsigned NSIS/MSI bundles, runs the clean install/launch/uninstall smoke test on the matching CPU architecture, and uploads separate executable, installer, and checksum artifacts for 14 days.

The workflow is a verification pipeline, not a public release publisher. Authenticode release signing should be added only after a certificate and protected release environment exist. The signing job must retain the same verification script and must not expose certificate secrets to pull requests.

## Cross-build evidence from the Mac development host

On 2026-08-11, the shared source passed Windows-targeted Rust check and Clippy with warnings denied, then cross-compiled these unsigned x64 artifacts:

| Artifact | Size | SHA-256 |
| --- | ---: | --- |
| `minetrace.exe` | 12,841,984 bytes | `6ebdacdb8101e08aa15cf32444b39b82999846c71764d1fc7aa0ff44342b2d5d` |
| `MineTrace_0.1.0_x64-setup.exe` | 3,575,067 bytes | `2c7b01ad9521b7088f1dce85165d9d42640f4c3caa18698c76e82da554906e2b` |

The application PE is x86-64 and declares the Windows GUI subsystem, so a release launch does not open a console window. The NSIS setup executable uses NSIS's normal 32-bit bootstrap stub while installing that x64 payload. Static inspection found no known sample/demo record names in either artifact.

The same source also passed ARM64-targeted Rust check and Clippy with warnings denied, then produced:

| Artifact | Size | SHA-256 |
| --- | ---: | --- |
| ARM64 `minetrace.exe` | 11,060,224 bytes | `245bddcfe99419288f36d3d1f3ed1f36b1b84f5e7ad114f730dc76756eb6b775` |
| `MineTrace_0.1.0_arm64-setup.exe` | 3,214,289 bytes | `456c20c14025d5bd11cfe75ebf4b0e7030874eda9fa5a67c688a6d095061573d` |

Static PE inspection identifies the application as AArch64 and the Windows GUI subsystem. The setup file is the expected x86 NSIS bootstrap carrying the native ARM64 payload. Static inspection found no known sample/demo record names in either ARM artifact.

These files are engineering evidence, not public release candidates: they are unsigned, Tauri labels cross-platform bundling experimental, MSI creation was not attempted on macOS, and neither artifact can be launched or smoke-tested on the Mac host. The native Windows workflow remains the release gate.

## Manual release checklist

Before calling either Windows architecture finished:

- source gates pass on its Windows MSVC target with warnings denied;
- the NSIS and MSI installers build on Windows;
- the unpacked and installed application PE machine matches x86-64 or ARM64 as labeled;
- the app launches without a console window;
- Official and default-layout Prism discovery use real Windows fixture trees;
- a manual path with spaces and Unicode validates and scans;
- long paths, mixed path casing, junctions, symlinks, and locked/unreadable logs fail safely;
- cancellation remains responsive during traversal, hashing, plain parsing, and gzip parsing;
- a second scan is idempotent and a rotated/replaced `latest.log` preserves prior history;
- the app, NSIS installer, and MSI have valid timestamped Authenticode signatures for public distribution;
- NSIS install/upgrade/uninstall and MSI install/upgrade/uninstall are tested separately;
- WebView2-present and WebView2-missing machines both follow the documented installer behavior;
- the ARM64 package is installed and launched on native Windows ARM hardware or a native ARM runner, not certified from x64 emulation alone;
- no Minecraft source hash or modification timestamp changes after scanning;
- no sample archive, path, credential, username, or private server data appears in the package.

## Current limitations

MineTrace remains the bounded log-evidence v1 documented in the README. It does not claim world-save/NBT inspection, mod inventory, screenshots, corrections, exports, additional launcher adapters, updater delivery, or Microsoft Store packaging. Prism installations using a custom external `InstanceDir` must currently be added through the folder picker. Those limitations are shared product scope, not Windows-only failures.

The Mac development host can cross-compile and statically inspect x64 and ARM64 executables and NSIS packages, but it cannot prove a Windows installer or Windows runtime. Completion therefore requires the matching native workflow or a real Windows machine to produce and smoke-test the final `.exe`/`.msi` artifacts.
