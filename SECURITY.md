# Security policy

## Supported versions

Only the latest release on the `main` branch receives fixes.

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Report it privately through GitHub's
[private vulnerability reporting](https://github.com/pungoyal/poker/security/advisories/new).
Include the version or commit, steps to reproduce and the impact you expect.

You can expect an acknowledgement within a few days. Once a fix is ready we will
publish an advisory and credit you, unless you prefer to stay anonymous.

Relevant areas include parsing of hand-history files (`replay`, `leaks`,
`drill`), the LLM coach's handling of your API key and of the network responses
it streams, the Docker images, and the CI and release workflows.

## Your API key

The LLM coach reads `ANTHROPIC_API_KEY` (or `ANTHROPIC_AUTH_TOKEN`) from the
environment and sends it only to the Anthropic API (or to `ANTHROPIC_BASE_URL`
if you set one). It is never written to disk or to hand histories. Each
request carries the current hand's cards, actions and coaching conversation.
