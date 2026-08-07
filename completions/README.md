# Shell completion for `--profile`

Tab-completion of `--profile` values (reads `~/.aws/config` +
`~/.aws/credentials` directly; no `aws` command is run).

zsh:

```sh
mkdir -p ~/.zsh/completions && cp completions/_smew ~/.zsh/completions/
# in ~/.zshrc, before compinit:  fpath=(~/.zsh/completions $fpath)
```

bash:

```sh
cp completions/smew.bash ~/.smew-completion.bash
echo 'source ~/.smew-completion.bash' >> ~/.bashrc
```
