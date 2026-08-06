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
├── Downloads/                  # TEMPORARY ONLY — auto-cleaned weekly
│   ├── .keep                   # Empty marker
│   └── [auto-cleaned weekly]
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
| Downloads | `~/Downloads/` | Auto-clean weekly |
| Archives/old files | `~/Archives/` | Manual |
| Trash | `~/.trash/` | Auto-purge monthly |
| Scripts | `~/bin/` | Manual |

---

## Automatic Cleanup Rules

### Downloads — weekly auto-clean
```bash
# Delete files older than 7 days
find ~/Downloads -type f -mtime +7 -delete
find ~/Downloads -type d -empty -delete
```

### Trash — monthly auto-purge
```bash
# Delete files older than 30 days
find ~/.trash -type f -mtime +30 -delete
```

### Archives — yearly compression
```bash
# Compress files older than 1 year
find ~/Archives -type f -mtime +365 -exec gzip {} \;
```

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

## Cron Jobs (Auto-Maintenance)

Add to crontab (`crontab -e`):

```bash
# Clean Downloads weekly (Sunday 3 AM)
0 3 * * 0 find /home/gagan/Downloads -type f -mtime +7 -delete 2>/dev/null || true

# Purge trash monthly (1st of month, 4 AM)
0 4 1 * * find /home/gagan/.trash -type f -mtime +30 -delete 2>/dev/null || true

# Compress old archives yearly (Jan 1, 5 AM)
0 5 1 1 * find /home/gagan/Archives -type f -mtime +365 -exec gzip {} \; 2>/dev/null || true
```

---

## Enforcement

### Bashrc aliases
```bash
# Add to ~/.bashrc
alias downloads='cd ~/Downloads && ls -lt | head -20'
alias projects='cd ~/Projects'
alias models='cd ~/Models'
alias datasets='cd ~/Datasets'
alias work='cd ~/Workspace'
alias clean-downloads='find ~/Downloads -type f -mtime +7 -delete && find ~/Downloads -type d -empty -delete'
```

### Starship prompt — show current directory context
```toml
[directory]
truncation_length = 4
style = "bold cyan"
```

---

## Anti-Clutter Rules

1. **Downloads is temporary** — if you haven't moved it in 7 days, it gets deleted automatically
2. **No project files in Downloads** — create project directory first, then download there
3. **Documents go to Documents** — not Downloads, not Desktop
4. **Models go to Models** — not Downloads, not home
5. **Datasets go to Datasets** — not Downloads
6. **Archives go to Archives** — not cluttering active directories
7. **Trash goes to .trash** — auto-purged, not in Downloads

---

## Benefits

- **Clean home** — only dotfiles and top-level directories
- **Easy backup** — snapshot `~/Workspace`, `~/Models`, `~/Datasets` separately
- **Fast search** — `find ~/Documents` instead of `find ~`
- **No clutter** — Downloads auto-cleans, Trash auto-purges
- **Clear separation** — personal vs work vs AI vs archives

---

**This policy is enforced by the installer.**
