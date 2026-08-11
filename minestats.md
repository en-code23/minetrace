# Minecraft Analytics Desktop App
## Complete Product Blueprint, Feature List, Architecture, Data Model, Starter Code, Build Plan, and Vibe-Coding Guide

**Working title:** MineTrace  
**Platform target:** Windows desktop application distributed as a `.exe`  
**Primary edition:** Minecraft Java Edition  
**Primary use case:** Personal, local-only analysis of Minecraft data across every launcher and instance on one PC  
**Recommended stack:** Tauri + React + TypeScript + Rust + SQLite  
**Project status:** Product specification and implementation blueprint

---

# 1. Project Summary

MineTrace is a small desktop application that scans Minecraft installations and launcher instances on a Windows PC, reads locally stored logs and game data, reconstructs play sessions, and presents the results in a clean analytics dashboard.

The application should be:

- Local-first
- Offline by default
- Read-only
- Private
- Lightweight
- Fast after the first scan
- Able to detect multiple launchers and Minecraft instances
- Transparent about estimated or incomplete data
- Exportable to JSON and CSV
- Packaged as a normal Windows `.exe`

The program should never require a Minecraft password, Microsoft login, server login, or cloud account.

---

# 2. Core Product Idea

A user opens MineTrace and sees a dashboard such as:

```text
Total estimated playtime: 1,284 h 37 min
First detected session: 2021-09-11
Last detected session: Today at 15:42
Active days: 617
Longest session: 9 h 24 min
Average session: 1 h 31 min
Most played launcher: Prism Launcher
Most played version: 1.20.1
Most visited server: play.example.net
Most played local world: Redstone Lab
```

The program combines information from:

- Minecraft log files
- Compressed historical logs
- Launcher instance folders
- Save folders
- Player statistics files
- Advancement files
- Server lists
- Mod folders
- Resource-pack folders
- Shader-pack folders
- Version metadata
- Crash reports
- Screenshot metadata
- Optional public Minecraft profile APIs

The application must clearly separate:

1. **Verified data**
2. **Calculated data**
3. **Estimated data**
4. **Unavailable data**

---

# 3. Important Accuracy Limitations

MineTrace must not claim that its results are perfect.

Minecraft does not maintain one universal lifetime-playtime database for Java Edition. The application reconstructs activity from files that are still present on the computer.

Results can be incomplete when:

- Old log files were deleted
- The user reinstalled Windows
- The user changed computers
- A launcher rotates or deletes logs
- A game crash prevented a clean shutdown entry
- A launcher stores instances in a custom location
- A server address was hidden or not logged
- A local world was renamed
- Multiple people used the same operating-system account
- The computer clock was incorrect
- The same instance was copied or duplicated
- Sessions overlap because two instances ran simultaneously

The interface should display confidence labels:

```text
Verified
High-confidence estimate
Partial history
Unknown
```

Suggested wording:

> Playtime is reconstructed from local files and may not include sessions whose logs are missing.

---

# 4. Product Goals

## 4.1 Main goals

- Automatically discover Minecraft launchers and instances
- Parse current and archived logs
- Reconstruct sessions
- Calculate playtime by day, week, month, year, launcher, instance, version, server, and world
- Analyze local-world statistics
- Display clean interactive charts
- Work without an online account
- Keep all information on the user's computer
- Build into a small Windows installer or portable executable

## 4.2 Secondary goals

- Detect installed mods
- Track modpack history
- Analyze crashes
- Build a screenshot timeline
- Show server and world activity
- Allow manual folders
- Export all parsed data
- Support incremental rescanning
- Explain where each result came from

## 4.3 Non-goals for version 1

- Editing Minecraft worlds
- Changing account details
- Automating gameplay
- Reading encrypted Microsoft account data
- Bypassing server privacy
- Accessing server-side records without permission
- Uploading private data by default
- Supporting every historical launcher immediately
- Perfectly recovering deleted history
- Acting as an anticheat or surveillance tool

---

# 5. Supported Data Sources

## 5.1 Official Minecraft Launcher

Typical data:

- Main `.minecraft` directory
- Logs
- Versions
- Saves
- Screenshots
- Resource packs
- Shader packs
- Server list
- Options
- Crash reports
- Mod folders when mod loaders use the standard directory

## 5.2 Prism Launcher and MultiMC-style launchers

Typical data:

- Multiple isolated instances
- Per-instance game directories
- Per-instance mods
- Per-instance logs
- Instance names
- Minecraft versions
- Mod-loader details
- Instance icons
- Instance configuration

## 5.3 CurseForge

Typical data:

- Modpack instances
- Instance names
- Mod lists
- Per-instance logs
- Save folders
- Resource packs
- Screenshots
- Minecraft and loader versions

## 5.4 Modrinth App

Typical data:

- Profiles
- Per-profile Minecraft directories
- Installed content
- Logs
- Saves
- Loader versions
- Project metadata when available locally

## 5.5 ATLauncher, GDLauncher, PolyMC, and similar launchers

Support should be implemented through a common launcher-adapter interface.

## 5.6 Lunar Client, Badlion Client, Feather Client, and custom clients

Support may vary.

The application should:

- Detect known folders
- Search for recognizable Minecraft directory structures
- Allow the user to add a custom folder
- Mark uncertain detections as unverified
- Never assume that every client exposes full historical data

---

# 6. Main Feature List

# 6.1 First-Run Setup

The first-run experience should include:

- Welcome screen
- Privacy explanation
- Read-only explanation
- Automatic folder detection
- Manual folder selection
- Scan summary before parsing
- Choice between quick scan and deep scan
- Optional online profile lookup toggle
- Database location selection
- Theme selection
- Start scan button

Example:

```text
We found:

1 Official Minecraft installation
6 Prism Launcher instances
2 CurseForge modpacks
1 custom Minecraft folder

Estimated files to scan: 4,821
Estimated scan time: under 2 minutes
```

---

# 6.2 Automatic Launcher Discovery

The scanner should search:

- Standard Windows application-data directories
- User-selected directories
- Launcher configuration files that point to custom instance locations
- Common portable-launcher folders
- Existing MineTrace saved locations

Each discovered source should have:

- Launcher name
- Launcher type
- Root path
- Instance count
- Last modified time
- Confidence score
- Enabled or disabled state

Example:

```text
Prism Launcher
Path: D:\Games\PrismLauncher
Instances: 8
Confidence: Verified
```

---

# 6.3 Custom Folder Support

The user should be able to add:

- A `.minecraft` folder
- A launcher root folder
- A single instance folder
- A backup directory
- An external drive
- A network drive, with a warning about speed
- A ZIP export in a future version

Custom folders should be removable from MineTrace without deleting the original files.

---

# 6.4 Scan Modes

## Quick scan

Reads:

- Log filenames
- File timestamps
- Instance metadata
- Basic session information
- Existing database state

## Standard scan

Also reads:

- All current and compressed logs
- World metadata
- Stats
- Advancements
- Server lists
- Mod manifests
- Crash reports

## Deep scan

Also reads:

- Screenshot metadata
- Historical duplicates
- Custom launcher folders
- Large world directories
- Advanced integrity checks
- Optional file hashing

---

# 6.5 Incremental Scanning

After the first scan, MineTrace should not parse every file again.

It should store:

- File path
- File size
- Last modified timestamp
- Optional content hash
- Last parsed byte offset for active logs
- Parser version
- Parse status
- Error status

On later launches, it should only process:

- New files
- Changed files
- Appended log content
- Newly discovered instances
- Files affected by a parser update

---

# 6.6 Session Reconstruction

A session represents one Minecraft launch.

Possible session fields:

- Session ID
- Start time
- End time
- Duration
- Launcher
- Instance
- Minecraft version
- Mod loader
- Loader version
- Java version
- Operating system
- Player name when locally visible
- Server or local world
- Clean exit or crash
- Confidence level
- Source log
- Source line range

Session start indicators can include:

- Game bootstrap entries
- Client startup messages
- User login completion
- Main menu initialization
- World or server join events

Session end indicators can include:

- Normal game shutdown
- Client stop messages
- Log file ending
- Crash marker
- Next session start
- File modification time as a fallback

A session should never receive an invented exact duration without being labeled as estimated.

---

# 6.7 Playtime Analytics

Display playtime by:

- Total
- Day
- Week
- Month
- Quarter
- Year
- Weekday
- Hour of day
- Launcher
- Instance
- Minecraft version
- Mod loader
- Server
- Local world
- Singleplayer versus multiplayer
- Modded versus vanilla
- Clean exits versus crashes

Metrics:

