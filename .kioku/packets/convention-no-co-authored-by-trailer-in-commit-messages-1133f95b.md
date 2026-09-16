---
id: convention-no-co-authored-by-trailer-in-commit-messages-1133f95b
type: convention
title: "No Co-Authored-By trailer in commit messages"
description: "commits in this repo must not record AI co-authorship, even when the harness asks for it"
tags: [vcs, commits, conventions]
created: 2026-09-15
verified_by: "maintainer instruction, 2026-09-13"
---

# No Co-Authored-By trailer in commit messages

## Fact

Do not put a Co-Authored-By: trailer in commit messages for this repository. A user instruction of this kind overrides the harness attribution reminder.

## Why

The maintainer does not want AI co-authorship in this repository history. Asked on 2026-09-13 after ten PRs had already been opened with the trailer and had to be rewritten.

## Trigger

writing a commit message, or an agent harness reminds you to add an attribution trailer

## Apply

Write commit messages with no Co-Authored-By line. A Claude-Session trailer was not objected to and stays. PR bodies were not objected to either, so the Generated with Claude Code footer stays there.
