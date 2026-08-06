# Home Directory Organization Policy
**For: MSI Sword 16 HX B14VEKG**
**Goal: Zero clutter, automatic separation**

---

## Core Rule

**Nothing stays in `/home/gagan/` except dotfiles and symlinks.**

Every file type has a canonical directory. Downloads go to a staging area, not a permanent home.

---

## Directory Tree

```
/home/gagan/
├── .config/                    # App configs (managed by dots-hyprland)
├── .local/                     # Local app data
├── .ssh/                       # SSH keys
├── .gnupg/                     # GPG keys
│
├── Documents/                  # Personal documents ONLY
│   ├── Financial/
│   ├── Medical/
│   ├── Legal/
│   ├── Education/
│   └── Archives/               # Old docs compressed/archived
│
├── Pictures/                   # Photos, screenshots, wallpapers
│   ├── Wallpapers/
│   ├── Screenshots/
│   └── Camera/
│
├── Videos/                     # Recordings, movies, tutorials
│   ├── Movies/
│   ├── ScreenRecordings/
│   └── Tutorials/
│
├── Music/                      # Audio files
│   ├── Playlists/
│   └── Podcasts/
│
├── Downloads/                  # Staging area — auto-sorted by smart-sort
│   └── [watched and sorted automatically]
│
├── Projects/                   # ALL development work
│   ├── personal/               # Personal projects (not on GitHub)
│   │   ├── scripts/
│   │   ├── experiments/
│   │   └── learning/
│   ├── work/                   # Work projects (if any)
│   └── archived/               # Completed/deprecated projects
│
├── Workspace/                  # Git repos (active development)
│   ├── nexus-kernel/
│   ├── NexusAOS/
│   └── SeshaOS/
│
├── Models/                     # AI models ONLY
│   ├── ollama/                 # Symlink to ~/.ollama
│   ├── huggingface/            # HF cache
│   ├── checkpoints/            # Training checkpoints
│   └── embeddings/             # Vector DBs, embeddings
│
├── Datasets/                   # Training data ONLY
│   ├── raw/                    # Original, untouched data
│   ├── processed/              # Cleaned, tokenized, ready for training
│   ├── experiments/            # Experiment-specific subsets
│   └── external/               # Downloaded datasets (Keras, HF, etc.)
│
├── Archives/                   # Compressed, old, inactive files
│   ├── 2024/
│   ├── 2025/
│   └── backups/
│
├── bin/                        # User scripts and binaries
│   ├── msi-mux-switcher        # Installed here, symlinked to /usr/local/bin
│   └── utils/
│
└── .trash/                     # Local trash, auto-purged monthly
```

---

## Placement Rules

| File Type | Destination | Auto-Move? |
|-----------|-------------|------------|
| Git repos | `~/Workspace/` | Manual clone |
| Personal projects | `~/Projects/personal/` | Manual |
| AI models | `~/Models/` | Symlink from `~/.ollama` |
| Training data | `~/Datasets/` | Manual |
| Documents | `~/Documents/` | Manual |
| Photos/screenshots | `~/Pictures/` | Manual |
| Videos | `~/Videos/` | Manual |
| Music | `~/Music/` | Manual |
| Downloads | `~/Downloads/` | Auto-sorted by smart-sort |
| Archives/old files | `~/Archives/` | Manual |
| Trash | `~/.trash/` | Auto-purge monthly |
| Scripts | `~/bin/` | Manual |

---

## Automatic Organization

### smart-sort service
Downloads are automatically sorted by file type using `smart-sort`:
- Videos → `~/Videos/Movies/`
- Images → `~/Pictures/`
- Documents → `~/Documents/`
- Archives/ISOs → `~/Archives/`
- Music → `~/Music/`
- Installers → `~/Archives/Installers/`
- Torrents → `~/Downloads/torrents/`
- Code/projects → `~/Projects/`
- AI models → `~/Models/`

Runs as a systemd user service. Watches Downloads for new files and moves them automatically.

---

## Migration from Current Ubuntu Setup

### Step 1: Identify current home contents
```bash
ls -la /home/gagan/
```

