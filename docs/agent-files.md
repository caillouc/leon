# Agent and skill files

How Leon finds your agents and skills, what it understands in them, and how
it hands them to each CLI. State on 2026-11-18: Claude Code 2.1.288, Codex
0.160.0, Copilot CLI 1.0.113. ⚠️ marks what was not checked on a real run
(listed at the end).

## In short

- Write agents and skills once, in `~/.agents/` (yours) or a project's
  `.agents/` (shared with the repository). No CLI is privileged: Leon bridges
  each file to the CLI that runs it.
- An agent is one Markdown file: `name`, `description`, optional `model` and
  `tools`, then its instructions.
- A skill is one Markdown file, `<name>.md` (or a `<name>/SKILL.md` folder
  when it brings scripts). Every CLI sees every skill and loads one only when
  the agent needs it.
- `~/.agents/AGENTS.md` holds what every agent should know, in every CLI,
  like a `CLAUDE.md` for all of them.
- Your existing `.claude/agents/` and `.github/agents/` files keep working.

## Where Leon looks

**Agents**, a later folder hiding an agent of the same name in an earlier one:

| Level | Folders, in order |
|---|---|
| Yours | `~/.claude/agents/`, `~/.github/agents/`, `~/.agents/agents/` |
| Source folders (Settings) | `<folder>/agents/` |
| The project | `.claude/agents/`, `.github/agents/`, `.agents/agents/` |

So `.agents/` wins over `.claude/` and `.github/`, and a project's agent wins
over yours. Every `*.md` file directly in the folder is an agent (subfolders
are skipped). Leon reads them when it starts and when a project is added:
restart Leon after adding one.

**Skills**: `~/.agents/skills/`, each source folder's `skills/`, then the
project's `.agents/skills/` (a later one hides an earlier one of the same
name). In `.agents/skills/`, a skill is a `<name>.md` file or a
`<name>/SKILL.md` folder; in a source folder, a `<name>/SKILL.md` folder.
Leon reads them each time an agent session starts: a new skill shows up in
the next session.

The `/` list of the message box also shows each CLI's own skills and
commands (`.claude/skills/`, `.github/skills/`, `.claude/commands/`); those
reach only their own CLI.

**Settings → Agents and skills** lists the folders Leon finds (`~/.agents`,
`~/.claude`, `~/.github` when they exist) and the ones you added, with how
many agents and skills each holds.

## An agent file

```markdown
---
name: reviewer
description: Reviews a diff for bugs and missing tests. Use after code changes.
model: opus
tools: [read, search, execute]
---
You review code changes. You never edit files.

1. Read the diff and the code around it. Follow the `code-style` skill.
2. If tests changed, run them.
3. Answer with findings: file:line, what breaks, the fix.
```

| Field | Leon | Required |
|---|---|---|
| `name` | The agent's name; else the file name without `.md` (or `.agent.md`) | No |
| `description` | Shown in the new-conversation form | Yes |
| `model` | Picks the CLI and the model (below) | No |
| `tools` | Limits what the agent can do (below) | No |
| body | The agent's instructions | Yes |

Lists can be written `[a, b]`, `a, b`, or one `- a` per line. Other fields
(`color`, `skills`, `permissionMode`…) are ignored by Leon.

### `model` picks the CLI

The agent runs on the first CLI, in the order set in **Settings → CLI for an
agent's model** (default Claude Code, Codex, Copilot CLI), that offers the
model:

| `model` | Runs on |
|---|---|
| `opus`, `sonnet`, `haiku` | Claude Code (its own short names) |
| `claude-opus-5-5` or `claude-opus-5.5` | Claude Code (dots and hyphens match) |
| `gpt-6-luna` | Codex (Copilot CLI offers it too, but comes later) |
| `gemini-3.8-flash` | Copilot CLI, the only one offering it |
| none, or `inherit` | The CLI and model chosen in the form, or the calling agent's |
| a model no CLI offers | The same; the form says so |

The models each CLI offers are the ones its pickers show in Leon. Picking
the agent in the new-conversation form sets the CLI and model pickers; you
can still change them. An agent started by another agent takes its file's
model and CLI the same way, else the caller's. On another CLI than the
caller's, the effort is kept when that CLI has it and the permission becomes
that CLI's "auto".

### `tools` limits the agent

Without `tools`, the agent has every tool its CLI has. With it, the agent
keeps only these abilities, in GitHub's custom-agent words (Claude Code's
tool names are understood too). MCP tools are never limited.

