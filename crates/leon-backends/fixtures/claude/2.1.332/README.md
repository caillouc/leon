# Claude Code 2.1.332

`initialize.jsonl`: the answer to an `initialize` control request over
stream-json (`claude -p --input-format stream-json --output-format
stream-json --verbose`), recorded on 2026-11-26. Changes from the recording:
only its `models` are kept (the account, agents and commands are left out);
the request id is `leon`.

`rate-limit-fable.jsonl`: the `rate_limit_event` of a one-line Fable turn
(`claude -p --model fable --output-format stream-json --verbose`), recorded on
2026-11-27: its windows include `seven_day_overage_included`, which Claude
Code labels "Fable limit". Changes from the recording: the uuid and session
id are zeros.

`get-usage.jsonl`: the answer to a `get_usage` control request, sent the same
way with `--no-session-persistence`, recorded on 2026-11-27: no turn runs.
Changes from the recording: only `rate_limits.limits` is kept (the account's
plan, spend and tips are left out); the request id is `leon`.
