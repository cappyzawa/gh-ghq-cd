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

### Nix (with home-manager)

Add to your `flake.nix` inputs:

```nix
inputs.gh-ghq-cd = {
  url = "github:cappyzawa/gh-ghq-cd";
  inputs.nixpkgs.follows = "nixpkgs";
};
```

Then use with `programs.gh.extensions`:

```nix
programs.gh = {
  enable = true;
  extensions = [
    inputs.gh-ghq-cd.packages.${system}.gh-ghq-cd
  ];
};
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

### Terminal Multiplexer Integration

When running inside a terminal multiplexer ([tmux](https://github.com/tmux/tmux), [zellij](https://github.com/zellij-org/zellij) or [herdr](https://herdr.dev)), you can use additional options. The tool automatically detects your environment via the `HERDR_ENV`, `TMUX` or `ZELLIJ` environment variables.

```bash
# Open in new window (tmux) / tab (zellij) / workspace (herdr)
gh cd -w

# Open in new pane (vertical split, default)
gh cd -p

# Open in new pane with 2 sub-panes (vertical + horizontal split)
gh cd -p 2

# Open in new window/tab/workspace split into 2 panes
gh cd -w -p 2
```

> [!NOTE]
> A new window/tab/workspace already opens with one pane, so `-w -p` does the
> same as `-w`. Use `-w -p 2` to split it.

#### Pane Split Direction

```bash
# Vertical split (default)
gh cd -p -V

# Horizontal split
gh cd -p -H

# With 2 sub-panes
gh cd -p 2 -V    # vertical + top/bottom
gh cd -p 2 -H    # horizontal + left/right

# New window/tab/workspace split left/right
gh cd -w -p 2 -H
```

#### Run Command in New Pane/Window

```bash
# Run a command in the new pane/window
gh cd -w -c "claude"
gh cd -p -c "npm run dev"
```

> [!NOTE]
> `-c` cannot be used with `-p 2` (multiple panes)

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
