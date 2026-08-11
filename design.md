# MineTrace dashboard design

## Design direction: Forensic Cave Ledger

MineTrace uses a **Forensic Cave Ledger** style: a calm, information-dense desktop archive inspired by Minecraft's underground material world and the visual language of evidence analysis.

It should feel like opening a carefully maintained field record—not a gaming launcher, social dashboard, or generic analytics template. The interface is dark and mineral-toned by default, restrained in its use of color, precise about uncertainty, and designed to make a large personal history feel inspectable rather than overwhelming.

The visual shorthand is:

> Technical archaeology meets a quiet native desktop utility.

## Product intent

The person opening MineTrace has usually finished playing and wants to understand a history reconstructed from local files. Their core questions are:

1. How much activity was detected?
2. When did it happen?
3. Which instances, versions, worlds, and servers contributed?
4. How trustworthy or complete is the reconstruction?

The dashboard therefore prioritizes **truth, chronology, and confidence** over achievement, competition, or decorative statistics.

### Design checkpoint

| Area | Decision |
| --- | --- |
| Intent | Help a player inspect reconstructed local history and immediately understand its limits. |
| Hierarchy | Detected runtime is the focal point; coverage is the required second reading; detailed charts and lists follow. |
| Palette | Cave black, quartz, moss, torch amber, rain blue, redstone, and copper—colors that belong to the product's world. |
| Depth | Matte surface shifts and quiet borders; shadows are rare and reserved for true overlays or recovery panels. |
| Surfaces | Dark geological layers by default, with a parchment-and-stone light theme using the same hierarchy. |
| Typography | IBM Plex Sans Variable for readable interface copy; IBM Plex Mono for dates, durations, counts, and evidence metadata. |
| Spacing | Compact desktop density with a 4px base rhythm and small optical adjustments for icons and data rows. |

## Domain exploration

### Domain concepts

- local archive
- client logs
- reconstructed sessions
- evidence confidence
- chronological traces
- missing rotations and coverage gaps
- launcher and instance provenance
- worlds and multiplayer destinations
- read-only scanning
- private, local-first history

### Natural color world

- unlit cave stone
- deep forest moss
- pale quartz
- torchlight amber
- rain-washed blue stone
- oxidized copper
- redstone warning red
- parchment-colored map paper

### Signature element: the evidence seam

The evidence seam is the defining MineTrace visual. It is a horizontal run of small segments whose height, color, and texture communicate the strength and density of evidence over time.

- **Solid moss** means directly verified evidence.
- **Rain blue** means a strong estimate with a partially inferred boundary.
- **Torch hatching** means partial or inferred evidence.
- **Muted hatching** means unknown or missing evidence.
- Segment height shows relative activity intensity without pretending to be an exact chart axis.

This pattern appears in the hero ledger and is echoed by the monthly chart, heatmap, session timeline nodes, confidence indicators, and coverage messaging. It gives MineTrace an identity that would not make sense in an unrelated finance or administration dashboard.

### Defaults deliberately rejected

| Generic default | MineTrace alternative |
| --- | --- |
| Bright blue SaaS accent on gray cards | Moss, quartz, torch, and stone colors taken from the product domain. |
| A grid of equal KPI cards | One dominant runtime ledger, a coverage companion, then a restrained metric rail. |
| Decorative gradients and glowing charts | Flat mineral surfaces, low-contrast borders, solid evidence, and meaningful hatch patterns. |
| Every data point presented as equally certain | Confidence is part of the visual encoding and copy everywhere it matters. |
| Gamified achievements and celebratory graphics | Archival language, source-backed facts, and explicit caveats. |
| Large rounded consumer-app cards | Compact 6px/9px/13px radius scale suited to a desktop tool. |

## Overall appearance

At desktop size, MineTrace has a narrow permanent sidebar on the left, a translucent native-style toolbar across the top, and a centered content canvas. The dashboard opens with a title and detected date range, followed by a large ledger-like hero panel.

The hero is visually asymmetrical:

- the left side carries the oversized detected-runtime number;
- the right side is a darker coverage chamber with observed months and known gaps;
- a thin evidence seam runs across the full width beneath both;
- a compact rail of secondary metrics closes the panel.

Below the hero, panels alternate between broad visual summaries and tighter evidence lists. The layout avoids a monotonous grid of identical boxes. The monthly activity chart receives more width than the context ranking; the yearly heatmap receives more width than recent sessions. This creates an intentional rhythm of **overview → explanation → inspection**.

The interface is mostly charcoal-green and quartz. Moss is used sparingly for verified evidence, active navigation, and primary action. Amber appears only when uncertainty or gaps need attention. The result is dark but not cyberpunk, Minecraft-inspired but not pixel-art, and technical without looking like a developer console.

## Profile composition: player dossier

