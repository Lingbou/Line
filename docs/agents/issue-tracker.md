# Issue tracker: GitHub

Issues, specs, and agent-ready tickets for this repo live in GitHub Issues for `Lingbou/Line`. Use the `gh` CLI for tracker operations.

## Conventions

- **Create an issue**: `gh issue create --title "..." --body-file <file> --label v0.2 --label ready-for-agent`
- **Read an issue**: `gh issue view <number> --comments`
- **List issues**: `gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'`
- **Comment on an issue**: `gh issue comment <number> --body "..."`
- **Apply or remove labels**: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **Close**: `gh issue close <number> --comment "..."`

Infer the repository from `git remote -v`; `gh` does this automatically when run inside a clone.

## Pull requests as a triage surface

**PRs as a request surface: no.**

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Run `gh issue view <number> --comments`.

## Worktree rule

Implementation work happens in a Git worktree under `.temp/<name>`. Branch names must be human-oriented and must not contain `codex` or other AI-identifying prefixes. All commits use both sign-off and GPG signing: `git commit -s -S`.
