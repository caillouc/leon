# Agent files for Leon and every CLI

How to turn agent files written for Claude Code into files that Leon, Claude
Code, Codex and Copilot CLI all use. State on 2026-11-18: branch `v1-build`,
Claude Code 2.1.288, Codex 0.160.0, Copilot CLI 1.0.113. Each fact about a CLI
names its source; ⚠️ marks what was not verified (listed at the end).

## In short

- Leon reads only `name`, `description` and the body. Inside Leon, no CLI
  applies the other frontmatter fields to the agent you talk to.
- Flat `<name>.md` files. Yours in `~/.claude/agents/`, plus a symlink
  `~/.copilot/agents → ~/.claude/agents`. A project's in `.claude/agents/`.
- Frontmatter: `name` and `description`. What `tools:` and `skills:` meant
  goes into the body as sentences; `model:` goes.
- Body in plain words. Other agents are started with Leon's `start_agent`,
  with a fallback sentence for outside Leon.
- Codex has no Markdown agents: inside Leon it gets the body; alone it needs a
  TOML copy.

## 1. How Leon finds and uses agents today

**Where it looks.** The `Catalog` port ([ports.rs:139-149](../crates/leon-core/src/ports.rs)),
implemented by `AgentFiles::agents` ([leon-catalog/src/lib.rs:46-60](../crates/leon-catalog/src/lib.rs)).
On a name clash the later folder wins:

| Level | Folders, in order |
|---|---|
| User | `~/.agents/agents/`, `~/.claude/agents/`, `~/.github/agents/` (lib.rs:43, 49-51) |
| Source folders | `<folder>/agents/` per `[sources] folders` in `~/.config/leon/config.toml` (lib.rs:52-54) |
| Project | `.agents/agents/`, `.claude/agents/`, `.github/agents/` (lib.rs:55-57) |

**What it reads** (lib.rs:106-133, parser [frontmatter.rs:7-46](../crates/leon-catalog/src/frontmatter.rs)):
- every `*.md` directly in the folder (`*.agent.md` included); subfolders are skipped;
- `name` (else the filename without `.md` / `.agent.md`), `description`, and the body as
  instructions ([`AgentDefinition`, ports.rs:88-97](../crates/leon-core/src/ports.rs)). Every
  other key is ignored;
- frontmatter only if `---` is line 1, else the whole file is the body (frontmatter.rs:8-16);
  `key: value` lines, quotes, `>` / `|` multi-line values;
- read at start and when a project is added ([app.rs:247, 342](../crates/leon-core/src/app.rs)):
  restart Leon after editing agent files.

**Settings.** CLI, model, effort, permission come from the new-conversation
form; an agent started by another takes the caller's
([agents.rs:104-110](../crates/leon-core/src/agents.rs), decision I7). `model:`
in a file is never used.

**What each CLI receives** (definition looked up by name, [app.rs:540-553](../crates/leon-core/src/app.rs)):

| CLI | What Leon sends | Code |
|---|---|---|
| Claude Code | `--agents '{"<name>":{"description":…,"prompt":<body>}}' --agent <name>` | [claude.rs:43-51](../crates/leon-backends/src/claude.rs) |
| Codex | Body as `developerInstructions` of `thread/start`; name and description not sent | [codex.rs:57-59](../crates/leon-backends/src/codex.rs) |
| Copilot CLI | No `--agent`. Body as ACP `systemPrompt` (`append`) if the CLI advertises `sessionCapabilities.systemPrompt`, else glued before the first message ⚠️ | [acp/setup.rs:38-46, 87-97](../crates/leon-backends/src/acp/setup.rs), [acp/session.rs:89-93, 218-225](../crates/leon-backends/src/acp/session.rs) |

- Claude Code: `--agents` outranks `.claude/agents/` and `~/.claude/agents/`
  (docs, sub-agents page), so inside Leon the file's `tools`, `model`,
  `skills`… do not apply. With `--agent` the body *replaces* the default system
  prompt; CLAUDE.md still loads (same page).
