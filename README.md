# Agent Deck

Agent Deck is a read-only macOS dashboard that brings your Codex, Claude, and Cursor activity into one place.

![Agent Deck showing active sessions and usage at 2560 × 720](./agent-deck-2560x720.png)

## A companion display for AI coding sessions

Codex, Claude, and Cursor each keep their conversations and usage information in a different place. Agent Deck turns those separate sources into a single, glanceable workspace.

The app is designed to stay open beside your main display or on a dedicated touchscreen. At a glance, you can see which agents are working, which sessions need attention, what happened in the selected conversation, and how much of each subscription quota you have used.

Agent Deck is intentionally passive. It does not create sessions, send prompts, resume work, or change provider settings.

## What you can monitor

- Recent Codex, Claude, and Cursor sessions in one unified list.
- Working, waiting, completed, and attention-required states.
- The selected conversation’s summary, recent events, model, branch, duration, and token count.
- Available plan limits, reset times, credits, and usage measurements.
- Provider availability and partial-data warnings.
- Automatic refresh every 15 seconds without interrupting the interface.

The three-column layout is optimized for a 2560 × 720 ultrawide touchscreen. On narrower displays, Agent Deck keeps sessions and usage visible while opening conversation details in a slide-over panel.

The interface follows your light or dark system appearance and supports English and French. A manual language choice is stored locally.

## How Agent Deck reads your data

Agent Deck has no account system and no dedicated cloud service. Its Rust adapters read the files and authenticated sessions already created by the installed desktop applications.

| Provider | Conversations | Usage |
| --- | --- | --- |
| Codex | `~/.codex/state_5.sqlite` and JSONL rollouts | The `codex app-server` bundled with the Codex app |
| Claude | `~/.claude/projects` and Claude Desktop session metadata | Local usage history and the Anthropic OAuth usage endpoint when available |
| Cursor | Cursor’s local `state.vscdb` database | Cursor’s usage summary using the existing local session |

Local databases are opened in read-only mode. Credentials and tokens remain in the Rust backend and are never exposed to the Vue interface.

> These local formats are implementation details rather than stable public APIs. Updates to Codex, Claude, or Cursor may require adapter changes.

## Run Agent Deck

You need macOS, Node.js 22 or later, pnpm 10, a stable Rust toolchain, and at least one supported app that has already been used on the Mac.

```bash
git clone https://github.com/eMeRiKa13/agent-deck.git
cd agent-deck
pnpm install
pnpm dev:desktop
```

Tauri starts the frontend at `http://127.0.0.1:1421` and opens the native macOS window. Agent Deck begins polling the available local providers automatically.

To work on the interface in a browser without reading local application data, run the demo adapter:

```bash
pnpm dev
```

## Develop and verify

Agent Deck uses Tauri 2 and Rust for local data access, with Vue 3, TypeScript, Vue I18n, Vite, and Tailwind CSS for the interface.

Run the frontend checks:

```bash
pnpm typecheck
pnpm test
pnpm build
```

Build the desktop application:

```bash
pnpm build:desktop
```

Run the Rust unit tests:

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo test --manifest-path src-tauri/Cargo.toml
```

Smoke tests that inspect installed application data are ignored by default. Run them explicitly with:

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo test --manifest-path src-tauri/Cargo.toml -- --ignored
```

## Project structure

```text
src/                    Vue interface and browser demo data
src/components/         Dashboard components
src/composables/        Polling and application state
src-tauri/src/providers Codex, Claude, and Cursor adapters
src-tauri/              Tauri configuration and Rust backend
```

## Privacy

Agent Deck runs locally. It does not upload conversations to an Agent Deck service or expose credentials collected by the provider adapters to the frontend.
