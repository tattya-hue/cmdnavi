# Load with: source <(cmdnavi --completions bash)
_cmdnavi_complete() {
    local candidate
    COMPREPLY=()
    while IFS= read -r -d '' candidate; do
        [[ -n $candidate ]] && COMPREPLY+=("$candidate")
    done < <(command cmdnavi --complete bash "$((COMP_CWORD - 1))" -- "${COMP_WORDS[@]:1}" 2>/dev/null)
}
complete -o nosort -o noquote -F _cmdnavi_complete cmdnavi