- Codex: `developerInstructions` is a `ThreadStartParams` field (schema from
  `codex app-server generate-json-schema`). Leon leaves `baseInstructions`
  unset; whether Codex keeps its base prompt ⚠️.

**Skills and commands** ([commands.rs:1-38](../crates/leon-catalog/src/commands.rs)): only listed
in the `/` menu — `<name>/SKILL.md` under `.agents/skills/`, `.claude/skills/`, `.github/skills/`,
and `.claude/commands/*.md`; home, then source folders (`skills/`, `commands/`), then project.
Picking one types `/name ` ([message_input.rs:156](../crates/leon-ui/src/message_input.rs)); the
CLI resolves it. Leon never injects a skill.

**Source folders and MCP.** A source folder has the plugin layout (`agents/`,
`skills/`, `commands/`, `.mcp.json`; decision P3). Claude Code gets it as
`--plugin-dir` (claude.rs:68-74); Codex and Copilot CLI get only its `.mcp.json`
stdio servers (codex.rs:69-78, acp/setup.rs:114-120; I10); `url` servers are
skipped ([sources.rs:19-41](../crates/leon-catalog/src/sources.rs)). Leon's
shared MCP list (Connectors view) reaches every CLI
([leon-core/src/tools.rs:67-75](../crates/leon-core/src/tools.rs), I14).

## 2. What each CLI reads on its own

| | Claude Code 2.1.288 | Codex 0.160.0 | Copilot CLI 1.0.113 |
|---|---|---|---|
| Your agents | `~/.claude/agents/` | `~/.codex/agents/` | `~/.copilot/agents/`; **not** `~/.claude/agents/` |
| Project agents | `.claude/agents/` | `.codex/agents/` | `.github/agents/`, `.claude/agents/` |
| Format | Markdown + YAML | **TOML**, one per file | Markdown + YAML, `.agent.md` or `.md` |
| Identity | `name` | `name` | Filename minus `.agent.md` / `.md`; subfolder → `dir--name` |
| Required | `name`, `description` | `name`, `description`, `developer_instructions` | `description` |
| Other fields | `tools`, `disallowedTools`, `model` (`sonnet`, `opus`, `haiku`, `fable`, full id, `inherit`), `permissionMode`, `skills`, `mcpServers`, `hooks`, `memory`, `color`, `effort`, `isolation`, `maxTurns`…; unknown ignored | `model`, `model_reasoning_effort`, feature and skill switches; sandbox, approvals, MCP follow the parent | `name` (display), `tools`, `model`, `target`, `mcp-servers`, `disable-model-invocation`, `user-invocable`, `metadata`, `include-custom-instructions` |
| Use an agent | `claude --agent <name>` | Spawned as a subagent | `copilot --agent <name>`, `/agent` |
| Own subagents | `Agent` tool (was `Task`) | `multi_agent` feature, on | Delegates to custom agents |
| Your skills | `~/.claude/skills/` | `~/.agents/skills/` | `~/.copilot/skills/`, `~/.agents/skills/` |
| Project skills | `.claude/skills/` | `.agents/skills/`, cwd up to repo root | `.github/skills/`, `.agents/skills/`, `.claude/skills/` |
| Symlinked skills folder | ⚠️ not tested | Followed (tested) | Followed (tested) |

Sources, 2026-11-18. Claude Code: `claude --help`; docs `code.claude.com/docs/en/sub-agents`, `/skills`,
`/plugins-reference`. Codex: docs `learn.chatgpt.com/docs/agent-configuration/subagents` and
`/docs/build-skills` (where `developers.openai.com/codex/…` redirects); `codex features list`.
Copilot CLI: `copilot help config` ("Claude Code plugins, skills, hooks and settings"), `copilot skill
--help`; `docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/create-custom-agents-for-cli`,
`/en/copilot/reference/custom-agents-configuration`. Skills: `agentskills.io/specification` (`name`,
`description` required; `name` lowercase-hyphen, equal to the folder). Symlinks: tested in a throwaway
home with `copilot skill list --json` and Codex app-server `skills/list`.

## 3. The portable format

### Where the files go

Claude Code's folders stay the real ones (Leon reads them too); the other CLIs
get symlinks.