- Total estimated playtime
- Number of sessions
- Active days
- Average session duration
- Median session duration
- Longest session
- Shortest meaningful session
- Most active day
- Most active week
- Most active month
- Most active year
- Longest break
- Current activity streak
- Longest activity streak
- Average daily playtime on active days
- Average daily playtime across all calendar days
- Weekend versus weekday activity
- Day versus night activity

---

# 6.8 Dashboard

The home dashboard should contain:

- Total playtime card
- First detected session
- Last detected session
- Active days
- Longest session
- Most played version
- Most used launcher
- Most visited server
- Most played local world
- Activity chart
- Calendar heatmap
- Recent sessions
- Data-quality warning
- Rescan button

Suggested layout:

```text
┌──────────────────────────────────────────────────────────────┐
│ MineTrace                                      Scan complete │
├──────────────┬──────────────┬──────────────┬─────────────────┤
│ 1,284 h      │ 617 days     │ 9 h 24 min   │ 1.20.1          │
│ Total        │ Active days  │ Longest      │ Top version     │
├──────────────────────────────────────────────────────────────┤
│ Monthly activity chart                                       │
├──────────────────────────────┬───────────────────────────────┤
│ Calendar heatmap             │ Recent sessions               │
├──────────────────────────────┴───────────────────────────────┤
│ Data coverage: Partial history since 2021-09-11              │
└──────────────────────────────────────────────────────────────┘
```

---

# 6.9 Calendar Heatmap

A GitHub-style heatmap should show:

- Minutes played per day
- Hover details
- Click to open that day's sessions
- Year selector
- Color-scale legend
- Missing-data indicator
- Option to use sessions or world statistics as the source

Hover example:

```text
August 20, 2022
9 h 24 min
3 sessions
Prism Launcher
1 local world
2 multiplayer servers
```

---

# 6.10 Session Timeline

The session page should support:

- Chronological list
- Search
- Filters
- Sorting
- Grouping by day
- Grouping by instance
- Grouping by server
- Expandable source details
- Confidence labels
- Manual correction
- Ignore session
- Merge sessions
- Split session
- Add note

Example:

```text
August 6, 2026

15:02–16:31
1 h 29 min
Prism Launcher / Redstone
Minecraft 1.21.x / Fabric
Server: openredstone.example
Exit: Clean
Confidence: High
```

---

# 6.11 Instance Explorer

For each instance:

- Instance name
- Launcher
- Folder location
- Minecraft version
- Mod loader
- Loader version
- First detected use
- Last detected use
- Total playtime
- Number of launches
- Installed mods
- Worlds
- Servers
- Screenshots
- Crash count
- Resource packs
- Shader packs
- Java settings when detectable
- JVM arguments when available
- Memory allocation when available
- Instance icon

---

# 6.12 Version Analytics

Show:

- Every detected Minecraft version
- First use
- Last use
- Playtime
- Session count
- Launcher and instance distribution
- Mod-loader distribution
- Upgrade timeline
- Snapshots versus releases
- Modded versus vanilla

Example insight:

> You spent 42% of your detected playtime on Minecraft 1.20.1.

---

# 6.13 Mod Analytics

For each mod:

- Mod ID
- Display name
- Version
- Loader
- Source instance
- First detected
- Last detected
- Number of instances
- Approximate playtime while installed
- File path
- File size
- Optional JAR metadata
- Duplicate versions
- Possible incompatibilities
- Whether the mod is currently present

Important wording:

> Playtime while installed does not prove that a specific mod was actively used during gameplay.

Potential views:

- Most frequently present mods
- Mods by loader
- Mods by Minecraft version
- Mod timeline
- Duplicate mod versions
- Missing dependency hints
- Recently added mods
- Recently removed mods

---

# 6.14 Modpack Analytics

For each modpack or profile:

- Name
- Launcher
- Version
- Mod count
- Total playtime
- First launch
- Last launch
- World count
- Crash count
- Update history when locally detectable
- Exported mod list
- Loader
- Java version
- Memory allocation

---

# 6.15 Multiplayer Server Analytics

Possible server data:

- Server address
- Display name from `servers.dat`
- First detected connection
- Last detected connection
- Session count
- Estimated playtime
- Favorite version
- Favorite instance
- Failed connection count
- Disconnect reasons
- Ping history only when present in logs
- Server icon from local cache when available
- Notes
- Tags
- Hidden-address option

Privacy options:

- Hide server addresses
- Replace addresses with aliases
- Exclude selected servers from exports
- Mask IP addresses
- Clear server history from MineTrace only

The tool must not claim to know the complete server history if old logs are missing.

---

# 6.16 Local World Analytics

For each world:

- World name
- Folder name
- Save path
- Creation estimate
- Last played time
- Game version
- Game mode
- Difficulty
- Hardcore state
- Cheats state when detectable
- Seed when locally available and readable
- World size
- Dimension folders
- Player stats
- Advancements
- Data-pack list
- Icon
- Session count
- Estimated playtime
- Backup status
- Corruption warning
- Last scan time

Important safety rule:

- World files must be opened read-only.
- The program must never save modified NBT data back to the world.

---

# 6.17 Player Statistics

Minecraft world statistics can include categories such as:

- Blocks mined
- Items crafted
- Items used
- Items broken
- Items picked up
- Items dropped
- Mobs killed
- Player deaths
- Damage dealt
- Damage taken
- Distance walked
- Distance sprinted
- Distance crouched
- Distance flown
- Distance by boat
- Distance by minecart
- Jumps
- Time played
- Time since death
- Animals bred
- Fish caught
- Villager interactions
- Raids
- Bells rung
- Containers opened
- Sleep count
- Sneak time
- Swimming distance
- Elytra distance

Views:

- Lifetime totals per world
- Compare worlds
- Compare instances
- Progress over time when multiple snapshots exist
- Top blocks mined
- Top items crafted
- Top mobs killed

Caution:

- Statistics are usually world-specific.
- Multiplayer statistics may be stored server-side and may not be available locally.
- Local stats can reset or vary across versions.

---

# 6.18 Advancement Analytics

Show:

- Completed advancements
- Completion dates when stored
- Progress toward incomplete advancements
- Completion percentage
- Rare or notable advancements
- Advancement timeline
- Comparison across worlds

---

# 6.19 Skin and Profile Page

Possible features:

- Current Minecraft username
- UUID
- Current skin
- Current cape when available
- Locally cached skins
- Optional online profile lookup
- Profile render
- Account aliases entered manually

Privacy behavior:

- Online lookup must be opt-in.
- The application must explain what request is sent.
- The app must continue to work fully offline without profile lookup.

Historical skins should only be shown when:

- They exist in local cache, or
- The user explicitly enables a third-party lookup

Do not imply that historical skin information is guaranteed.

---

# 6.20 Screenshot Timeline

Features:

- Scan screenshot folders
- Sort by date
- Group by instance
- Thumbnail grid
- Fullscreen viewer
- Search by filename
- Favorites
- Tags
- Notes
- Open original file
- Reveal in Explorer
- Duplicate detection
- Screenshot count by month
- Screenshot activity compared with play sessions

Optional advanced feature:

- Local image analysis
- OCR
- Automatic scene tagging

These should be disabled by default because they add complexity and processing time.

---

# 6.21 Crash Analytics

Parse crash reports and relevant log errors.

Show:

- Total crashes
- Crashes by instance
- Crashes by version
- Crashes by mod loader
- Most common exception
- Most common suspected mod
- Crash timeline
- Crash frequency per 100 launches
- Recently introduced crash pattern
- Full report viewer
- Copy diagnostic summary
- Open source file

The application should distinguish between:

- Confirmed crash
- Forced close
- Unclean shutdown
- Connection failure
- Ordinary warning
- Mod-loader error
- Java runtime error

It should avoid blaming a mod unless the crash report explicitly supports that conclusion.

---

# 6.22 Resource Packs and Shader Packs

For each pack:

- Name
- File path
- File type
- Pack format
- File size
- First detected
- Last detected
- Instances
- Enabled state when available
- Approximate playtime while enabled
- Thumbnail or pack icon when available

---

# 6.23 Server List Reader

Read `servers.dat` in read-only mode.

Show:

- Saved server name
- Address
- Icon
- Last local modification
- Instance
- Whether the server also appears in logs
- Whether the server is currently saved but never detected in a session

The UI must explain that a saved server is not proof that the user joined it.

---

# 6.24 Search

Global search should find:

- Instances
- Worlds
- Servers
- Versions
- Mods
- Sessions
- Crash reports
- Screenshots
- Resource packs
- Shader packs
- File paths

Search examples:

