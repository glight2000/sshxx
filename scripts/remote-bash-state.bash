# Source at the end of an interactive remote Bash startup file.
# The saved SSH profile forwards one opaque namespace per terminal window.
[[ $- == *i* && ${SSHXX_HISTORY_ID-} =~ ^sshxx-[a-zA-Z0-9]{20}$ ]] || return 0
sshxx_state_dir=${XDG_STATE_HOME:-"$HOME/.local/state"}/sshxx/"$SSHXX_HISTORY_ID"
unset SSHXX_HISTORY_ID
(umask 077 && mkdir -p -- "$sshxx_state_dir") || return 0
[[ -O $sshxx_state_dir && ! -L $sshxx_state_dir ]] || return 0
chmod 700 -- "$sshxx_state_dir" || return 0

if [[ -o history && -n ${HISTFILE-} && $HISTFILE != /dev/null ]]; then
  HISTFILE=$sshxx_state_dir/history
  shopt -s histappend
  if [[ ! ${HISTFILESIZE-} =~ ^[0-9]+$ ]] || (( HISTFILESIZE > 10000 )); then
    HISTFILESIZE=10000
  fi
fi

# "Open terminal here" supplies an explicit directory and takes precedence.
if [[ -z ${SSHXX_INITIAL_DIRECTORY-} && -f $sshxx_state_dir/cwd ]] &&
  IFS= read -r -d '' sshxx_saved_cwd < "$sshxx_state_dir/cwd" &&
  [[ $sshxx_saved_cwd == /* && -d $sshxx_saved_cwd ]]; then
  builtin cd -- "$sshxx_saved_cwd" 2>/dev/null || :
fi
unset sshxx_saved_cwd sshxx_last_cwd

sshxx_save_shell_state() {
  local status=$?
  if [[ -o history && ${HISTFILE-} == "$sshxx_state_dir/history" ]]; then
    builtin history -a
  fi
  if [[ $PWD != "${sshxx_last_cwd-}" ]]; then
    local tmp=$sshxx_state_dir/cwd.$$.tmp
    if (umask 077 && printf '%s\0' "$PWD" > "$tmp" && mv -f -- "$tmp" "$sshxx_state_dir/cwd"); then
      sshxx_last_cwd=$PWD
    fi
  fi
  return "$status"
}

if (( BASH_VERSINFO[0] > 5 || (BASH_VERSINFO[0] == 5 && BASH_VERSINFO[1] >= 1) )); then
  PROMPT_COMMAND+=(sshxx_save_shell_state)
else
  PROMPT_COMMAND=${PROMPT_COMMAND:+"$PROMPT_COMMAND"$'\n'}sshxx_save_shell_state
fi
