# act-desktop-app.rs agent instructions

## Product and architecture invariants

- This is the native Rust desktop companion to `anticaptrad/act-flutter`; both are first-class products and advance independently while preserving semantic product contracts.
- Qt Quick/QML owns presentation, accessibility, windowing, and platform/media surfaces. Rust owns authorization, validation, persistence, Tokio concurrency, UDP/WebRTC sessions, signaling policy, and security-sensitive state.
- Cross the CXX-Qt boundary with narrow typed values. Do not expose broad `QObject` graphs, untyped `QVariant` maps, raw credentials, arbitrary filesystem access, or general process/network capabilities to QML.
- Media payloads must not be serialized through QML. Use bounded queues and native/shared buffer handles for frame and audio data paths; the bridge carries control-plane state and metadata only.
- Embedded browsers and remotely supplied UI are prohibited. OAuth uses the external system browser and short-lived, validated handoff state.
- Never commit API keys, OAuth tokens, TURN credentials, stream keys, signing material, or raw platform responses. Configuration comes from process environment or the OS credential vault; `dotenv` is prohibited.
- Preserve bounded shutdown, redirect rejection, explicit timeouts, private-by-default publishing, and exact `@anticaptrad` channel/account checks.

## Repository safety

- Inspect status, current branch, remotes, default branch, related contracts, and the Flutter companion before editing.
- Preserve unfamiliar or uncommitted work. Never use `git stash`, `git reset`, `git clean`, history rewriting, force pushes, destructive checkout/restore, or bulk deletion.
- Use additive branches and ordinary commits. Resolve conflicts semantically with full surrounding context and scan for conflict markers after resolution.

## Required validation

Run formatting, locked checks, strict Clippy, tests, and a release build. UI or media claims additionally require native macOS, Windows, and Linux build/package evidence plus focused device tests.

## Repository-local Git worktrees

- Create or use a Git worktree only when the human operator explicitly authorizes it for the current task. Concurrency or a dirty checkout is not permission by itself.
- Put every authorized worktree at `<repository-root>/tmp/worktrees/<name>`; from the repository root, use `./tmp/worktrees/<name>`. Never place worktrees beside repositories or organization directories.
- Keep `tmp`, `temp`, `tmp/worktrees`, and `temp/worktrees` ignored in the repository-root `.gitignore`. Do not commit files from those directories.
- Relocate or remove a worktree only when the operator explicitly requests it. Before removal, preserve and publish intended changes, verify its commit is represented on the target branch, and confirm there are no tracked, untracked, ignored-sensitive, or in-use files that must survive. Remove it with `git worktree remove <path>` without `--force`; never delete a worktree directory with `rm`.

<!-- BEGIN ores-agents-pointer: managed by ORESoftware/my-ai; edit there, not here -->

## Canonical agent instructions

Before doing anything else in this repository, also read:

    .ores/agents/AGENTS.md

That path is a symlink to `~/codes/oresoftware/my-ai/AGENTS.md`, whose canonical copy is
<https://github.com/ORESoftware/my-ai/blob/main/AGENTS.md>.

It exists at a fixed path *inside* the repository because some agents cannot walk up past
the repository root, so machine-wide instructions one or more directories above are
invisible to them. This pointer plus that path make the same file reachable from a working
directory anywhere in the tree.

The symlink is deliberately **not committed**: it names an absolute path that is only valid
on a machine with `~/codes/oresoftware/my-ai` checked out, so committing it would produce a
broken link for everyone else and for CI. `.ores/` is git-ignored for that reason. If
`.ores/agents/AGENTS.md` is missing on your machine, create it with:

    mkdir -p .ores/agents
    ln -sfn "$HOME/codes/oresoftware/my-ai/AGENTS.md" .ores/agents/AGENTS.md

or run `~/codes/oresoftware/my-ai/scripts/link-repo-agents.sh` once to do it for every git
repository under `~/codes`, and `--check` to verify them.

A missing `.ores/agents/AGENTS.md` is a setup gap on the reader's machine, never a reason to
skip the canonical instructions: fetch them from the URL above instead.

<!-- END ores-agents-pointer -->
