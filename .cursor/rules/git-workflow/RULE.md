---
description: "Git workflow standards: proper commit messages, pre-commit checks, and git worktree usage for independent feature work"
alwaysApply: true
---

# Git Workflow and Code Quality Standards

## Git Commit Requirements

**Always commit files with proper, descriptive git log messages.**

### Commit Message Format

Follow conventional commit format:
- **Type**: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`
- **Scope**: Optional, indicates the area affected
- **Description**: Clear, imperative mood description (e.g., "add feature" not "added feature")
- **Body**: Optional, detailed explanation (separated by blank line)
- **Footer**: Optional, references to issues

Example:
```
feat(websocket): add reconnection logic

Implement exponential backoff retry mechanism for WebSocket connections
to handle network interruptions gracefully.

Fixes #123
```

### Before Committing

**ALWAYS run these checks before committing code:**

1. **Format check**: `just format-check` or `cargo fmt --check`
2. **Lint check**: `just lint` or `cargo clippy --all-targets --all-features -- -D warnings`
3. **Tests**: `cargo test`

All checks must pass before creating a commit. Fix any issues found before proceeding.

### Commit Process

When committing changes:
1. Stage changes with `git add`
2. Run `just format-check`, `just lint`, and `cargo test`
3. If all checks pass, commit with a proper message
4. Push to the appropriate branch

## Git Worktree Usage for Independent Feature Work

**Use git worktrees to work independently on different features, one per agent/session.**

Each AI agent session should operate in its own git worktree to avoid conflicts and maintain isolation.

### Creating a Worktree

When starting work on a new feature:

```bash
# Create a new branch and worktree for the feature
git worktree add ../polymarket-tui-feature-name -b feature/feature-name

# Navigate to the worktree
cd ../polymarket-tui-feature-name
```

### Worktree Workflow

1. **Create worktree**: Use descriptive names based on the feature being worked on
2. **Work independently**: Each worktree operates independently with its own checkout
3. **Commit within worktree**: Follow commit standards within the worktree context
4. **Merge when ready**: When feature is complete, merge the branch back to main from the main repository

### Cleaning Up Worktrees

```bash
# Remove a worktree when feature is complete
cd /Users/penso/tmp/polymarket-tui
git worktree remove ../polymarket-tui-feature-name
```

## Code Quality Checklist

Before committing, ensure:

- [ ] Code is formatted (`just format-check` passes)
- [ ] Code passes clippy linting (`just lint` passes)
- [ ] All tests pass (`cargo test`)
- [ ] Commit message follows conventional commit format
- [ ] Changes are logically grouped in the commit
- [ ] No debug code or temporary files are included

## Justfile Commands Reference

Use these commands from the justfile:
- `just format` - Format code
- `just format-check` - Check formatting without modifying
- `just lint` - Run clippy linting
- `just build` - Build project
- `just build-release` - Build in release mode