Profile extends the Forensic Cave Ledger into a **player dossier**, not a social-network profile. The hero combines a pixel-accurate front skin render, a compact head portrait, the locally detected player identity, and four archive facts. A diagonal evidence line and a thin moss-to-copper seam connect it to the dashboard without turning the screen into a game launcher.

The internal tabs are deliberately task-based:

1. **Overview** — stat spotlights, most-played worlds, save availability, backups, and locally retained previous skins.
2. **Statistics** — source-backed single-player statistics grouped into general, movement, mined, crafted, used, and defeated categories.
3. **Clients & launchers** — every retained launcher aggregate plus locally detected launcher identities.

Stat spotlights are selected deterministically from real local data, so they feel varied without changing every render or relying on fabricated achievements. The tiles reuse moss, torch, rain, and copper as a four-beat ledger sequence; they do not become rainbow KPI cards.

World status uses explicit evidence language:

- moss dot: save folder available;
- redstone dot: save not found, which may mean deleted, moved, renamed, or outside approved locations;
- torch dot: backup archive found without a matching live save.

The share flow is an honest image builder. The user selects fields, MineTrace renders a 1200×630 PNG locally, and a native save panel chooses the destination. The app never claims to post directly to a network and never includes server addresses in the card.

## Color system

Token names are part of the product vocabulary. Surfaces are named as cave layers, text as quartz, and semantic accents after materials or weather.

### Dark theme — default

| Token | Value | Role |
| --- | --- | --- |
| `--canvas` | `#0d110e` | Window and sidebar background. |
| `--cave-1` | `#111713` | Primary panel surface. |
| `--cave-2` | `#151c17` | Secondary panels, hover rows, coverage chamber. |
| `--cave-3` | `#1a231c` | Stronger hover or nested surface. |
| `--cave-raised` | `#202a22` | Raised overlays and recovery surfaces. |
| `--control-inset` | `#0b100d` | Inputs and recessed evidence strips. |
| `--quartz` | `#f0f2ea` | Primary text and hero values. |
| `--quartz-secondary` | `#b7beb4` | Supporting values and headings. |
| `--quartz-tertiary` | `#858e84` | Descriptions and secondary information. |
| `--quartz-muted` | `#7c867c` | Metadata, labels, and disabled information. |
| `--moss` | `#91af72` | Verified evidence and primary action. |
| `--moss-bright` | `#abd087` | Active emphasis and focus. |
| `--torch` | `#d6a75d` | Partial evidence, gaps, and caution. |
| `--redstone` | `#ca7569` | Errors and destructive states. |
| `--rain` | `#7d9fab` | High-confidence estimates. |
| `--copper` | `#b07a5c` | Small identity accents. |

### Light theme

The light theme is not plain white. It resembles pale map paper, limestone, and softly weathered UI panels:

- canvas: `#efede5`
- primary panel: `#f5f3ec`
- nested stone: `#e8e6dd` and `#deddd4`
- raised surface: `#fbfaf5`
- primary ink: `#18201a`
- moss accent: `#587745`
- torch accent: `#91682f`

The hue relationships remain the same across themes; only lightness and contrast change. This keeps evidence semantics stable.

### Color-use rules

- Keep approximately 90% of the screen neutral.
- Moss communicates verified evidence, active location, or a primary action—not decoration.
- Torch communicates uncertainty or incomplete evidence, never success.
- Redstone is reserved for errors and destructive actions.
- Rain blue means high confidence, not a second brand color.
- Never use color alone: combine it with text, icons, texture, or ARIA labels.
- Do not introduce unrelated purple, cyan, or bright gaming-neon accents.

## Typography

### Typeface roles

- **IBM Plex Sans Variable** is the main UI face. It feels engineered but remains human and readable in dense desktop layouts.
- **IBM Plex Mono** is used for durations, dates, counts, keyboard shortcuts, scales, evidence labels, and other machine-derived facts.

The combination reinforces the product idea: Sans explains the archive; Mono records the evidence.

### Hierarchy

| Element | Typical treatment |
| --- | --- |
| Hero runtime | 54–84px, weight 590, `-0.055em` tracking, tabular numbers. |
| Page title | 25–34px, weight 590, tight line height. |
| Panel title | 17px, weight 580. |
| Body/description | 12–13px, regular, tertiary quartz, 1.45–1.55 line height. |
| Eyebrow | 11px IBM Plex Mono, uppercase, 0.12em tracking, moss. |
| Data value | 11–15px IBM Plex Mono, weight 500, tabular numbers. |
| Metadata | 11px, muted quartz. |

Weight and contrast do more hierarchy work than unnecessary size changes. Large headings use slightly negative tracking; explanatory copy uses comfortable line height and balanced wrapping.

## Layout system

### Application shell

- Expanded sidebar: `232px`
- Collapsed sidebar: `68px`
- Desktop top bar: `56px`
- Content maximum width: `1540px`
- Main desktop page padding: `28px 32px 56px`
- Standard dashboard gap: `16px`

The sidebar and canvas share the same background. A subtle border separates them so the application reads as one continuous workspace rather than two colored zones.

The top bar uses a 92% opaque canvas color with an 18px backdrop blur. This is the only glass-like treatment; panels remain matte.

### Overview composition

1. Page header and detected range.
2. Hero ledger.
3. Primary grid: monthly activity at roughly `1.55fr`, top contexts at `0.75fr`.
4. Secondary grid: yearly heatmap at roughly `1.12fr`, recent sessions at `0.88fr`.
5. Coverage method note.

This order follows the user's mental model: **total → confidence → rhythm → context → individual evidence → caveat**.

### Density and spacing

MineTrace is compact but not cramped. Common measurements are:

- 4–8px for icon and micro-label gaps;
- 10–18px for row and control padding;
- 16px between dashboard panels;
- 24–32px for page and hero breathing room;
- 40px minimum desktop navigation-row height;
- 34–38px compact desktop control height;
- 44px-or-larger touch targets in mobile navigation and touch-oriented layouts.

Small 1–3px adjustments are optical corrections for traces, icons, and chart marks—not a second spacing system.

## Surface and depth system

The dashboard primarily uses **surface-color shifts plus quiet borders**.

| Level | Surface | Use |
| --- | --- | --- |
| 0 | `--canvas` | Window, sidebar, and workspace. |
| 1 | `--cave-1` | Standard panels and hero body. |
| 2 | `--cave-2` | Nested regions, hover rows, coverage chamber. |
| 3 | `--cave-3` | Strong hover and selected sub-surfaces. |
| Raised | `--cave-raised` | Dialogs, recovery UI, and real overlays. |
| Inset | `--control-inset` | Inputs, search, evidence seam background. |

Borders are deliberately weak:

- soft: about 5.5% opacity;
- standard: about 9.5%;
- strong/focus-adjacent: about 16%.

Large shadows are avoided in normal dashboard panels. They appear only where elevation is real, such as the recovery surface or command dialog.

### Radius scale

- controls: `6px`
- panels: `9px`
- large hero/dialog: `13px`

This modest scale keeps the product professional and tool-like. Pills and oversized rounded cards are not part of the visual language.

## Core dashboard components

### 1. Page header

The header establishes archival context before presenting numbers:

- mono moss eyebrow;
- clear human-readable title;
- one-sentence description;
- compact detected-range control aligned opposite the title on wide screens.

### 2. Hero ledger

The hero ledger is the screen's single focal point. The detected runtime is intentionally much larger than every other number. Its supporting copy explicitly says the figure is reconstructed and is not claimed as complete lifetime playtime.

Coverage is structurally attached to the hero rather than hidden in a tooltip. This prevents a large number from appearing more authoritative than the evidence supports.

### 3. Evidence seam

The seam is both brand signature and information model. It must always have a textual or ARIA explanation. Hatching must mean uncertainty or absence consistently throughout the product.

### 4. Metric rail

Sessions, active days, longest session, and average session appear as a horizontal ledger rail beneath the hero. These are supporting facts, not competing KPI cards. Labels stay small and muted; values use mono type and tabular figures.

### 5. Monthly activity

The activity chart uses slim moss bars on a quiet three-line grid. Any inferred portion is overlaid with torch-colored hatching. Missing calendar months remain visible as full-height muted hatched tracks, so gaps are not collapsed.

The chart can switch to an accessible data table. This is a functional view change, not a decorative animation.

### 6. Top contexts

Instances, versions, launchers, servers, and worlds are presented as compact linked rows with restrained icons. Runtime is used only where it is a defensible aggregate; world/server values are labeled “session-linked” rather than pretending a narrower destination duration is known.

### 7. Year heatmap

The heatmap uses tiny square trace cells rather than rounded contribution bubbles. Moss intensity shows detected runtime, torch hatching marks incomplete evidence, and muted hatching marks missing evidence. An accessible table exposes every day and its confidence.

### 8. Recent sessions

Recent sessions form a quiet chronological trace:

- time on the left;
- a one-pixel vertical line;
- a confidence-colored node;
- destination and instance/version context;
- duration and confidence at the right.

This reads as evidence entering the archive, not a generic activity feed.

### 9. Coverage note

The final amber note is a method statement, not a warning banner for drama. It explains known gaps, incomplete coverage, or the absence of evidence and links back to Scan Center.

## Confidence language

Confidence is a first-class visual dimension:

| Confidence | Color/treatment | Meaning |
| --- | --- | --- |
| Verified | Solid moss | Start and end are directly supported. |
| High | Rain blue | Most timestamps are supported; one boundary is inferred. |
| Partial | Torch amber with hatching | Available evidence is incomplete. |
| Unknown | Muted quartz with hatching | A reliable duration cannot be established. |
| Missing coverage | Transparent/muted hatch with outline | No evidence is available for that period. |

