#!/usr/bin/env zsh

typeset -gi DGO_DEMO_SCENE=0

function dirgo-demo-chapter() {
  (( DGO_DEMO_SCENE += 1 ))
  zle -I
  print -n $'\e[2J\e[H'
  print -P '  %F{#30D158}%BDIRGO 0.6%b%f'

  case $DGO_DEMO_SCENE in
    1)
      print -P '  %F{#F5F5F7}%BProject scripts%b%f'
      print -P '  %F{#8E8E93}package.json · 6 local scripts · Tab inserts%f'
      ;;
    2)
      print -P '  %F{#F5F5F7}%BGit command catalog%b%f'
      print -P '  %F{#8E8E93}25 commands · live descriptions · zero execution%f'
      ;;
    3)
      print -P '  %F{#F5F5F7}%BCargo + project context%b%f'
      print -P '  %F{#8E8E93}PROJ first · global catalog behind it · instant lookup%f'
      ;;
    *)
      print -P '  %F{#F5F5F7}%BReady for your next command%b%f'
      print -P '  %F{#8E8E93}Local by design · fast by default · control stays yours%f'
      ;;
  esac

  print
  zle reset-prompt
}

zle -N dirgo-demo-chapter
bindkey '^G' dirgo-demo-chapter