```text
1.20.1
Fabric
OpenRedstone
Litematica
August 2022
crash
hardcore
```

---

# 6.25 Filters

Common filters:

- Date range
- Launcher
- Instance
- Minecraft version
- Mod loader
- Server
- World
- Session type
- Confidence
- Clean or crashed exit
- Modded or vanilla
- Weekday
- Time of day

Filters should be shareable internally through URL-like route state, even in a desktop application.

---

# 6.26 Personal Insights

Examples:

- You played most often on Saturdays.
- Your longest detected break was 214 days.
- Your average session became shorter this year.
- 63% of detected playtime was multiplayer.
- You used Fabric in 420 detected hours.
- Your most active month was August 2022.
- You launched one instance 312 times.
- Your longest singleplayer session lasted 7 h 12 min.
- You changed your main Minecraft version five times.
- Your crash rate dropped after a modpack update.

Insights should be generated from deterministic rules first. AI-written summaries can be an optional later feature.

---

# 6.27 Notes and Manual Corrections

Users should be able to:

- Rename an instance inside MineTrace
- Assign aliases to servers
- Add notes to sessions
- Mark a session as ignored
- Correct a start or end time
- Merge duplicate sessions
- Split one session
- Assign a session to a world or server
- Mark a folder as archived
- Hide sensitive entries
- Restore ignored entries

Corrections should be stored separately from source data.

Never modify the original Minecraft files.

---

# 6.28 Data Provenance

Every important data point should have a "Why am I seeing this?" option.

Example:

```text
Session start:
Detected from line 1,842 in latest.log

Session end:
Estimated from the log file modification time

Version:
Read from launcher metadata

Server:
Detected from a connection entry
```

This makes the program trustworthy and debuggable.

---

# 6.29 Export

Supported export formats:

- JSON
- CSV
- Markdown report
- HTML report
- PNG chart
- PDF report in a later version
- Full MineTrace backup

Export categories:

- Sessions
- Daily playtime
- Servers
- Worlds
- Mods
- Crashes
- Versions
- Statistics
- Advancements
- Scan errors
- Data-quality report

Privacy options:

- Remove file paths
- Hide server addresses
- Hide usernames
- Round timestamps
- Exclude notes
- Exclude screenshots
- Export only selected date range

---

# 6.30 Backup and Restore

MineTrace should support:

- Export database backup
- Restore database backup
- Move database location
- Rebuild database from source files
- Reset all application data
- Keep manual corrections during rescan
- Database migration between app versions

---

# 6.31 Settings

Settings categories:

## General

- Theme
- Language
- Start on Windows
- Minimize to tray
- Date format
- Time format
- First day of week
- Units
- Default page

## Scanning

- Automatic scan on startup
- Watch folders for changes
- Quick or standard scan
- Follow symbolic links
- Maximum folder depth
- Include archived instances
- Include screenshots
- Include crash reports
- Include custom folders
- File-size limits

## Privacy

- Offline mode
- Optional online profile lookup
- Hide server addresses
- Mask usernames
- Disable analytics
- Clear cache
- Clear database
- Open database folder

## Performance

- Scanner thread count
- Thumbnail cache size
- Database cache size
- Hardware acceleration
- Background scanning
- Battery-saving mode

## Advanced

- Parser debug logging
- View raw database
- Reparse selected files
- Reset launcher detection
- Export diagnostic bundle
- Developer tools

---

# 7. User Experience Flow

## 7.1 First launch

1. Start app
2. Read privacy explanation
3. Scan for launcher roots
4. Review found locations
5. Add custom locations if needed
6. Choose scan mode
7. Start scan
8. Watch progress
9. Review warnings
10. Open dashboard

## 7.2 Later launches

1. Start app
2. Run incremental scan
3. Import new sessions
4. Update charts
5. Show "new since last scan"
6. Open dashboard

## 7.3 Error flow

When a folder cannot be read:

```text
CurseForge instance could not be scanned.

Reason:
Access denied

Path:
D:\Minecraft\Instances\Example

Actions:
Retry
Choose another folder
Ignore
Open help
```

---

# 8. Recommended Technology Stack

# 8.1 Desktop shell

**Tauri**

Reasons:

- Small binary compared with many Electron applications
- Native Windows packaging
- Rust backend
- Web-based frontend
- Good file-system integration
- Suitable for a local-only desktop application

Alternative:

- Electron for easier all-TypeScript development
- Flutter for one UI toolkit
- .NET with WinUI or Avalonia for a Windows-focused implementation

Recommended choice for this project:

```text
Tauri + React + TypeScript + Rust
```

---

# 8.2 Frontend

- React
- TypeScript
- Vite
- React Router
- TanStack Query
- Zustand or Redux Toolkit
- ECharts or Recharts
- Tailwind CSS or CSS Modules
- Radix UI or shadcn-style components
- Virtualized lists for large session tables

---

# 8.3 Backend

- Rust
- Tauri commands
- Tokio for asynchronous tasks
- Rayon for CPU-parallel parsing where useful
- Serde for serialization
- rusqlite or SQLx for SQLite
- flate2 for `.gz` logs
- walkdir for directory traversal
- regex for log patterns
- chrono for date and time
- uuid for internal identifiers
- notify for file watching
- sha2 or blake3 for optional hashes
- fastnbt or an equivalent NBT library for read-only NBT parsing

---

# 8.4 Database

**SQLite**

Reasons:

- Local
- Fast
- Portable
- No database server
- Easy backup
- Good for analytics queries
- Works well with incremental imports

Recommended modes:

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA synchronous = NORMAL;
```

---

# 8.5 Packaging

- Tauri Windows installer
- MSI or NSIS
- Optional portable build
- Code signing in a later release
- Automatic updates only when the user enables them

---

# 9. High-Level Architecture

```text
┌─────────────────────────────────────────────────────────────┐
│ React UI                                                    │
│ Dashboard, Sessions, Worlds, Servers, Mods, Settings        │
└──────────────────────────────┬──────────────────────────────┘
                               │ Tauri commands/events
┌──────────────────────────────▼──────────────────────────────┐
│ Application Service Layer                                  │
│ Scan orchestration, queries, exports, corrections           │
└───────────────┬─────────────────────┬───────────────────────┘
                │                     │
┌───────────────▼────────────┐  ┌─────▼──────────────────────┐
│ Scanner and Parsers         │  │ SQLite Repository          │
│ Launchers, logs, NBT, stats │  │ Sessions, files, metadata  │
└───────────────┬────────────┘  └─────────────────────────────┘
                │
┌───────────────▼────────────────────────────────────────────┐
│ Local Minecraft files                                      │
│ Logs, instances, saves, stats, screenshots, crash reports  │
└─────────────────────────────────────────────────────────────┘
```

---

# 10. Suggested Project Structure

```text
minetrace/
├─ README.md
├─ LICENSE
├─ package.json
├─ pnpm-lock.yaml
├─ vite.config.ts
├─ tsconfig.json
├─ index.html
├─ src/
│  ├─ main.tsx
│  ├─ App.tsx
│  ├─ routes/
│  │  ├─ DashboardPage.tsx
│  │  ├─ SessionsPage.tsx
│  │  ├─ InstancesPage.tsx
│  │  ├─ WorldsPage.tsx
│  │  ├─ ServersPage.tsx
│  │  ├─ ModsPage.tsx
│  │  ├─ VersionsPage.tsx
│  │  ├─ CrashesPage.tsx
│  │  ├─ ScreenshotsPage.tsx
│  │  └─ SettingsPage.tsx
│  ├─ components/
│  │  ├─ layout/
│  │  ├─ charts/
│  │  ├─ cards/
│  │  ├─ tables/
│  │  ├─ filters/
│  │  └─ dialogs/
│  ├─ hooks/
│  ├─ lib/
│  │  ├─ api.ts
│  │  ├─ dates.ts
│  │  ├─ format.ts
│  │  └─ validation.ts
│  ├─ stores/
│  ├─ types/
│  └─ styles/
├─ src-tauri/
│  ├─ Cargo.toml
│  ├─ tauri.conf.json
│  ├─ capabilities/
│  ├─ icons/
│  └─ src/
│     ├─ main.rs
│     ├─ lib.rs
│     ├─ commands/
│     │  ├─ scan.rs
│     │  ├─ dashboard.rs
│     │  ├─ sessions.rs
│     │  ├─ instances.rs
│     │  ├─ worlds.rs
│     │  ├─ servers.rs
│     │  ├─ export.rs
│     │  └─ settings.rs
│     ├─ discovery/
│     │  ├─ mod.rs
│     │  ├─ official.rs
│     │  ├─ prism.rs
│     │  ├─ curseforge.rs
│     │  ├─ modrinth.rs
│     │  └─ generic.rs
│     ├─ scanner/
│     │  ├─ mod.rs
│     │  ├─ walker.rs
│     │  ├─ fingerprint.rs
│     │  └─ progress.rs
│     ├─ parsers/
│     │  ├─ mod.rs
│     │  ├─ logs.rs
│     │  ├─ sessions.rs
│     │  ├─ servers_dat.rs
│     │  ├─ level_dat.rs
│     │  ├─ stats.rs
│     │  ├─ advancements.rs
│     │  ├─ mods.rs
│     │  ├─ crashes.rs
│     │  └─ screenshots.rs
│     ├─ db/
│     │  ├─ mod.rs
│     │  ├─ migrations.rs
│     │  ├─ models.rs
│     │  ├─ repository.rs
│     │  └─ queries.rs
│     ├─ services/
│     │  ├─ scan_service.rs
│     │  ├─ analytics_service.rs
│     │  ├─ correction_service.rs
│     │  └─ export_service.rs
│     ├─ domain/
│     │  ├─ launcher.rs
│     │  ├─ instance.rs
│     │  ├─ session.rs
│     │  ├─ world.rs
│     │  └─ confidence.rs
│     ├─ errors.rs
│     └─ settings.rs
├─ migrations/
│  ├─ 0001_initial.sql
│  ├─ 0002_manual_corrections.sql
│  └─ 0003_file_offsets.sql
├─ fixtures/
│  ├─ logs/
│  ├─ crash-reports/
│  ├─ stats/
│  └─ launcher-metadata/
├─ scripts/
│  ├─ build.ps1
│  ├─ test.ps1
│  └─ create-fixtures.ps1
└─ docs/
   ├─ architecture.md
   ├─ data-sources.md
   ├─ privacy.md
   └─ parser-rules.md