| What | Real folder | Add | Read by |
|---|---|---|---|
| Your agents | `~/.claude/agents/` | `ln -s ~/.claude/agents ~/.copilot/agents` ⚠️ | Leon, Claude Code, Copilot CLI |
| Project agents | `.claude/agents/` | — | Leon, Claude Code, Copilot CLI |
| Your skills | `~/.claude/skills/` | `mkdir -p ~/.agents && ln -s ~/.claude/skills ~/.agents/skills` | Leon, all three CLIs |
| Project skills | `.claude/skills/` | `mkdir -p .agents && ln -s ../.claude/skills .agents/skills`, committed | Leon, all three CLIs |
| MCP servers | Leon's Connectors view | — | Every CLI inside Leon (stdio only) |

Avoid `.github/agents/` (Claude Code skips it), `~/.agents/agents/` (only Leon
reads it ⚠️) and subfolders (Leon skips them, Copilot renames). A source folder
fits a shared kit with its `.mcp.json`, but its agents and skills reach only
Claude Code, as `<folder>:<name>`.

**Codex without Leon** needs a copy, which can drift; skip it unless you run
Codex alone. `~/.codex/agents/reviewer.toml`:

```toml
name = "reviewer"
description = "Reviews a diff for bugs and missing tests."
developer_instructions = """
<the Markdown body>
"""
```

### Frontmatter

| Field | Leon | Claude Code alone | Copilot CLI alone | Do |
|---|---|---|---|---|
| `name` | Used, else filename | Identity | Display; id = filename | Keep; lowercase-hyphen, same as the filename |
| `description` | Shown in the agent list | Required; drives delegation | Required | Keep: one quoted line, what and when |
| `tools` with plain names (`Read, Grep, Bash`) | Ignored | Enforced | Documented aliases ⚠️ | Optional, to restrict outside Leon; also say it in the body |
| `tools` with `mcp__server__tool` or `Agent(x)`; `disallowedTools` | Ignored | Claude only | ⚠️ | Drop; say it in the body |
| `model` (`opus`, `sonnet`, `inherit`) | Ignored | Alias | ⚠️ | Drop |
| `skills` | Ignored | Preloads them | ⚠️ | Drop; name the skills in the body |
| `permissionMode`, `hooks`, `mcpServers`, `memory`, `color`, `effort`, `isolation`, `maxTurns` | Ignored | Claude only | ⚠️ | Drop, unless Claude Code alone needs it |
| Copilot-only (`target`, `mcp-servers`, `user-invocable`…) | Ignored | Ignored | Used | Keep only if needed |

Leon's parser reads one `key: value` per line: quote a description holding `: `.

### The body

1. **Self-contained**: Claude Code uses it *instead of* its system prompt;
   Codex and Copilot CLI add it to theirs.
2. **What, not which tool**: "search the code", "run the tests", "never edit
   files" — not `Grep`, `Bash`, `Write`. `TodoWrite`, `Task`, `Agent` exist in
   Claude Code only.
3. **MCP tools by server and tool**: "the `get_pull_request` tool of the
   `github` MCP server", not `mcp__github__get_pull_request`.
4. **Skills by name**: "follow the `code-style` skill".
5. **Other agents through Leon's tools**: every agent session gets an MCP server
   named `leon` ([leon-app-tools/src/lib.rs:37](../crates/leon-app-tools/src/lib.rs))
   with these tools ([tools.rs:24-99](../crates/leon-app-tools/src/tools.rs)):

| Tool | Arguments | Does |
|---|---|---|
| `start_agent` | `agent` (name as in its file), `prompt`, optional `working_directory` (absolute) | Starts the agent in a new agent session of this conversation; returns its id at once. The answer arrives later as a message starting `[From <agent> · session <id>]` |
| `message_agent` | `session` (integer), `text` | Follow-up to another agent session of this conversation; same answer format |
| `rename_conversation` | `title` | Renames the conversation |
| `set_working_directory` | `path` (absolute) | Tells Leon this session now works in another folder, e.g. a worktree |

