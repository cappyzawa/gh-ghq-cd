# gh-ghq-cd

## Requires

- [`gh`](https://github.com/cli/cli) v2.0.0+
- [`ghq`](https://github.com/x-motemen/ghq)
- [`fzf`](https://github.com/junegunn/fzf)
- (Optional) [`bat`](https://github.com/sharkdp/bat)

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

# If you want to open the selected repository with tmux new window
gh cd -nw
```
