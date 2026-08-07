# bash completion for smew.
#
# Install:
#   cp completions/smew.bash ~/.smew-completion.bash
#   echo 'source ~/.smew-completion.bash' >> ~/.bashrc
#
# --profile completes from ~/.aws/config and ~/.aws/credentials directly
# (no aws command is run).

_smew_profiles() {
  local f cfg="${AWS_CONFIG_FILE:-$HOME/.aws/config}"
  local cred="${AWS_SHARED_CREDENTIALS_FILE:-$HOME/.aws/credentials}"
  for f in "$cfg" "$cred"; do
    [[ -r "$f" ]] || continue
    sed -n -E 's/^\[profile[[:space:]]+(.+)\]$/\1/p; s/^\[([^]]+)\]$/\1/p' "$f"
  done
}

_smew() {
  local cur prev
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD-1]}"

  case "$prev" in
    --profile)
      COMPREPLY=($(compgen -W "$(_smew_profiles)" -- "$cur"))
      return
      ;;
    --region)
      COMPREPLY=($(compgen -W "us-east-1 us-east-2 us-west-1 us-west-2 \
        ap-northeast-1 ap-northeast-2 ap-northeast-3 \
        ap-southeast-1 ap-southeast-2 ap-south-1 ap-east-1 \
        eu-west-1 eu-west-2 eu-west-3 eu-central-1 eu-north-1 \
        ca-central-1 sa-east-1" -- "$cur"))
      return
      ;;
  esac

  COMPREPLY=($(compgen -W "--profile --region --dry-run --dev -h --help" -- "$cur"))
}

complete -F _smew smew
complete -F _smew ./target/release/smew