`start_agent` ([agents.rs:65-131](../crates/leon-core/src/agents.rs)): the
agent must be in the project's list, else the error lists the names (:83-94);
it runs with the caller's CLI, model, effort and permission (:104-110), in the
conversation's folder unless `working_directory` is given (:95-103); it does
not see the caller's messages, so the prompt says everything.

Prefer it to the CLI's own subagents: those run inside one session and Leon
shows them only as counts ([claude.rs:160-161](../crates/leon-backends/src/claude.rs),
[codex.rs:332-336](../crates/leon-backends/src/codex.rs)); a `start_agent`
session has its own transcript and shows in the Flow. Portable sentence:

> To hand work to another agent, call Leon's `start_agent` tool if you have it
> (`agent: "<name>"`, a prompt that says everything), then wait for its
> `[From …]` message. Without Leon, use your CLI's own subagents, or do it
> yourself.

## 4. Example and checklist

Before, Claude Code only:

```markdown
---
name: reviewer
description: Use proactively after code changes to review the diff
tools: Read, Grep, Glob, Bash, mcp__github__get_pull_request
model: opus
skills:
  - code-style
color: purple
---
You are a senior reviewer. Track findings with TodoWrite. Fetch the PR with
mcp__github__get_pull_request. If tests changed, launch the test-runner
subagent with the Task tool. Never use Write or Edit.
```

After, portable:

```markdown
---
name: reviewer
description: "Reviews a diff for bugs, risky changes and missing tests. Use after code changes."
---
You review code changes. You never edit files.

1. Read the diff and the code around it. Follow the `code-style` skill.
2. For a pull request, read it with the `get_pull_request` tool of the
   `github` MCP server, if you have it.
3. If tests changed, hand them to the `test-runner` agent: call Leon's
   `start_agent` tool if you have it (`agent: "test-runner"`, a prompt that
   says everything), then wait for its `[From …]` message. Without Leon, use
   your CLI's own subagents, or run the tests yourself.
4. Answer with findings: file:line, what breaks, the fix.
```

Migrate a folder:

1. Back it up: `cp -R ~/.claude/agents ~/agents-backup`.
2. Flatten: no subfolders; each file `<name>.md`, lowercase-hyphen.
3. Each file starts with `---`; keep `name` and a quoted one-line `description`.
4. Frontmatter table: drop `model`, `skills`, Claude-only keys; keep `tools`
   (plain names) only if needed outside Leon.
5. Body: plain verbs; `Task` / `Agent` / "subagent" → the sentence above;
   `mcp__server__tool` → server and tool; skills by name.
6. Add the MCP servers the agents need in Leon's Connectors view.
7. Create the symlinks of the "Where the files go" table, where the target
   does not exist yet.
8. Leftovers: `grep -nE 'mcp__|Task|TodoWrite|^model:' ~/.claude/agents/*.md`.
9. Check: restart Leon, see the agents in the new-conversation form, start one
   on each CLI; `claude --agent <name>`; restart Copilot CLI, `/agent` and
   `copilot skill list`; `/skills` in Codex.

## Not verified

- Copilot CLI follows a symlinked `~/.copilot/agents/` (tested for skills only).
- Copilot CLI 1.0.113 over ACP advertises `sessionCapabilities.systemPrompt`,
  i.e. which way Leon sends the body (Leon never ran it,
  [acp.rs:4-5](../crates/leon-backends/src/acp.rs)).
- Copilot CLI with `model: opus`, a `tools` list of only `mcp__…` names, and
  unknown keys (`skills`, `color`…). The Claude tool-name aliases are
  documented for custom agents in general, not for the CLI.
- Copilot CLI loads a project's `.claude/agents/` only in a trusted folder.
- Codex: `developerInstructions` adds to, or replaces, its base prompt.
- `/name` from Leon's `/` menu runs a skill in Codex (its docs use `$name`),
  and a source-folder skill in Claude Code (named `/<folder>:<name>`).
- Claude Code reads `~/.agents/skills/` (string in its binary, not in its
  docs) and follows symlinked folders (the layout above does not need it).
- No CLI reads `~/.agents/agents/` or `.agents/agents/`: no docs mention it;
  for Codex the string is absent from its binary.