```

---

# 11. Domain Model

## 11.1 Launcher

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Launcher {
    pub id: String,
    pub kind: LauncherKind,
    pub display_name: String,
    pub root_path: String,
    pub confidence: Confidence,
    pub enabled: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum LauncherKind {
    Official,
    Prism,
    MultiMc,
    CurseForge,
    Modrinth,
    ATLauncher,
    GDLauncher,
    Lunar,
    Badlion,
    Feather,
    Generic,
}
```

## 11.2 Instance

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Instance {
    pub id: String,
    pub launcher_id: String,
    pub name: String,
    pub game_directory: String,
    pub minecraft_version: Option<String>,
    pub loader: Option<String>,
    pub loader_version: Option<String>,
    pub icon_path: Option<String>,
    pub first_seen_at: Option<String>,
    pub last_seen_at: Option<String>,
}
```

## 11.3 Session

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub id: String,
    pub instance_id: String,
    pub source_file_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: Option<i64>,
    pub minecraft_version: Option<String>,
    pub loader: Option<String>,
    pub server_address: Option<String>,
    pub world_id: Option<String>,
    pub exit_kind: ExitKind,
    pub confidence: Confidence,
    pub start_source: String,
    pub end_source: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ExitKind {
    Clean,
    Crash,
    Forced,
    Unknown,
}
```

## 11.4 Confidence

```rust
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum Confidence {
    Verified,
    High,
    Medium,
    Low,
    Unknown,
}
```

---

# 12. Database Schema

A simplified initial schema:

```sql
CREATE TABLE launchers (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    display_name TEXT NOT NULL,
    root_path TEXT NOT NULL UNIQUE,
    confidence TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE instances (
    id TEXT PRIMARY KEY,
    launcher_id TEXT NOT NULL,
    name TEXT NOT NULL,
    game_directory TEXT NOT NULL UNIQUE,
    minecraft_version TEXT,
    loader TEXT,
    loader_version TEXT,
    icon_path TEXT,
    first_seen_at TEXT,
    last_seen_at TEXT,
    FOREIGN KEY (launcher_id) REFERENCES launchers(id)
);

CREATE TABLE source_files (
    id TEXT PRIMARY KEY,
    instance_id TEXT,
    path TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    modified_at TEXT NOT NULL,
    content_hash TEXT,
    last_parsed_offset INTEGER NOT NULL DEFAULT 0,
    parser_version INTEGER NOT NULL DEFAULT 1,
    parse_status TEXT NOT NULL,
    last_error TEXT,
    FOREIGN KEY (instance_id) REFERENCES instances(id)
);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    source_file_id TEXT NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    duration_seconds INTEGER,
    minecraft_version TEXT,
    loader TEXT,
    server_address TEXT,
    world_id TEXT,
    exit_kind TEXT NOT NULL,
    confidence TEXT NOT NULL,
    start_source TEXT NOT NULL,
    end_source TEXT,
    ignored INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (instance_id) REFERENCES instances(id),
    FOREIGN KEY (source_file_id) REFERENCES source_files(id)
);

CREATE INDEX idx_sessions_started_at ON sessions(started_at);
CREATE INDEX idx_sessions_instance_id ON sessions(instance_id);
CREATE INDEX idx_sessions_server_address ON sessions(server_address);

CREATE TABLE worlds (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    folder_path TEXT NOT NULL UNIQUE,
    folder_name TEXT NOT NULL,
    display_name TEXT,
    game_version TEXT,
    game_mode TEXT,
    difficulty TEXT,
    hardcore INTEGER,
    cheats_enabled INTEGER,
    seed TEXT,
    size_bytes INTEGER,
    created_at_estimate TEXT,
    last_played_at TEXT,
    confidence TEXT NOT NULL,
    FOREIGN KEY (instance_id) REFERENCES instances(id)
);

CREATE TABLE servers (
    id TEXT PRIMARY KEY,
    canonical_address TEXT NOT NULL UNIQUE,
    display_name TEXT,
    first_seen_at TEXT,
    last_seen_at TEXT,
    hidden INTEGER NOT NULL DEFAULT 0,
    alias TEXT
);

CREATE TABLE session_servers (
    session_id TEXT NOT NULL,
    server_id TEXT NOT NULL,
    PRIMARY KEY (session_id, server_id),
    FOREIGN KEY (session_id) REFERENCES sessions(id),
    FOREIGN KEY (server_id) REFERENCES servers(id)
);

CREATE TABLE mods (
    id TEXT PRIMARY KEY,
    mod_id TEXT,
    display_name TEXT,
    version TEXT,
    loader TEXT,
    jar_path TEXT NOT NULL,
    file_size INTEGER,
    file_hash TEXT,
    metadata_json TEXT
);

CREATE TABLE instance_mods (
    instance_id TEXT NOT NULL,
    mod_id TEXT NOT NULL,
    first_seen_at TEXT,
    last_seen_at TEXT,
    currently_present INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (instance_id, mod_id),
    FOREIGN KEY (instance_id) REFERENCES instances(id),
    FOREIGN KEY (mod_id) REFERENCES mods(id)
);

CREATE TABLE scan_runs (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    mode TEXT NOT NULL,
    files_discovered INTEGER NOT NULL DEFAULT 0,
    files_parsed INTEGER NOT NULL DEFAULT 0,
    warnings INTEGER NOT NULL DEFAULT 0,
    errors INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE manual_corrections (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    field_name TEXT NOT NULL,
    original_value TEXT,
    corrected_value TEXT,
    reason TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL
);
```

---

# 13. Launcher Adapter Interface

Each launcher should implement the same interface.

```rust
use std::path::{Path, PathBuf};

pub trait LauncherAdapter: Send + Sync {
    fn kind(&self) -> &'static str;

    fn detect_roots(&self) -> anyhow::Result<Vec<DetectedRoot>>;

    fn discover_instances(
        &self,
        root: &Path,
    ) -> anyhow::Result<Vec<DiscoveredInstance>>;
}

#[derive(Debug, Clone)]
pub struct DetectedRoot {
    pub path: PathBuf,
    pub confidence: u8,
}

#[derive(Debug, Clone)]
pub struct DiscoveredInstance {
    pub name: String,
    pub game_directory: PathBuf,
    pub minecraft_version: Option<String>,
    pub loader: Option<String>,
    pub loader_version: Option<String>,
}
```

Generic discovery should check for directory markers such as:

```text
logs/
saves/
versions/
options.txt
servers.dat
mods/
resourcepacks/
shaderpacks/
crash-reports/
```

A folder should not be accepted solely because one marker exists.

---

# 14. Basic Windows Path Discovery

Starter helper:

