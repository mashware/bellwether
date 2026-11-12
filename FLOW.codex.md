# FLOW.codex.md

Sparse overlay for Codex, read after `FLOW.md` and any selected `FLOW_CONTEXT` layer.
Absent keys inherit; present-but-empty keys mask an earlier value; lists replace whole lists.
Configuration keys and defaults: the installed plugin's `examples/FLOW.template.md`.

## quality

The base's `pr-review-toolkit:*` agents are Claude Code plugins, unavailable in Codex.
Mask both panel selectors to use Flow's built-in, stack-agnostic review, which reads this
repository's conventions and sensitive paths. The review artifact must say "built-in only".
Do not substitute `code-review-vioque`: its PHP/Symfony specialists fit neither Rust nor Herdr mods.

- reviewers:
- review_skill:

## agents

Keep review inside the agentic session; the base's four-agent fan-out and twelve-agent total budget
still apply.

- exec_cmd:

## models

These select improvised subagents, bounded fan-out rounds and delegated waits respectively.
They do not change the main session's model or a named specialist's own model.
Match Claude's role split using the user's equivalence: Opus maps to Sol, Sonnet to Terra.

- agents: gpt-6.1-sol
- workers: gpt-6.1-terra
- supervisors: gpt-6.1-terra

## panel

Mask the optional claude.ai artifact mirror: Codex has no matching artifact integration here.
The live panel still goes through the `bellwether` MCP inside a Herdr console.

- artifact:
