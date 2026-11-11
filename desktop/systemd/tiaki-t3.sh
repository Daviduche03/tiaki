#!/bin/sh
# Start the T3 Code fork as the desktop user.
#
# The extra-MCP variable is why this wrapper exists instead of calling the
# dev runner directly: T3_ACP_EXTRA_MCP_SERVERS names the OS capability
# server (`agentd mcp serve`) so every provider session — generic ACP and
# native OpenCode alike — gets the tiaki tools. Values are concrete
# paths, not {env:} references: child servers resolve those against their
# own environment, which does not carry ours.
set -eu

export HOME=/home/desktop
export T3CODE_HOME=/home/desktop/.t3
export TIAKI_HOME=/home/desktop/.tiaki
export TIAKI_PRINCIPAL=user:ada
export TIAKI_WORKSPACE=/home/desktop/.tiaki/workspace
export T3_ACP_EXTRA_MCP_SERVERS='[{"name":"tiaki","command":"/usr/local/bin/agentd","args":["mcp","serve"],"env":[{"name":"TIAKI_HOME","value":"/home/desktop/.tiaki"},{"name":"TIAKI_PRINCIPAL","value":"user:ada"},{"name":"TIAKI_WORKSPACE","value":"/home/desktop/.tiaki/workspace"}]}]'

mkdir -p "$TIAKI_HOME/workspace"
cd /opt/t3code-upstream
exec ./node_modules/.bin/vp run dev