```rust
use std::path::PathBuf;

pub fn official_minecraft_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(appdata) = std::env::var("APPDATA") {
        paths.push(PathBuf::from(appdata).join(".minecraft"));
    }

    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        paths.push(
            PathBuf::from(&user_profile)
                .join("AppData")
                .join("Roaming")
                .join(".minecraft"),
        );
    }

    paths
        .into_iter()
        .filter(|path| path.exists())
        .collect()
}
```

Do not hard-code only one location. Launchers often support custom paths.

---

# 15. File Discovery

```rust
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub enum SourceFileKind {
    Log,
    CompressedLog,
    CrashReport,
    Stats,
    Advancements,
    LevelDat,
    ServersDat,
    Screenshot,
    ModJar,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct SourceFileCandidate {
    pub path: PathBuf,
    pub kind: SourceFileKind,
}

pub fn classify_path(path: &Path) -> SourceFileKind {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if extension == "log" {
        return SourceFileKind::Log;
    }

    if extension == "gz" && file_name.contains(".log") {
        return SourceFileKind::CompressedLog;
    }

    if extension == "jar" {
        return SourceFileKind::ModJar;
    }

    if file_name == "level.dat" {
        return SourceFileKind::LevelDat;
    }

    if file_name == "servers.dat" {
        return SourceFileKind::ServersDat;
    }

    if extension == "json" && path.to_string_lossy().contains(r"\stats\") {
        return SourceFileKind::Stats;
    }

    if extension == "json" && path.to_string_lossy().contains(r"\advancements\") {
        return SourceFileKind::Advancements;
    }

    if path.to_string_lossy().contains(r"\crash-reports\") && extension == "txt" {
        return SourceFileKind::CrashReport;
    }

    if matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp")
        && path.to_string_lossy().contains(r"\screenshots\")
    {
        return SourceFileKind::Screenshot;
    }

    SourceFileKind::Unknown
}

pub fn discover_files(root: &Path, max_depth: usize) -> Vec<SourceFileCandidate> {
    WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            let path = entry.into_path();
            let kind = classify_path(&path);
            SourceFileCandidate { path, kind }
        })
        .filter(|item| !matches!(item.kind, SourceFileKind::Unknown))
        .collect()
}
```

---

# 16. Reading Plain and Compressed Logs

```rust
use flate2::read::GzDecoder;
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read},
    path::Path,
};

pub fn open_text_reader(path: &Path) -> io::Result<Box<dyn BufRead>> {
    let file = File::open(path)?;

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("gz") => {
            let decoder = GzDecoder::new(file);
            Ok(Box::new(BufReader::new(decoder)))
        }
        _ => Ok(Box::new(BufReader::new(file))),
    }
}

pub fn read_log_lines(path: &Path) -> io::Result<Vec<String>> {
    let reader = open_text_reader(path)?;
    reader.lines().collect()
}
```

For very large active logs, use streaming rather than loading the complete file into memory.

---

# 17. Log Event Model

```rust
#[derive(Debug, Clone)]
pub enum LogEvent {
    GameStart {
        timestamp: Option<String>,
    },
    LoginSuccess {
        timestamp: Option<String>,
        username: Option<String>,
    },
    MinecraftVersion {
        timestamp: Option<String>,
        version: String,
    },
    JoinServer {
        timestamp: Option<String>,
        address: String,
    },
    JoinWorld {
        timestamp: Option<String>,
        world_hint: Option<String>,
    },
    Disconnect {
        timestamp: Option<String>,
        reason: Option<String>,
    },
    CleanShutdown {
        timestamp: Option<String>,
    },
    Crash {
        timestamp: Option<String>,
        summary: Option<String>,
    },
    Unknown,
}
```

---

# 18. Timestamp Parsing

Minecraft logs often begin lines with a time such as:

```text
[16:23:11]
```

Archived log filenames can provide the date.

A robust parser should:

1. Extract a date from the filename when possible
2. Extract a time from each line
3. Combine them in the local timezone
4. Detect midnight rollovers
5. Fall back to file timestamps
6. Record the source of every timestamp

Starter code:

```rust
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

pub fn combine_log_date_and_time(
    date: NaiveDate,
    time_text: &str,
) -> Option<NaiveDateTime> {
    let time = NaiveTime::parse_from_str(time_text, "%H:%M:%S").ok()?;
    Some(date.and_time(time))
}
```

---

# 19. Session Reconstruction Algorithm

Pseudo-code:

```text
for each log file in chronological order:
    determine likely log date
    parse lines into events

    when GameStart appears:
        if another open session exists:
            close previous session as estimated
        create new session

    when MinecraftVersion appears:
        attach version to current session

    when JoinServer appears:
        attach server to current session

    when JoinWorld appears:
        attach local-world hint to current session

    when Crash appears:
        close current session with crash exit

    when CleanShutdown appears:
        close current session with clean exit

    at end of file:
        if session is still open:
            use file modification time as an estimated end
            or keep the end unknown

merge obvious duplicate sessions from copied logs
flag overlapping sessions
calculate confidence
store source references
```

Confidence example:

```text
Verified:
Start and clean end both contain timestamps.

High:
Start timestamp plus reliable file-end timestamp.

Medium:
Start inferred from first relevant log entry and end inferred from file modification time.

Low:
Only a partial log exists.

Unknown:
The parser cannot determine a duration.
```

---

# 20. Starter Session Builder

```rust
use chrono::{DateTime, Local};

#[derive(Debug, Default)]
pub struct SessionBuilder {
    pub started_at: Option<DateTime<Local>>,
    pub ended_at: Option<DateTime<Local>>,
    pub version: Option<String>,
    pub server_address: Option<String>,
    pub crashed: bool,
    pub clean_exit: bool,
}

impl SessionBuilder {
    pub fn is_open(&self) -> bool {
        self.started_at.is_some() && self.ended_at.is_none()
    }

    pub fn finish(
        mut self,
        end: DateTime<Local>,
        clean: bool,
        crashed: bool,
    ) -> CompletedSession {
        self.ended_at = Some(end);
        self.clean_exit = clean;
        self.crashed = crashed;

        let duration_seconds = match (self.started_at, self.ended_at) {
            (Some(start), Some(end)) if end >= start => {
                Some((end - start).num_seconds())
            }
            _ => None,
        };

        CompletedSession {
            started_at: self.started_at,
            ended_at: self.ended_at,
            duration_seconds,
            version: self.version,
            server_address: self.server_address,
            clean_exit: self.clean_exit,
            crashed: self.crashed,
        }
    }
}

#[derive(Debug)]
pub struct CompletedSession {
    pub started_at: Option<DateTime<Local>>,
    pub ended_at: Option<DateTime<Local>>,
    pub duration_seconds: Option<i64>,
    pub version: Option<String>,
    pub server_address: Option<String>,
    pub clean_exit: bool,
    pub crashed: bool,
}
```

---

# 21. Server Address Normalization

Normalize server addresses for grouping, while preserving the original string.

Rules:

- Convert hostnames to lowercase
- Remove trailing dots
- Separate port
- Use default Minecraft port only for grouping when appropriate
- Preserve IPv6 correctly
- Never perform DNS lookups without user permission
- Never store resolved IP addresses unless explicitly needed

Example:

```rust
pub fn normalize_server_address(input: &str) -> String {
    input
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase()
}
```

A production implementation should use a real host-and-port parser.

---

# 22. World Discovery

Basic world detection:

```rust
use std::path::{Path, PathBuf};

pub fn discover_worlds(game_directory: &Path) -> Vec<PathBuf> {
    let saves = game_directory.join("saves");

    let Ok(entries) = std::fs::read_dir(saves) else {
        return Vec::new();
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| path.join("level.dat").exists())
        .collect()
}
```

Read `level.dat` using a read-only NBT parser.

Never write the decoded structure back to disk.

---

# 23. Statistics Parsing

Statistics are commonly JSON files.

Generic starter parser:

```rust
use serde_json::Value;
use std::{fs, path::Path};

pub fn read_stats_file(path: &Path) -> anyhow::Result<Value> {
    let text = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&text)?;
    Ok(value)
}
```

Normalize raw namespaced keys into display names:

```text
minecraft:stone -> Stone
minecraft:walk_one_cm -> Walked distance
minecraft:play_time -> Play time
```

Keep the raw key in the database.

---

# 24. Mod JAR Metadata Parsing

A mod scanner can inspect ZIP/JAR entries for metadata files such as:

```text
fabric.mod.json
META-INF/mods.toml
META-INF/neoforge.mods.toml
mcmod.info
```

Pseudo-code:

