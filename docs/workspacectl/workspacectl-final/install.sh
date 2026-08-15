#!/usr/bin/env bash
set -euo pipefail
mkdir -p ~/.local/bin
cp -a workspacectl.sh ~/.local/bin/workspacectl
chmod +x ~/.local/bin/workspacectl
echo "Installed to ~/.local/bin/workspacectl"
