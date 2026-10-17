function __cmdnavi_complete
    set -l words (commandline -opc)
    set -l raw (commandline -ct)
    set -l current (string unescape -- "$raw")
    if test $status -ne 0
        set current $raw
    end
    set -l position (math (count $words) - 1)
    command cmdnavi --complete fish $position -- $words[2..-1] "$current" 2>/dev/null
end
complete -c cmdnavi -f -a '(__cmdnavi_complete | string split0)'