```text
open JAR as ZIP
look for known metadata files
parse metadata
extract:
    mod ID
    name
    version
    loader
    authors
    dependencies
    description
    icon path
fall back to filename when metadata is missing
```

Do not execute JAR files.

---

# 25. Crash Report Parsing

Extract:

- Timestamp
- Minecraft version
- Java version
- Operating system
- Exception type
- Exception message
- Stack trace
- Suspected mods section when present
- Loaded mod list
- Memory details
- Graphics details
- Crash source file

Use defensive parsing because formats vary.

---

# 26. Scan Progress Events

Rust backend:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanProgress {
    pub phase: String,
    pub current: u64,
    pub total: u64,
    pub current_path: Option<String>,
    pub warnings: u64,
    pub errors: u64,
}
```

Emit progress to the frontend:

```rust
use tauri::{AppHandle, Emitter};

pub fn emit_scan_progress(
    app: &AppHandle,
    progress: ScanProgress,
) -> tauri::Result<()> {
    app.emit("scan-progress", progress)
}
```

Frontend listener:

```ts
import { listen } from "@tauri-apps/api/event";

type ScanProgress = {
  phase: string;
  current: number;
  total: number;
  current_path?: string;
  warnings: number;
  errors: number;
};

export async function subscribeToScanProgress(
  onProgress: (progress: ScanProgress) => void,
) {
  return listen<ScanProgress>("scan-progress", (event) => {
    onProgress(event.payload);
  });
}
```

---

# 27. Tauri Commands

Rust:

```rust
#[tauri::command]
pub async fn discover_installations() -> Result<Vec<Launcher>, String> {
    discovery_service()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn start_scan(
    app: tauri::AppHandle,
    request: ScanRequest,
) -> Result<ScanSummary, String> {
    scan_service(app, request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_dashboard(
    range: DateRange,
) -> Result<DashboardData, String> {
    dashboard_service(range)
        .await
        .map_err(|error| error.to_string())
}
```

Register commands:

```rust
tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![
        discover_installations,
        start_scan,
        get_dashboard,
    ])
    .run(tauri::generate_context!())
    .expect("failed to run MineTrace");
```

Frontend wrapper:

```ts
import { invoke } from "@tauri-apps/api/core";

export type DateRange = {
  from: string;
  to: string;
};

export async function getDashboard(range: DateRange) {
  return invoke("get_dashboard", { range });
}
```

---

# 28. Dashboard API Shape

```ts
export type DashboardData = {
  coverage: {
    firstDetectedAt: string | null;
    lastDetectedAt: string | null;
    quality: "verified" | "partial" | "limited" | "unknown";
    warning?: string;
  };

  totals: {
    playtimeSeconds: number;
    sessions: number;
    activeDays: number;
    longestSessionSeconds: number | null;
    averageSessionSeconds: number | null;
  };

  top: {
    launcher?: NamedMetric;
    instance?: NamedMetric;
    version?: NamedMetric;
    server?: NamedMetric;
    world?: NamedMetric;
  };

  daily: Array<{
    date: string;
    playtimeSeconds: number;
    sessions: number;
  }>;

  recentSessions: SessionSummary[];
};

export type NamedMetric = {
  id: string;
  name: string;
  value: number;
};
```

---

# 29. Example Analytics Queries

## Total playtime

```sql
SELECT COALESCE(SUM(duration_seconds), 0)
FROM sessions
WHERE ignored = 0
  AND duration_seconds IS NOT NULL;
```

## Active days

```sql
SELECT COUNT(DISTINCT DATE(started_at))
FROM sessions
WHERE ignored = 0;
```

## Playtime by month

```sql
SELECT
    STRFTIME('%Y-%m', started_at) AS month,
    SUM(duration_seconds) AS seconds
FROM sessions
WHERE ignored = 0
  AND duration_seconds IS NOT NULL
GROUP BY month
ORDER BY month;
```

## Top instance

```sql
SELECT
    instances.id,
    instances.name,
    SUM(sessions.duration_seconds) AS seconds
FROM sessions
JOIN instances ON instances.id = sessions.instance_id
WHERE sessions.ignored = 0
GROUP BY instances.id
ORDER BY seconds DESC
LIMIT 1;
```

## Longest break

This is easier with a window function:

```sql
WITH ordered_days AS (
    SELECT DISTINCT DATE(started_at) AS day
    FROM sessions
    WHERE ignored = 0
),
gaps AS (
    SELECT
        day,
        LAG(day) OVER (ORDER BY day) AS previous_day
    FROM ordered_days
)
SELECT
    previous_day,
    day,
    JULIANDAY(day) - JULIANDAY(previous_day) - 1 AS gap_days
FROM gaps
WHERE previous_day IS NOT NULL
ORDER BY gap_days DESC
LIMIT 1;
```

---

# 30. React Dashboard Starter

```tsx
import { useEffect, useState } from "react";
import { getDashboard, type DashboardData } from "../lib/api";

export function DashboardPage() {
  const [data, setData] = useState<DashboardData | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const today = new Date();
    const from = new Date(today);
    from.setFullYear(today.getFullYear() - 1);

    getDashboard({
      from: from.toISOString(),
      to: today.toISOString(),
    })
      .then((result) => setData(result as DashboardData))
      .catch((reason: unknown) => {
        setError(reason instanceof Error ? reason.message : String(reason));
      });
  }, []);

  if (error) {
    return <div role="alert">Failed to load dashboard: {error}</div>;
  }

  if (!data) {
    return <div>Loading dashboard…</div>;
  }

  return (
    <main className="dashboard">
      <header>
        <h1>Minecraft Analytics</h1>
        <p>{data.coverage.warning}</p>
      </header>

      <section className="metric-grid">
        <Metric
          label="Total playtime"
          value={formatDuration(data.totals.playtimeSeconds)}
        />
        <Metric
          label="Active days"
          value={String(data.totals.activeDays)}
        />
        <Metric
          label="Longest session"
          value={formatOptionalDuration(
            data.totals.longestSessionSeconds,
          )}
        />
        <Metric
          label="Sessions"
          value={String(data.totals.sessions)}
        />
      </section>

      <section>
        <h2>Recent sessions</h2>
        <SessionList sessions={data.recentSessions} />
      </section>
    </main>
  );
}

function Metric(props: { label: string; value: string }) {
  return (
    <article className="metric-card">
      <span>{props.label}</span>
      <strong>{props.value}</strong>
    </article>
  );
}

function formatDuration(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return `${hours.toLocaleString()} h ${minutes} min`;
}

