# ACP fixtures (schema v1.32.0, protocol version 1)

These lines come from the ACP spec's own examples (agentclientprotocol.com and
the `schema/v1` folder of agentclientprotocol/agent-client-protocol, checked on
2026-11-17), not from a recorded CLI: Copilot CLI was not run. Changes from the examples: ids follow the order Leon sends requests in,
`cancel.jsonl` ends with the `cancelled` stop reason the spec describes,
`new-only.jsonl` and `system-prompt.jsonl` trim `agentCapabilities` to the
capability under test, and `config.jsonl` keeps one `session/set_config_option` response: the
example's, with the model changed.
