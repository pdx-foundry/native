---
name: watch-prs
description: Watch a GitHub PR or set of PRs and get notified when Codex posts a
  review. The script polls for you and will not wake you until the review
  terminates.
---

# Watch PRs

When you submit a PR that is ready for review (not a draft), a Codex review automatically starts. This repository has a tool to help you monitor the review. The script will notify you when the review is complete and what the review inline comments are. After submitting, run from the repository root:

```sh
node scripts/watch-codex-reviews.mjs "owner/repository#123"
```

You can also pass full PR URLs or watch several PRs:

```sh
node scripts/watch-codex-reviews.mjs \
  https://github.com/owner/repository/pull/123 \
  "owner/repository#124"
```

The script requires an authenticated `gh` CLI. It polls every 30 seconds, prints only status changes and findings for the current PR head, and exits when all reviews complete or fail. Tailing/piping the script output is unnecessary; use the script as-is.

When submitting revisions, do not watch for additional reviews. Codex reviews a PR once.

Options:

- `--interval <seconds>` changes the polling interval.
- `--once` checks each PR once and exits.
- `--help` prints command usage.

## With PR Stacks

When working with stacked PRs, wait until the entire stack is complete and ready for review before running the script. The script terminates when every PR you supply has a completed review, so you will get all of the reviews at once. As a reminder, Codex only reviews *ready* PRs, not drafts.