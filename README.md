# gh-ghq-cd

## Requires

- [`gh`](https://github.com/cli/cli) v2.0.0+
- [`ghq`](https://github.com/x-motemen/ghq)
- [`fzf`](https://github.com/junegunn/fzf)

### Optional (for README preview)

- [`bat`](https://github.com/sharkdp/bat) (recommended)
- `cat` (fallback)

## How to install

```bash
gh extension install https://github.com/cappyzawa/gh-ghq-cd

# Upgrade
gh extension upgrade gh-ghq-cd
```

### (Optional) Recommended Setting

```bash
gh alias set cd ghq-cd
```

## How to use

```bash
gh ghq-cd

# If you set "cd" as an alias for ghq-cd
gh cd
```

### Tmux Integration

When running inside tmux, you can use additional options:

```bash
# Open in new tmux window
gh cd -w

# Open in new pane (vertical split, default)
gh cd -p

# Open in new pane with 2 sub-panes (vertical + horizontal split)
gh cd -p 2
```

#### Pane Split Direction

```bash
# Vertical split (default)
gh cd -p -V

# Horizontal split
gh cd -p -H

# With 2 sub-panes
gh cd -p 2 -V    # vertical + top/bottom
gh cd -p 2 -H    # horizontal + left/right
```

#### Layout Examples

**`gh cd -p` (vertical split)**
```
┌─────────┬─────────┐
│ existing│   new   │
│  pane   │  pane   │
└─────────┴─────────┘
```

**`gh cd -p 2` (vertical + 2 sub-panes)**
```
┌─────────┬─────────┐
│ existing│   new   │ ← focus here
├─────────┼─────────┤
│         │   new   │
└─────────┴─────────┘
```

**`gh cd -p -H` (horizontal split)**
```
┌───────────────────┐
│   existing pane   │
├───────────────────┤
│     new pane      │
└───────────────────┘
```

**`gh cd -p 2 -H` (horizontal + 2 sub-panes)**
```
┌───────────────────┐
│   existing pane   │
├─────────┬─────────┤
│   new   │   new   │
│ ← focus │         │
└─────────┴─────────┘
```