### Step 2: Sort into new structure
```bash
# Documents
mv /home/gagan/Documents/* ~/Documents/

# Pictures
mv /home/gagan/Pictures/* ~/Pictures/

# Videos
mv /home/gagan/Videos/* ~/Videos/

# Music
mv /home/gagan/Music/* ~/Music/

# Projects (non-Git)
mkdir -p ~/Projects/personal
mv /home/gagan/StudioProjects/* ~/Projects/personal/ 2>/dev/null || true
mv /home/gagan/PycharmProjects/* ~/Projects/personal/ 2>/dev/null || true
mv /home/gagan/AndroidStudioProjects/* ~/Projects/personal/ 2>/dev/null || true

# Workspace (Git repos)
mkdir -p ~/Workspace
mv /home/gagan/Workspace/* ~/Workspace/ 2>/dev/null || true

# Models
mkdir -p ~/Models
mv /home/gagan/.ollama ~/Models/ollama 2>/dev/null || true

# Datasets
mkdir -p ~/Datasets
mv /home/gagan/Datasets/* ~/Datasets/ 2>/dev/null || true

# Archives
mkdir -p ~/Archives
mv /home/gagan/Downloads/*.zip ~/Archives/ 2>/dev/null || true
mv /home/gagan/Downloads/*.tar.gz ~/Archives/ 2>/dev/null || true
mv /home/gagan/Downloads/*.deb ~/Archives/ 2>/dev/null || true
```

### Step 3: Clean up empty directories
```bash
rmdir /home/gagan/StudioProjects 2>/dev/null || true
rmdir /home/gagan/PycharmProjects 2>/dev/null || true
rmdir /home/gagan/AndroidStudioProjects 2>/dev/null || true
```

---

## BTRFS Subvolumes (Optional)

For easier backup and snapshots, create separate subvolumes:

```bash
sudo btrfs subvolume create /@home_gagan
sudo btrfs subvolume create /@home_gagan_workspace
sudo btrfs subvolume create /@home_gagan_models
sudo btrfs subvolume create /@home_gagan_datasets

# Add to /etc/fstab
UUID=<your-btrfs-uuid> /home/gagan/Workspace btrfs noatime,compress=zstd:1,subvol=@home_gagan_workspace 0 0
UUID=<your-btrfs-uuid> /home/gagan/Models btrfs noatime,compress=zstd:1,subvol=@home_gagan_models 0 0
UUID=<your-btrfs-uuid> /home/gagan/Datasets btrfs noatime,compress=zstd:1,subvol=@home_gagan_datasets 0 0
```

**Benefit:** Can snapshot `/home` independently of large model/dataset data.

---

## smart-sort Service

Installed as a systemd user service:
```bash
systemctl --user status smart-sort.service
```

Logs:
```bash
journalctl --user -u smart-sort -f
```

Manual trigger:
```bash
~/bin/smart-sort --once
```

Dry run:
```bash
~/bin/smart-sort --dry-run
```

---

## Enforcement

### Bashrc aliases
```bash
alias projects='cd ~/Projects'
alias models='cd ~/Models'
alias datasets='cd ~/Datasets'
alias work='cd ~/Workspace'
alias sort-downloads='~/bin/smart-sort --once'
```

---

## Anti-Clutter Rules

1. **Downloads is staging, not storage** — smart-sort moves files automatically within seconds
2. **No project files in Downloads** — create project directory first, then download there
3. **Documents go to Documents** — not Downloads, not Desktop
4. **Models go to Models** — not Downloads, not home
5. **Datasets go to Datasets** — not Downloads
6. **Archives go to Archives** — not cluttering active directories
7. **Trash goes to .trash** — not in Downloads

## Enforcement

### Bashrc aliases
```bash
alias projects='cd ~/Projects'
alias models='cd ~/Models'
alias datasets='cd ~/Datasets'
alias work='cd ~/Workspace'
alias sort-downloads='~/bin/smart-sort --once'
```

---

## Benefits

- **Clean home** — only dotfiles and top-level directories
- **Easy backup** — snapshot `~/Workspace`, `~/Models`, `~/Datasets` separately
- **Fast search** — `find ~/Documents` instead of `find ~`
- **No clutter** — Downloads auto-sorts, nothing gets deleted
- **Clear separation** — personal vs work vs AI vs archives
- **Zero manual sorting** — smart-sort handles everything automatically

---

**This policy is enforced by the installer.**
