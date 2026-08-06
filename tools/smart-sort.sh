#!/usr/bin/env bash
#
# smart-sort — Intelligent Downloads Sorter
# Watches ~/Downloads and automatically moves files to correct locations.
#
set -euo pipefail

USER_HOME="${HOME:-/home/gagan}"
DOWNLOADS="${USER_HOME}/Downloads"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info()  { echo -e "${BLUE}[SORT]${NC} $*"; }
log_ok()    { echo -e "${GREEN}[OK]${NC}   $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_move()  { echo -e "${GREEN}[MOVE]${NC} $*"; }

DRY_RUN=false

# =============================================================================
# Classification rules
# =============================================================================
# Format: "ext1,ext2,...:destination:subfolder"
# First match wins. Lowercase extensions only.

declare -a RULES=(
    # Videos
    "mkv,mp4,avi,mov,wmv,flv,webm:Videos:Movies"
    
    # Images
    "jpg,jpeg,png,gif,bmp,svg,webp,ico,heic,raw:Pictures"
    
    # Documents
    "pdf,doc,docx,txt,rtf,odt,md,epub,mobi:Documents"
    "xls,xlsx,csv,ods,ppt,pptx,odp:Documents"
    
    # Archives
    "zip,tar,gz,bz2,xz,7z,rar,tgz:Archives"
    
    # ISOs
    "iso,img,dmg:Archives/ISOs"
    
    # Music
    "mp3,flac,wav,aac,ogg,m4a,wma:Music"
    
    # Installers
    "deb,rpm,AppImage,pkg,msi,exe:Archives/Installers"
    
    # Torrents
    "torrent:Downloads/torrents"
    
    # Code/projects
    "py,js,ts,jsx,tsx,java,c,cpp,h,rs,go,rb,php,sh,bash,sql,html,css,json,yaml,yml,toml,lock:Projects"
    
    # AI models
    "bin,safetensors,gguf,pth,h5,pb:Models"
    
    # Data
    "db,sqlite,sqlite3:Datasets"
)

# Files to NEVER move
PROTECTED_PATTERNS=(
    "ssh-backup.tar.gz"
    "cachyos-desktop-linux-260628.iso"
    "*.part"
    "*.crdownload"
    "*.tmp"
    ".DS_Store"
    "Thumbs.db"
)

is_protected() {
    local filename="$1"
    for pattern in "${PROTECTED_PATTERNS[@]}"; do
        # Simple glob match
        if [[ "$filename" == $pattern ]]; then
            return 0
        fi
    done
    return 1
}

classify_file() {
    local filepath="$1"
    local filename
    filename="$(basename "$filepath")"
    local ext="${filename##*.}"
    local basename="${filename%.*}"
    
    [[ -d "$filepath" ]] && return 1
    [[ "$filename" == .* ]] && return 1
    is_protected "$filename" && return 1
    
    # Skip very recent files (< 60s old) — might still be downloading
    local mtime
    mtime=$(stat -c %Y "$filepath" 2>/dev/null || echo 0)
    local now
    now=$(date +%s)
    [[ $((now - mtime)) -lt 60 ]] && return 1
    
    local lower_ext="${ext,,}"
    for rule in "${RULES[@]}"; do
        local rule_exts="${rule%%:*}"
        local dest="${rule##*:}"
        local subfolder="${dest##*:}"
        dest="${dest%%:*}"
        
        # Check extension match
        for rext in ${rule_exts//,/ }; do
            if [[ "$lower_ext" == "$rext" ]]; then
                echo "${dest}:${subfolder}"
                return 0
            fi
        done
    done
    
    return 1
}

move_file() {
    local src="$1"
    local dest_dir="$2"
    local filename
    filename="$(basename "$src")"
    
    mkdir -p "${dest_dir}"
    
    local dest="${dest_dir}/${filename}"
    
    # Handle duplicates
    if [[ -e "$dest" ]]; then
        local base="${filename%.*}"
        local ext="${filename##*.}"
        local counter=1
        while [[ -e "${dest_dir}/${base}_${counter}.${ext}" ]]; do
            ((counter++))
        done
        dest="${dest_dir}/${base}_${counter}.${ext}"
    fi
    
    if [[ "${DRY_RUN}" == "true" ]]; then
        log_move "[DRY RUN] ${src} → ${dest}"
    else
        mv "$src" "$dest"
        log_move "${src} → ${dest}"
    fi
}

sort_once() {
    log_info "Sorting Downloads..."
    local count=0
    local moved=0
    
    for file in "${DOWNLOADS}"/*; do
        [[ -e "$file" ]] || continue
        ((count++))
        
        local classification
        classification=$(classify_file "$file") || continue
        
        local dest="${USER_HOME}/${classification%%:*}"
        local subfolder="${classification##*:}"
        
        [[ -n "$subfolder" && "$subfolder" != "$dest" ]] && dest="${dest}/${subfolder}"
        move_file "$file" "$dest"
        ((moved++))
    done
    
    log_info "Sorted ${moved}/${count} files"
}

sort_watch() {
    log_info "Watching Downloads for new files..."
    
    if ! command -v inotifywait >/dev/null 2>&1; then
        log_info "Installing inotify-tools..."
        sudo pacman -S --noconfirm --needed inotify-tools
    fi
    
    sort_once
    
    log_info "Watching for new files... (Ctrl+C to stop)"
    
    inotifywait -m -e create -e moved_to -r "${DOWNLOADS}" --format '%w%f' | while read -r newfile; do
        sleep 2
        
        local classification
        classification=$(classify_file "$newfile") || continue
        
        local dest="${USER_HOME}/${classification%%:*}"
        local subfolder="${classification##*:}"
        
        [[ -n "$subfolder" && "$subfolder" != "$dest" ]] && dest="${dest}/${subfolder}"
        move_file "$newfile" "$dest"
    done
}

install_service() {
    log_info "Installing smart-sort systemd service..."
    
    local service_file="/etc/systemd/user/smart-sort.service"
    sudo mkdir -p /etc/systemd/user
    
    sudo tee "$service_file" > /dev/null <<EOF
[Unit]
Description=Smart Downloads Sorter
After=default.target

[Service]
Type=simple
ExecStart=${USER_HOME}/bin/smart-sort --watch
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

    mkdir -p "${USER_HOME}/bin"
    cp "$(dirname "${BASH_SOURCE[0]}")/smart-sort.sh" "${USER_HOME}/bin/smart-sort"
    chmod +x "${USER_HOME}/bin/smart-sort"
    
    systemctl --user daemon-reload
    systemctl --user enable --now smart-sort.service
    
    log_ok "smart-sort service installed and running"
}

main() {
    echo ""
    log_info "========================================"
    log_info " Smart Downloads Sorter"
    log_info "========================================"
    echo ""
    
    local watch=false
    
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --dry-run) DRY_RUN=true; shift ;;
            --once)    watch=false; shift ;;
            --watch)   watch=true; shift ;;
            --install-service) install_service; exit 0 ;;
            -h|--help)
                echo "Usage: $0 [--dry-run] [--once] [--watch] [--install-service]"
                exit 0
                ;;
            *) log_error "Unknown option: $1"; exit 1 ;;
        esac
        shift
    done
    
    if [[ "$watch" == "true" ]]; then
        sort_watch
    else
        sort_once
    fi
}

main "$@"
