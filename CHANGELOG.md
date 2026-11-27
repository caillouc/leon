# What's new in Leon

Each release, what changed for you: one short line per change, the newest
release first.

## v0.9.4

- **Fable usage**: Doctor shows Claude Code's Fable weekly limit as soon as it runs, without a Fable turn first.
- **Copilot usage**: Doctor says why Copilot CLI has no usage bars (it counts premium requests).
- **To do page**: each flow's header shows "x/y done" and an "Open flow" button; its mark takes its pull requests' colour.
- **To-do age**: on the To do page, a pill shows how long each to-do has been open, amber from 2 days, red from a week.
- **Settings**: Diffs' fold patterns show as chips, each removed by its cross, added from a box under them and saved at once.

## v0.9.3

- **Errors**: a conversation whose last turn failed still shows its error after Leon restarts, in its flow and the left pane.

## v0.9.2

- **Commands**: an MCP server's tools count as one in an answer's folded line ("Slack ×3").

## v0.9.1

- **Models**: each CLI's models come from the CLI itself, so a new one (like Fable 5.2) shows without a Leon update.
- **Model picker**: only the newest version of each model, plus the one in use; the effort picker offers that model's own levels.
- **Zed**: "Open in Zed" opens the file in the worktree's own project, in the window already open.
- **Images**: images in a message you are writing stay when you switch conversation.

## v0.9.0

- **Commands**: an answer's commands gather in one grey line at its top; opened, each says what it was for.
- **Full commands**: opening a command shows it whole, not cut to the line.
- **Queued messages**: a Queue button beside Stop; each queued message shows on its own line, can be edited in place, and all go as one.
- **Restarts**: queued messages and agents waiting for each other are kept when Leon restarts.
- **New conversation**: the agent, CLI and model you pick stay while you look at another conversation.
- **TLDR**: a line starting with TLDR stands out in green.

## v0.8.6

- **Projects**: a project's flow count ends its row; on hover it moves left of the +.

## v0.8.5

- **To-dos**: the To do page is lighter: no "Added by" line, no "Open flow" link (a flow's title opens it).

## v0.8.4

- **Agents**: an agent can start a conversation without an agent file, as you can from the new-conversation form.
- **Zed**: a file opens in its worktree's own workspace, not in the main checkout's window.
- **Links**: a link to a file on your Mac, like a page an agent made, opens it.

## v0.8.3

- **Tabs**: a flow shows tabs only for the agents you talk with or opened, each with a cross to close it.

## v0.8.2

- **To-dos**: ticking or removing one happens at once, with an Undo at the bottom right.

## v0.8.1

- **Archive**: sending a message in an archived flow takes it out of the archive.
- **Projects**: a project without flows shows folded.

## v0.8.0

- **Mark as unread**: one click in a flow's title bar, for the conversation shown or the whole flow.

## v0.7.1

- **CI checks**: a pull request's checks count each job once, as GitHub shows them.

## v0.7.0

- **Queued messages**: write while the agent works; your message goes when it finishes.
- **Open in your editor**: open a worktree or a file in Zed or VS Code (pick it in Settings).
- **Usage**: Doctor remembers each CLI's usage and shows how old it is.
- **Cost**: shown only for a CLI on an API key, no longer on a subscription.
- **Diffs**: more room for code, a wider comment box, a resizable file list.
- **Cleaner screens**: right pane on flows only, smaller bottom buttons, fewer built-in connectors.