| Word | Also understood | Claude Code without it | Codex without it | Copilot CLI without it |
|---|---|---|---|---|
| `read` | `Read` | no `Read` | — (reads through its shell) | `--deny-tool=read` |
| `search` | `Grep`, `Glob` | no `Grep`, `Glob` | — (searches through its shell) | — ⚠️ |
| `edit` | `Edit`, `Write`, `MultiEdit` | no `Edit`, `Write`, `NotebookEdit` | read-only sandbox | `--deny-tool=write` |
| `execute` | `shell`, `Bash` | no `Bash` | `features.shell_tool = false` ⚠️ | `--deny-tool=shell` |
| `web` | `WebFetch`, `WebSearch` | no `WebFetch`, `WebSearch` | `web_search = "disabled"` ⚠️ | `--deny-tool=url` |
| `agent` | `Task` | no `Agent` | `features.multi_agent = false` ⚠️ | — |
| `todo` | `TodoWrite` | no `TodoWrite`, `TaskCreate`, `TaskUpdate` | — | — |

"—": that CLI has no switch for it, so the agent keeps it there.

### What each CLI receives

| CLI | The agent's instructions |
|---|---|
| Claude Code | `--agents '{"<name>":{…,"prompt":<body>}}' --agent <name>`: the body replaces Claude Code's default system prompt; `CLAUDE.md` still loads |
| Codex | the body as `developerInstructions` of `thread/start` |
| Copilot CLI | the body as the ACP system prompt, when the CLI offers it ⚠️ |

Write the body so it stands alone, in plain verbs ("search the code", "run
the tests") rather than one CLI's tool names. To hand work to another agent,
use Leon's `start_agent` tool (`agent: "<name>"`, a prompt that says
everything); its answer comes back as a message starting `[From …]`.

## Shared instructions: `~/.agents/AGENTS.md`

What every agent should know, whatever its CLI: your conventions, how you
like answers, what never to do. Leon reads it each time an agent session
starts and hands it over:

| CLI | How | Checked |
|---|---|---|
| Claude Code | `--append-system-prompt`: after its default prompt, or after the agent's instructions | 2.1.288, 2026-11-18: an agent run with `--agent` follows it |
| Codex | first in `developerInstructions`, before the agent's instructions | ⚠️ not in a real turn |
| Copilot CLI | `COPILOT_CUSTOM_INSTRUCTIONS_DIRS=~/.agents`: it reads the file itself | `copilot instruction list`, 1.0.113; ⚠️ not with `--acp` |

A project's own `AGENTS.md` is left to the CLIs: Codex and Copilot CLI read
it themselves, Claude Code when the project has no `CLAUDE.md` (or a
`CLAUDE.md` that imports it with `@AGENTS.md`). Each CLI's personal file
(`~/.claude/CLAUDE.md`, `~/.codex/AGENTS.md`,
`~/.copilot/copilot-instructions.md`) still loads in that CLI: keep only
what is specific to that CLI there, or an agent reads it twice.

## A skill file

```markdown
---
name: release
description: Cuts a release: changelog, version bump, tag. Use when asked to release.
---
1. Update CHANGELOG.md from the merged pull requests.
…
```

`name` (else the file name) and `description` are what every CLI shows the
agent up front; the body loads only when the agent decides to use the skill.
An agent that must use a skill says so in its text ("follow the `release`
skill"); there is no skills field.

How each CLI sees them, without Leon writing into the CLIs' own folders:

| CLI | Reads itself | Leon hands it |
|---|---|---|
| Claude Code | — | every `.agents/skills` skill, copied as `<dir>/.claude/skills/<name>/SKILL.md`, `--add-dir=<dir>`; source folders as plugins |
| Codex | `.agents/skills/<name>/SKILL.md` | `<name>.md` skills and source-folder skills, copied, through `skills/extraRoots/set` |
| Copilot CLI | `.agents/skills/<name>/SKILL.md` | the same, copied into a plugin folder, `--plugin-dir=<dir>` |

The copies live in Leon's data folder (`~/Library/Application
Support/leon/skills/` on macOS) and are refreshed each time a session starts.
Edit the originals, never the copies.

## Not verified

- Codex: `~/.agents/AGENTS.md` through `developerInstructions` was not
  checked in a real turn.

- Codex: the extra skill folder was checked with `skills/list`, not in a
  real turn; the `tools` switches (`features.shell_tool`, `web_search`,
  `features.multi_agent`) come from its config reference, not a real turn.
- Copilot CLI: `--plugin-dir` was checked with `copilot skill list`, and the
  `--deny-tool` rules from `copilot help permissions`, not with `--acp` as
  Leon runs it; nor whether the ACP system prompt is offered.
- Claude Code reads `~/.agents/skills` itself: its binary names that folder
  only in its "import from other tools" scan, so Leon hands those skills
  over.