Unknown and missing values should display as unavailable or open—not as zero. Visual confidence must match backend confidence rather than being upgraded for presentation.

## Interaction and motion

Motion is quick and functional:

- hover/color changes: about `140ms`;
- press feedback: `120ms`, generally `scale(0.97–0.99)`;
- sidebar column change: `180ms`;
- primary easing: `cubic-bezier(0.23, 1, 0.32, 1)`;
- spinner: `750ms` linear rotation.

Only transform, opacity, color, border color, and background color should animate. Command search and frequently repeated navigation should feel immediate. `prefers-reduced-motion` reduces animations and disables smooth scrolling.

Interactive elements need visible hover, active, focus-visible, and disabled states. The focus ring uses bright moss and a 2px offset.

## Responsive behavior

MineTrace is desktop-first but deliberately responsive:

- **Below 1280px:** page gutters tighten and data columns simplify.
- **Below 1100px:** overview grids stack into one column; context rows can use two columns.
- **Below 920px:** the sidebar collapses to a 68px icon rail and detailed table columns are reduced.
- **Below 720px:** the sidebar disappears, a five-item bottom navigation appears, the top bar becomes icon-focused, and the hero stacks coverage below runtime.
- **Below 430px:** the metric rail becomes a single column and complex summaries reduce further.

Mobile is a reflow of the same evidence hierarchy, not a separate visual theme.

## Platform character

macOS and Windows share the same dashboard language and information architecture. Small shell details adapt to the platform:

- macOS reserves a title-bar drag zone near the sidebar brand;
- Windows uses the same app chrome without opening a console window;
- both use the same dark/light themes, evidence encodings, responsive behavior, and privacy mask.

The interface should feel native enough to inhabit either desktop while remaining unmistakably MineTrace.

## Accessibility and truthful states

- Primary navigation, panels, charts, and lists use semantic elements.
- A skip link moves focus directly to main content.
- Focus-visible outlines are always retained.
- Chart-only information has table or textual equivalents.
- Confidence uses icons, labels, patterns, and descriptions—not color alone.
- Loading uses structural skeletons matching the page hierarchy.
- Empty states distinguish never scanned, scan completed without evidence, and no matching results.
- Errors provide a recovery action without implying source files were modified.
- Server privacy masking changes visible text and accessible labels together.
- Dynamic numbers use tabular figures to prevent shifting.
- Headings use balanced wrapping and body text uses pretty wrapping.

## Design rules for future work

### Continue doing

- Lead each page with one clear archival question.
- Reuse cave, quartz, moss, torch, rain, copper, and redstone tokens.
- Treat evidence confidence as part of the component—not an optional badge added later.
- Prefer rows, rails, and seams over grids of identical statistic cards.
- Keep panels matte, borders quiet, and controls visibly inset.
- Use mono typography only for machine-derived facts and compact metadata.
- Preserve explicit loading, empty, error, partial, and truncated states.
- Keep privacy and source-read-only claims consistent with actual behavior.

### Avoid

- generic blue SaaS visuals;
- neon gaming effects, pixel fonts, or Minecraft texture imitation;
- glass panels throughout the content area;
- decorative gradients and large drop shadows;
- equal visual weight for every metric;
- charts that collapse missing time or hide uncertainty;
- replacing missing values with zero;
- oversized pills, excessive rounding, or playful badges;
- unmotivated accent colors;
- animation that delays common navigation or data inspection.

## Implementation map

The implemented design is primarily defined in:

- `src/styles/global.css` — tokens, shell, dashboard layout, states, and responsive rules;
- `src/pages/OverviewPage.tsx` — dashboard composition and archive states;
- `src/components/dashboard/HeroLedger.tsx` — focal runtime, coverage, seam, and metric rail;
- `src/components/dashboard/ActivityChart.tsx` — observed/estimated/missing monthly rhythm;
- `src/components/dashboard/CalendarHeatmap.tsx` — daily trace calendar and accessible table;
- `src/components/dashboard/TopContexts.tsx` — dominant instance/version/launcher/destination rows;
- `src/components/dashboard/RecentSessions.tsx` — chronological evidence feed;
- `src/components/dashboard/CoverageNote.tsx` — evidence-method disclosure;
- `src/components/ui/EvidenceSeam.tsx` — signature confidence visualization;
- `src/components/ui/ConfidenceBadge.tsx` — shared confidence semantics;
- `src/components/layout/AppShell.tsx` — desktop navigation, top bar, privacy action, and mobile shell.

This document describes the current MineTrace dashboard rather than a future redesign. New dashboard work should preserve this hierarchy and vocabulary unless the overall product direction is intentionally changed.
