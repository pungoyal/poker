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

Relevant areas include the page's handling of stored and imported hands, the
LLM coach's handling of your API key and of the network responses it streams,
the Docker image, and the CI and deploy workflows.

## Your API key

The LLM coach is optional. The key you enter in Settings is kept in your
browser's local storage for the site and sent only to `api.anthropic.com`,
straight from the page; there is no server in between. Anyone with access to
your browser profile can read it, so use a key you can revoke, and clear it in
Settings on a shared device. It is never written to hand histories or exports.
Each request carries the current hand's cards, actions and coaching
conversation.