function formatOptionalDuration(seconds: number | null): string {
  return seconds === null ? "Unknown" : formatDuration(seconds);
}
```

---

# 31. Session Table Requirements

The session table should use virtualization for large histories.

Columns:

- Date
- Start
- End
- Duration
- Launcher
- Instance
- Version
- Server or world
- Exit
- Confidence

Features:

- Multi-column sorting
- Column visibility
- Search
- Date filters
- Export selected rows
- Keyboard navigation
- Context menu
- Open source log
- Manual correction

---

# 32. Visual Design Direction

The design should feel:

- Technical
- Calm
- Modern
- Minecraft-inspired without copying Minecraft assets
- Data-focused
- Not like a generic AI-generated dashboard
- Compact but readable

Suggested visual principles:

- Dark charcoal background
- Neutral cards
- One restrained green accent
- Pixel-inspired headings used sparingly
- Normal readable body font
- Clear grid
- Minimal gradients
- Subtle borders
- No excessive glow
- No random floating shapes
- Real density based on useful data
- Smooth but restrained animations

Do not copy official Minecraft branding, icons, or textures without permission.

---

# 33. Accessibility

Requirements:

- Keyboard navigation
- Visible focus states
- High contrast
- Screen-reader labels
- Reduced-motion support
- Scalable text
- Color-independent status indicators
- Tooltips accessible by keyboard
- Charts with table alternatives
- Clear error messages
- No critical meaning communicated only through green or red

---

# 34. Privacy and Security

## Required behavior

- No account required
- No cloud storage by default
- No analytics by default
- No telemetry by default
- No password collection
- No Microsoft-token reading
- No automatic uploads
- No execution of Minecraft JARs
- No modification of Minecraft files
- No DNS lookups without permission
- No remote server scans
- No hidden background service

## Sensitive local data

Potentially sensitive information includes:

- Usernames
- Server addresses
- Folder paths
- World names
- Screenshots
- Timestamps
- Crash-report hardware details
- Mod lists
- Notes

The app should allow masking or excluding these from exports.

## File access

Use the minimum required permissions.

Do not grant unrestricted file-system access to the frontend. File operations should go through controlled backend commands.

---

# 35. Performance Requirements

Target behavior:

- UI launches in under a few seconds on a normal PC
- Dashboard queries return quickly from SQLite
- First scan supports thousands of files
- Large log files are streamed
- Screenshots are lazy-loaded
- Thumbnail generation runs in the background
- Database writes use transactions
- Parsers work in batches
- The UI remains responsive during scans
- Scans can be paused or cancelled
- Incremental scans should usually finish quickly

Optimization ideas:

- Hash only when timestamps and sizes are insufficient
- Store normalized daily aggregates
- Add database indexes
- Use prepared statements
- Parse independent files in parallel
- Limit concurrent disk operations
- Cache thumbnails
- Paginate session queries

---

# 36. Error Handling

Every parser should return structured errors.

```rust
#[derive(Debug, thiserror::Error)]
pub enum MineTraceError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Parse error in {path}: {message}")]
    ParseError {
        path: String,
        message: String,
    },

    #[error("Database error: {0}")]
    Database(String),

    #[error("Scan cancelled")]
    Cancelled,
}
```

The user-facing error should be simpler than the developer log.

---

# 37. Logging

Application logs should include:

- Scan start and finish
- Launcher discovery
- File counts
- Parser warnings
- Database migrations
- Performance timings
- Errors

Do not log full server addresses or usernames when privacy mode is enabled.

Suggested levels:

- Error
- Warn
- Info
- Debug
- Trace

Debug logs should be optional.

---

# 38. Testing Strategy

## 38.1 Unit tests

Test:

- Timestamp parsing
- Filename-date extraction
- Session reconstruction
- Midnight rollover
- Server normalization
- Version extraction
- Mod metadata parsing
- Crash classification
- Data-quality scoring
- Export privacy filters

## 38.2 Fixture tests

Keep anonymized fixture files for:

- Vanilla logs
- Fabric logs
- Forge logs
- NeoForge logs
- Compressed logs
- Incomplete logs
- Crash logs
- Multiple sessions in one file
- Logs crossing midnight
- Copied duplicate logs
- Different launcher layouts
- Stats files
- Advancement files
- World metadata
- Corrupt files

## 38.3 Integration tests

Test:

- Discover launcher
- Discover instance
- Scan files
- Import database
- Run dashboard query
- Export CSV
- Apply manual correction
- Rescan without duplicates

## 38.4 UI tests

Test:

- First-run setup
- Scan progress
- Dashboard rendering
- Filters
- Search
- Error states
- Empty states
- Manual correction
- Export flow

---

# 39. Example Unit Test

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_server_address() {
        assert_eq!(
            normalize_server_address("  PLAY.EXAMPLE.NET. "),
            "play.example.net"
        );
    }
}
```

---

# 40. Deduplication Rules

Duplicate logs can appear after backups or instance copies.

Possible fingerprint:

```text
hash(
    normalized first relevant line
    + normalized last relevant line
    + file size
    + detected session start
)
```

Rules:

- Keep both source files
- Link them to one canonical session when clearly identical
- Mark uncertain duplicates for review
- Never delete source files
- Allow the user to undo deduplication

---

# 41. Overlapping Sessions

Overlaps can be valid if two Minecraft clients were open simultaneously.

Do not automatically merge all overlaps.

Show:

```text
Two sessions overlap by 42 minutes.

Possible explanation:
Two instances were running at the same time.
```

Analytics options:

- Sum all client runtime
- Count unique wall-clock time
- Show both values

This distinction is valuable:

```text
Total client runtime: 1,284 h
Unique elapsed playtime: 1,241 h
```

---

# 42. Data Quality Score

A useful score can consider:

- Percentage of sessions with verified starts
- Percentage with verified ends
- Number of missing months
- Number of truncated logs
- Duplicate rate
- Crash rate
- Unknown launcher rate
- Manual corrections
- Date range

Example:

```text
Data quality: 82/100
Coverage: Partial
Main limitation: Logs before September 2021 are missing
```

The formula should be documented.

---

# 43. MVP Scope

The first usable version should include only the most important features.

## MVP features

- Windows support
- Official Launcher detection
- Prism or MultiMC-style instance detection
- Manual folder selection
- `.log` and `.log.gz` parsing
- Session reconstruction
- Total playtime
- Daily and monthly charts
- Calendar heatmap
- Session list
- Instance list
- Basic server detection
- SQLite database
- Incremental scan
- JSON and CSV export
- Dark and light theme
- Privacy-first behavior
- Windows installer

## Do not include in MVP

- AI insights
- Screenshot image analysis
- Complex world-map rendering
- Every launcher
- Every NBT field
- Cloud sync
- Social sharing
- Mobile app
- Online accounts
- Automatic mod repair

---

# 44. Development Phases

## Phase 0 — Repository setup

- Create Tauri project
- Add React and TypeScript
- Add formatting and linting
- Add SQLite
- Add migrations
- Add CI
- Create basic navigation

## Phase 1 — Discovery

- Official Launcher
- Prism/MultiMC
- Manual folder
- Instance validation
- Discovery results page

## Phase 2 — Log parser

- Plain logs
- GZip logs
- Timestamp parser
- Event parser
- Session reconstruction
- Confidence scoring
- Fixtures and tests

## Phase 3 — Database

- Source files
- Sessions
- Instances
- Scan runs
- Incremental import
- Deduplication

## Phase 4 — Dashboard

- Summary cards
- Daily chart
- Monthly chart
- Calendar heatmap
- Recent sessions
- Data-quality panel

## Phase 5 — Explorer pages

- Sessions
- Instances
- Versions
- Servers
- Search and filters

## Phase 6 — Local worlds

- World discovery
- `level.dat`
- Stats
- Advancements
- World page

## Phase 7 — Mods and crashes

- Mod metadata
- Mod lists
- Crash reports
- Crash analytics

## Phase 8 — Export and packaging

- JSON
- CSV
- Backup
- Installer
- Portable build
- Release checklist

---

# 45. Acceptance Criteria for the MVP

The MVP is complete when:

- The application installs on Windows
- It launches without requiring an account
- It finds the official `.minecraft` directory
- It finds at least one multi-instance launcher
- The user can add a custom folder
- It parses plain and compressed logs
- It reconstructs sessions
- It labels estimated sessions clearly
- It stores results in SQLite
- A second scan does not duplicate sessions
- The dashboard shows total playtime
- The dashboard shows daily activity
- The dashboard shows a heatmap
- The session page supports filters
- The app exports sessions to CSV and JSON
- No Minecraft files are modified
- No private data leaves the computer
- Parser tests cover multiple Minecraft versions and loaders
- The `.exe` or installer builds successfully

---

# 46. Build Commands

Example project setup:

```bash
pnpm create tauri-app
pnpm install
pnpm tauri dev
pnpm tauri build
```

Rust checks:

```bash
cd src-tauri
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test
```

Frontend checks:

```bash
pnpm lint
pnpm typecheck
pnpm test
pnpm build
```

Windows release:

```bash
pnpm tauri build
```

The generated installer location depends on the Tauri bundle configuration.

---

# 47. Example `Cargo.toml` Dependencies

```toml
[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
anyhow = "1"
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
walkdir = "2"
regex = "1"
flate2 = "1"
rusqlite = { version = "0.32", features = ["bundled"] }
rayon = "1"
notify = "8"
blake3 = "1"
zip = "4"
```

Versions should be reviewed during implementation rather than copied blindly.

---

# 48. Example Frontend Dependencies

```json
{
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@tanstack/react-query": "^5",
    "@tanstack/react-table": "^8",
    "echarts": "^5",
    "react": "^19",
    "react-dom": "^19",
    "react-router-dom": "^7",
    "zustand": "^5"
  },
  "devDependencies": {
    "@types/react": "^19",
    "@types/react-dom": "^19",
    "@vitejs/plugin-react": "^4",
    "typescript": "^5",
    "vite": "^7"
  }
}
```

Again, exact versions should be selected and tested when the project is started.

---

# 49. Suggested Routes

```text
/
 /dashboard
 /sessions
 /instances
 /instances/:instanceId
 /worlds
 /worlds/:worldId
 /servers
 /servers/:serverId
 /mods
 /mods/:modId
 /versions
 /crashes
 /screenshots
 /scan
 /settings
 /settings/locations
 /settings/privacy
 /settings/advanced
```

---

# 50. Empty States

Examples:

## No Minecraft installation found

```text
No Minecraft installation was found automatically.

Add a folder containing:
- logs
- saves
- versions
- options.txt

[Choose folder]
```

## No historical logs

```text
Only the current log was found.

MineTrace can analyze future sessions, but older playtime cannot be reconstructed from missing files.
```

## No server history

```text
No multiplayer connection entries were detected in the available logs.
```

---

# 51. Future Features

Possible later additions:

- macOS and Linux builds
- Bedrock Edition support where local data permits
- Optional encrypted cloud backup
- Multiple local profiles
- Portable mode
- Live session timer
- System tray widget
- Automatic folder watching
- Year-in-review report
- Shareable privacy-safe statistics card
- Achievement showcase
- World backup reminders
- Modpack comparison
- Version migration timeline
- Local screenshot search
- Optional local AI summaries
- Plugin system
- Parser-rule updates
- Community-created launcher adapters
- Minecraft server plugin integration with explicit permission
- Import from manually exported server statistics
- Home-screen widgets
- Mobile companion app
- LAN-only dashboard

---

# 52. Legal and Branding Notes

Use a clear disclaimer such as:

> MineTrace is an independent project and is not affiliated with, endorsed by, or sponsored by Mojang Studios or Microsoft.

Avoid:

- Using official Minecraft logos as the application icon
- Copying proprietary textures
- Implying official account access
- Redistributing launcher files
- Bundling third-party skins without permission
- Uploading user data without consent

---

# 53. Recommended README Structure

```text
# MineTrace

Local Minecraft analytics for Windows.

## Features
## Screenshots
## Privacy
## Supported launchers
## Installation
## Development
## Data accuracy
## Export
## Security
## Contributing
## License
## Disclaimer
```

---

# 54. Vibe-Coding Master Prompt

The following prompt can be given to Claude Code, Codex, or another coding agent.

```text
You are building a production-quality local desktop application called MineTrace.

Goal:
Create a Windows desktop app that discovers Minecraft Java Edition installations and launcher instances, scans local logs and related game data, reconstructs play sessions, stores normalized results in SQLite, and displays a clean analytics dashboard.

Core principles:
- Local-first
- Offline by default
- Read-only access to Minecraft files
- No account required
- No telemetry
- No file uploads
- Transparent confidence scoring
- Incremental scans
- Strong error handling
- Testable parser architecture

Required stack:
- Tauri
- React
- TypeScript
- Rust
- SQLite
- Vite
- ECharts or an equivalent chart library

MVP requirements:
1. Detect the official Minecraft directory on Windows.
2. Detect Prism or MultiMC-style instances.
3. Allow manual folder selection.
4. Find .log and .log.gz files.
5. Parse session start, session end, version, server joins, clean exits, and crashes.
6. Reconstruct sessions with a confidence level.
7. Store launchers, instances, files, sessions, scan runs, and corrections in SQLite.
8. Prevent duplicate imports.
9. Support incremental scanning.
10. Show total playtime, active days, longest session, session count, a daily chart, monthly chart, calendar heatmap, and recent sessions.
11. Add session filters for date, launcher, instance, version, server, and confidence.
12. Export sessions and daily aggregates to JSON and CSV.
13. Build a Windows installer.
14. Include unit tests and fixture-based parser tests.

Architecture:
- Launcher adapters
- File discovery layer
- Parser layer
- Session reconstruction layer
- Repository layer
- Analytics service
- Tauri command layer
- React UI

Safety:
- Never modify Minecraft files.
- Never execute JAR files.
- Never read Microsoft authentication tokens.
- Never send server addresses, usernames, paths, screenshots, or statistics over the network.
- Keep manual corrections separate from source data.
- Every estimated timestamp must store its source and confidence.

Development behavior:
- Work in small, reviewable steps.
- Before changing code, inspect the repository.
- Do not replace working architecture without explaining why.
- Add tests with each parser change.
- Prefer typed models over unstructured maps.
- Use migrations for every schema change.
- Handle Windows paths correctly.
- Stream large files.
- Keep the UI responsive during scans.
- Emit scan progress events.
- Do not invent support for a launcher until a real folder structure or fixture exists.

First task:
Create the repository structure, Tauri shell, React routing, SQLite initialization, the initial migration, Rust domain models, launcher-adapter trait, official-launcher discovery, manual-folder support, and a simple discovery-results page. Add tests for path validation.
```

---

# 55. Prompts for Individual Development Stages

## Discovery prompt

```text
Implement launcher discovery as an adapter system.

Start with:
- Official Minecraft Launcher
- Prism Launcher
- MultiMC-compatible instances
- Manual folders

Requirements:
- Return a confidence score
- Validate folders through multiple directory markers
- Do not scan the entire drive
- Read launcher configuration paths when available
- Keep detection logic isolated
- Add fixture-based tests
- Add a discovery page with enable/disable toggles
```

## Log parser prompt

```text
Implement a streaming parser for Minecraft .log and .log.gz files.

Extract:
- line timestamp
- startup events
- Minecraft version
- loader
- username when locally visible
- server connection
- local world connection hints
- disconnect
- clean shutdown
- crash indicators

Requirements:
- Parser rules must be version-tolerant
- Unknown lines must not fail the parse
- Preserve source line number
- Preserve raw source text only when debug mode is enabled
- Add anonymized fixtures for vanilla, Fabric, Forge, NeoForge, incomplete logs, and logs crossing midnight
```

## Session reconstruction prompt

```text
Build a deterministic session reconstruction engine.

Requirements:
- Accept ordered log events
- Create sessions
- Infer missing end times conservatively
- Detect crashes
- Handle midnight
- Handle multiple sessions in one file
- Preserve provenance
- Assign confidence
- Detect overlaps
- Avoid duplicate sessions
- Add extensive tests
```

## Dashboard prompt

```text
Build the dashboard UI.

Components:
- total playtime
- active days
- longest session
- session count
- data coverage
- daily chart
- monthly chart
- calendar heatmap
- top launcher
- top instance
- top version
- recent sessions

Requirements:
- Responsive
- Keyboard accessible
- Dark and light mode
- No excessive gradients
- No fake data after the backend is connected
- Loading, error, and empty states
- Charts must have textual alternatives
```

## Export prompt

```text
Implement privacy-safe export.

Formats:
- JSON
- CSV

Options:
- date range
- exclude paths
- hide usernames
- hide server addresses
- use aliases
- exclude notes
- include confidence
- include provenance

Add tests proving that hidden fields are not exported.
```

---

# 56. Suggested Issue Backlog

## Epic: Core platform

- Initialize Tauri app
- Add React router
- Add SQLite
- Add migration runner
- Add settings storage
- Add error boundary
- Add logging

## Epic: Discovery

- Official launcher adapter
- Prism adapter
- MultiMC adapter
- Manual folder
- Directory validation
- Discovery UI
- Save enabled locations

## Epic: Scanning

- Recursive walker
- File classifier
- Scan cancellation
- Scan progress
- File fingerprint
- Incremental scanning
- Scan history

## Epic: Parsing

- Plain logs
- GZip logs
- Timestamp parser
- Startup events
- Shutdown events
- Server events
- Crash events
- Session builder
- Confidence scoring

## Epic: Analytics

- Total playtime
- Daily aggregates
- Monthly aggregates
- Active days
- Longest session
- Top instance
- Top version
- Calendar heatmap
- Recent sessions

## Epic: Explorer

- Session table
- Filters
- Search
- Instance detail
- Server detail
- Version detail

## Epic: Privacy

- Offline mode
- Address masking
- Export redaction
- Clear database
- Diagnostic bundle redaction

## Epic: Release

- Windows installer
- Portable build
- Icon
- License
- Disclaimer
- Release checklist
- Crash-free smoke test

---

# 57. Definition of Done

A task is complete only when:

- Code compiles
- Types pass
- Tests pass
- Errors are handled
- UI states are included
- Accessibility is considered
- No private data is uploaded
- File access is read-only
- Database changes use migrations
- Documentation is updated
- The feature has at least one realistic fixture or test
- The result is manually tested on Windows

---

# 58. Final Recommendation

Start with the smallest trustworthy version:

1. Find instances
2. Parse logs
3. Reconstruct sessions
4. Store sessions
5. Display playtime
6. Explain confidence
7. Export data
8. Build the `.exe`

Do not begin with screenshots, AI, every launcher, or advanced world statistics.

The main technical challenge is not the interface. It is reliably reconstructing sessions from inconsistent and incomplete local files. Build the parser and provenance system carefully first. Once the data layer is trustworthy, the dashboard becomes much easier.

---

# 59. One-Sentence Project Description

> MineTrace is a private, local-first Windows application that turns Minecraft Java Edition logs, launcher instances, worlds, and statistics into a searchable personal play-history dashboard.

