# vm-curator

A fast and friendly Rust TUI for managing desktop QEMU/KVM virtual machines — with 3D acceleration, GPU and physical-disk passthrough, managed virtual networks, VM import, and 130+ pre-configured OS profiles!

### Changelog

**Unreleased**
- **Virtual Disk Resize** (thanks @HenriqueCrj, #94): New **Resize Storage** management action grows a VM's raw or qcow2 system disk in place with `qemu-img resize` — with a disk picker for multi-disk VMs, refusing to shrink, to touch physical disks or firmware images, or to resize a running VM; expand the guest partition afterwards
- **Fix Duplicated Disks in VM Info** (thanks @adonespitogo, #86): Disk parsing is scoped to the default boot invocation of `launch.sh`, so drives from the `--install`/`--cdrom` branches no longer show up as duplicates or literal `$2` paths
- **Fix VMs on a Managed Network Losing Connectivity** (thanks @adonespitogo, #87): `net-up.sh` now tells NetworkManager to leave vm-curator's bridges and tap devices unmanaged, so it can no longer silently detach a VM's tap from the bridge
- **Find `qemu-bridge-helper` on NixOS** (thanks @pimkoter, #89): The bridge-helper lookup also checks `/run/wrappers/bin`
- **UEFI Firmware Detection Without libvirt (NixOS)**: Firmware lookup now falls back to the edk2 images QEMU bundles in its own data directory (`qemu-system-x86_64 -L help`), so UEFI and Windows 11 VMs boot on a plain NixOS host without `virtualisation.libvirtd`; a new `VM_CURATOR_OVMF_DIR` variable overrides auto-detection anywhere
- **Fix `OVMF_VARS.fd: Permission denied` on NixOS**: The per-VM UEFI variables copy inherited the Nix store's read-only mode; generated scripts now `chmod u+w` it, and launching repairs VMs created before the fix
- **Single-GPU Passthrough Setup for NixOS**: The one-time setup writes a self-contained Nix module to `~/.config/vm-curator/single-gpu-vfio.nix` with import/rebuild instructions instead of dropping files in `/etc/modprobe.d` and hunting for an initramfs tool; it also offers to remove stray files from an earlier run
- **Fix Single-GPU Start Script Not Killing niri/Hyprland**: Both compositors are now matched (`Hyprland` is capitalised), so the GPU driver can actually unload on those desktops
- **Per-VM Launch Log**: Every launch writes `<vm>/qemu.log` (timestamp, exact command, and everything QEMU prints for the life of the VM; previous run kept as `qemu.log.old`), and launch failures now point at it
- **Fix Generated Scripts Not Executable on NixOS**: All generated scripts use `#!/usr/bin/env bash`, so running `./launch.sh` by hand and the sudo-executed network and single-GPU scripts work where `/bin/bash` doesn't exist
- **Fix Single-GPU Start Script Failing to Unload the Driver** (thanks @ProgrammerManSudjo, #92; also #64, #75): `single-gpu-start.sh` now waits for the display server to exit, sweeps processes holding `/dev/dri`, `/dev/kfd` or `/dev/nvidia*` with escalating signals, and retries the module unload — instead of a fixed sleep and a single attempt. Failures print real diagnostics; regenerate scripts with `g` in Single GPU Setup

**v1.4.0**
- **Physical Disk Passthrough as Boot Device** (#80): Pass whole NVMe/SATA/USB disks straight through to guests — a new **Physical Disk** mode in the creation wizard (with a safety-filtered picker that excludes the host system disk, mounted disks, and LVM/swap members) and a **Passthrough Disks** management screen for existing VMs, with per-disk boot index; USB passthrough gains a boot-index option too. Also fixes a latent bug where Fedora's qcow2 OVMF firmware could be mistaken for the VM's disk — letting Reset VM delete it
- **Virtual Network Manager** (#53): Create, edit, start/stop, and delete managed **NAT** or **Isolated** (host-only) networks from the new Networks screen (`n`), with configurable subnets and built-in DHCP — ideal for multi-VM labs. Host changes run only via inspectable generated `net-up.sh`/`net-down.sh` scripts with explicit sudo; managed networks appear in the per-VM bridge picker, and launching against a stopped network gives a clear error
- **Homebrew Tap** (#77): `brew install mroboff/tap/vm-curator` — for atomic distros like Bluefin/Silverblue; the formula tracks releases automatically
- **Fix Relative VM Library Paths** (#79): Entering a relative path (e.g. `VMs`) at setup made discovery cwd-dependent and broke launching entirely; paths are now anchored at home, and existing configs self-heal on load

**v1.3.0**
- **Configurable VM Window Size** (thanks @HenriqueCrj, #67): New Settings entry sets the initial display resolution for launched VMs (`WIDTHxHEIGHT`, e.g. `1440x900`) — applied at runtime via an environment variable on scripts using standard VGA, virtio-vga, virtio-vga-gl, or QXL video; existing scripts are patched automatically, and hand-customized video setups are left alone
- **Fix Shared Folders with Spaces in Paths** (thanks @HenriqueCrj, #66): The managed `SHARED_FOLDERS_ARGS` section is now a properly quoted bash array, so host paths containing spaces or quotes no longer break the launch script
- **Fix USB Passthrough Launch Handling** (#66): Launching from the TUI no longer passes USB QEMU flags as shell options `launch.sh` can't parse — the USB section saved in the script is the single source of truth
- **Fix UEFI Boot Delays from Stale NVRAM Entries** (#66): Normal UEFI boots now pin the disk first (`-boot strict=on` + virtio `bootindex=0`), so guests that once registered PXE/HTTP boot entries stop waiting on network boot every start
- **Keep Management Screens Open When Saving Fails** (#66): A failed save from the USB/PCI/Shared Folders unsaved-changes prompt no longer closes the screen and discards your selections

**v1.2.3**
- **Hide-KVM Toggle for Single-GPU Passthrough** (#71): The hypervisor-hiding CPU flags (`kvm=off,hv_vendor_id=…`) previously emitted only for NVIDIA are now a per-VM toggle (`[h]` in the Single GPU Setup screen) that defaults **on for AMD too** — fixes Code 43 / black screen in Windows guests on modern AMD cards like the RX 9070 XT; regenerate scripts to apply
- **Fix App Quitting When Typing 'q' in Settings** (#72): Editing a Settings value containing `q` (e.g. a path like `~/qemu-vms`) no longer quits the app — `q` still quits when just browsing

[Full changelog](CHANGELOG.md)

### Features

**VM Discovery & Organization**
- Automatically scans your VM library for directories containing `launch.sh` scripts
- Hierarchical organization by 16 OS families with emoji icons and 50 subcategories
- Parses QEMU launch scripts to extract configuration (emulator, memory, CPU, VGA, audio, network, disks)
- Smart categorization with configurable hierarchy patterns
- Live process monitoring — shows running VMs with status indicators
- Search and filter VMs by name

**VM Creation Wizard**
- 5-step guided wizard for creating new VMs
- 130+ pre-configured OS profiles with optimal QEMU settings (Windows, macOS, Linux, BSD, Unix, retro, and more)
- Automatic UEFI firmware detection across Linux distributions (Arch, Debian, Fedora, NixOS, etc.)
- ISO file browser for selecting installation media
- Configurable disk size/format, memory, CPU cores, and QEMU options with direct text editing and size suffixes (e.g., "8GB")
- Use existing disk images (copy or move) instead of creating new ones
- Pass a whole physical disk (NVMe/SATA/USB) through as the boot device instead of using a disk image
- Support for custom OS entries with user metadata

**VM Import Wizard**
- Import existing VMs from libvirt (virsh) XML configurations and Quickemu `.conf` files
- 5-step guided import: select source, choose VM, review compatibility warnings, configure disk handling, review and import
- Automatic OS profile detection from imported configurations
- Disk handling options: symlink, copy, or move existing disk images

**GPU Passthrough**
- **Single-GPU passthrough**: Pass your only GPU to a VM (requires TTY, stops display manager)
- **Guest driver compatibility**: Per-VM vBIOS ROM (`romfile=`) and Hide-KVM (`kvm=off`) options for AMD/NVIDIA Windows driver quirks
- **Multi-GPU passthrough**: Pass a secondary GPU while keeping the primary for the host
- **Looking Glass integration**: Near-zero latency display for multi-GPU setups with auto-launch support
- **PCI passthrough screen**: Select PCI devices (GPUs, USB controllers, NVMe) for VM passthrough
- **System setup wizard**: One-click VFIO/IOMMU configuration with initramfs regeneration

**3D Graphics Acceleration**
- Para-virtualized 3D acceleration with `virtio-vga-gl` and SDL `gl=on`
- Tested on NVIDIA RTX-4090 with driver 590.48.01+
- Automatic SDL display selection for 3D-enabled VMs

**Snapshot Management**
- Create, restore, and delete snapshots for qcow2 disk images
- Visual snapshot list with timestamps and sizes
- Background operations with progress feedback

**Physical Disk Passthrough**
- Pass whole physical disks (NVMe, SATA/HDD, USB) through to guests as raw virtio-blk devices — at creation time (wizard "Physical Disk" mode) or on existing VMs (the "Passthrough Disks" management screen)
- Safety-filtered disk picker: the host system disk, mounted disks, swap members, and LVM/LUKS/RAID members are excluded with the reason shown; selection requires an explicit typed confirmation
- Stable `/dev/disk/by-id` device paths, per-disk firmware boot index, and launch-time preflight checks (device present, read/write access with fix hints, refuses mounted partitions)

**Network Configuration**
- Network backend selection: user/SLIRP (NAT), passt, bridge, or none
- Port forwarding with presets for common services (SSH, RDP, HTTP, HTTPS, VNC)
- Bridge networking with automatic bridge detection, status checklist, and setup guidance
- Configurable network adapter models per VM
- **Virtual Network Manager** (`n` on the main menu): create, edit, start/stop, and delete managed NAT or Isolated (host-only) networks with configurable subnets and built-in DHCP — ideal for multi-VM lab topologies. Host changes run via inspectable generated `net-up.sh`/`net-down.sh` scripts with explicit sudo

**Shared Folders**
- Share host directories with VMs using virtio-9p
- Add, remove, and edit shared folders from the management menu
- Automatic mount tag generation

**Clipboard Sharing (SPICE)**
- Bidirectional host ⇄ guest copy/paste when the display backend is `spice-app`
- The SPICE guest-agent channel is added to `launch.sh` automatically — no extra configuration
- Requires `virt-viewer`/`remote-viewer` on the host and `spice-vdagent` running in the guest:
  - Debian/Ubuntu/Kali: `sudo apt install spice-vdagent`
  - Fedora/RHEL: `sudo dnf install spice-vdagent`
  - Arch: `sudo pacman -S spice-vdagent`
  - Then enable the service (`sudo systemctl enable --now spice-vdagentd`) and reboot the guest

**USB Passthrough**
- USB device enumeration via libudev with sysfs fallback
- xHCI USB 3.0 controller with 8 ports (supports up to 8 USB 2.0 + 8 USB 3.0 devices)
- Persistent passthrough configuration
- Optional per-device firmware boot index (`b`) — passed-through USB drives can be the boot device
- Hub filtering and keyboard/mouse detection for passthrough validation

**VM Notes**
- Free-form personal notes for any VM from the management menu
- Multi-line text editor with full keyboard navigation
- Notes displayed in the main info panel and preserved across VM renames

**Launch Script Editor**
- Edit `launch.sh` scripts directly in the TUI
- Syntax-aware display with line numbers and horizontal scrolling
- Automatic QEMU configuration re-parsing after saves
- Automatic single-GPU passthrough script regeneration when applicable

**Additional Features**
- Vim-style navigation (j/k, arrows, mouse) with full clickable interface
- Multiple boot modes (normal, install, custom ISO, recovery image, floppy)
- Dynamic display backend detection per emulator (GTK, SDL, SPICE-app, VNC)
- Headless VM support (display=none) with process monitoring
- Stop/force-stop VMs (ACPI poweroff or SIGKILL)
- VM rename with persistent custom display names
- OS metadata with historical blurbs, fun facts, and multi-step installation guides
- 42+ ASCII art logos for classic and modern operating systems
- BTRFS copy-on-write auto-disable for VM directories
- First-time setup wizard for configuring the VM library directory
- Configurable settings with persistence

### Screenshots

```
 VM Curator (QEMU VM Library in ~/vm-space)
┌─────────────────────────────────────────────────────────────────────┐
│ ┌─────────────────────────┐  ┌────────────────────────────────────┐ │
│ │ VMs (35)                │  │       _    _ _           _        │ │
│ │ ──────────────────────  │  │      | |  | (_)         | |       │ │
│ │ 🪟 Microsoft            │  │      | |/\| |_ _ __   __| | ___   │ │
│ │   ▼ DOS                 │  │      \  /\  / | '_ \ / _` |/ _ \  │ │
│ │     > MS-DOS 6.22   [*] │  │       \/  \/|_|_| |_|\__,_|\___/  │ │
│ │     > Windows 3.11      │  │                                   │ │
│ │   ▼ Windows 9x          │  │   Windows 95 OSR2.5               │ │
│ │     > Windows 95        │  │   Microsoft | August 1995 | i386  │ │
│ │     > Windows 98        │  │                                   │ │
│ │ 🐧 Linux                │  │   The OS that changed everything  │ │
│ │   ▼ Debian-based        │  │   with the Start Menu, taskbar,   │ │
│ │     > Debian 12         │  │   and 32-bit computing for all.   │ │
│ │     > Ubuntu 24.04      │  │                                   │ │
│ └─────────────────────────┘  └────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────┤
│ [Enter] Launch  [m] Manage  [c] Create  [n] Networks  [?] Help     │
└─────────────────────────────────────────────────────────────────────┘
```

### Installation

**AUR (Arch / Arch-derived)**

```bash
# Using your preferred AUR helper
paru -S vm-curator
yay -S vm-curator
```

**Homebrew (Linux, incl. atomic distros like Bluefin/Silverblue)**

```bash
brew install mroboff/tap/vm-curator
```

Installs the prebuilt x86_64 binary from [the tap](https://github.com/mroboff/homebrew-tap); QEMU itself is not bundled (see `brew info vm-curator`).

**crates.io**

```bash
cargo install vm-curator
```

**Binary Packages**

Pre-built packages (DEB, RPM, AppImage, tarball) are available from [GitHub Releases](https://github.com/mroboff/vm-curator/releases).

**Nix / NixOS**

```bash
# Run directly without installing
nix run github:mroboff/vm-curator

# Build the package
nix build .#default
```

For NixOS, add to `/etc/nixos/configuration.nix`:
```nix
{ pkgs, ... }:
{
  environment.systemPackages = [ pkgs.vm-curator ];
}
```

**From Source**

```bash
git clone https://github.com/mroboff/vm-curator.git
cd vm-curator
cargo build --release
```

The binary will be at `target/release/vm-curator`.

**Prerequisites**
- **Required**: QEMU (`qemu-system-*` binaries), qemu-img (for disk creation and snapshots), libudev
- **Build**: a recent Rust stable toolchain (see `rust-toolchain.toml`), libudev-dev (Debian/Ubuntu) or systemd-libs (Arch/Fedora)
- **Optional**:
  - OVMF/edk2 — UEFI boot support (`edk2-ovmf` on Arch, `ovmf` on Debian/Ubuntu); not needed where QEMU's bundled edk2 firmware is used (e.g. NixOS)
  - swtpm — TPM 2.0 emulation (required for Windows 11 VMs)
  - virt-viewer — SPICE-app display backend
  - passt — passt network backend
  - dnsmasq — DHCP for managed virtual networks (nftables or iptables for NAT/isolation)
  - Looking Glass client — multi-GPU passthrough display
  - polkit — bridge networking permissions

### Usage

#### TUI Mode (default)

```bash
vm-curator
```

#### CLI Commands

```bash
# List all VMs
vm-curator list

# Launch a VM
vm-curator launch windows-95
vm-curator launch windows-95 --install    # Boot in install mode
vm-curator launch windows-95 --cdrom /path/to/image.iso

# View VM configuration
vm-curator info windows-95

# Import a VM
vm-curator  # then press 'i' for the import wizard

# Manage snapshots
vm-curator snapshot windows-95 list
vm-curator snapshot windows-95 create my-snapshot
vm-curator snapshot windows-95 restore my-snapshot
vm-curator snapshot windows-95 delete my-snapshot

# List available QEMU emulators
vm-curator emulators
```

### Key Bindings

#### Main Menu

| Key | Action |
|-----|--------|
| `j/k` or `Down/Up` | Navigate VM list |
| `Enter` | Launch selected VM |
| `m` | Open management menu |
| `x` | Stop VM (if running) |
| `c` | Open VM creation wizard |
| `i` | Open VM import wizard |
| `s` | Open settings |
| `n` | Virtual Network Manager |
| `/` | Search/filter VMs |
| `?` | Show help |
| `PgUp/PgDn` | Scroll info panel |
| `Esc` | Back / Cancel |
| `q` | Quit |

#### VM Management

| Key | Action |
|-----|--------|
| `j/k` or `Down/Up` | Navigate menu |
| `Enter` | Select menu option |
| `1`–`9` | Jump directly to a menu option |
| `Esc` | Back |

Management menu options:
- Boot Options (normal, install, custom ISO)
- Snapshots
- USB Passthrough
- PCI Passthrough
- Passthrough Disks (whole physical disks)
- Shared Folders
- Network Settings
- Multi-GPU Passthrough (if enabled)
- Single GPU Passthrough (if enabled)
- Change Display
- 3D Acceleration toggle
- Edit Notes
- Resize Storage (grow the virtual disk)
- Rename VM
- Stop VM
- Reset VM (recreate disk)
- Delete VM
- Edit Raw Configuration

#### Create Wizard

| Key | Action |
|-----|--------|
| `j/k` or `Down/Up` | Navigate fields |
| `←/→` | Adjust / toggle the focused field |
| `Tab` | Edit the focused value as text (where supported) |
| `Enter` | Continue to next step |
| `Esc` | Previous step / cancel |

### Configuration

Settings are stored in `~/.config/vm-curator/config.toml` and can be edited via the Settings screen (`s` key).

```toml
# VM library location
vm_library_path = "~/vm-space"

# Default values for new VMs
default_memory_mb = 4096
default_cpu_cores = 2
default_disk_size_gb = 64
default_display = "gtk"      # gtk, sdl, spice-app, vnc
default_enable_kvm = true
# default_iso_path = "/path/to/isos"   # Directory the ISO browser opens in (omit = home)
# default_window_size = "1440x900"     # Initial VM window size (omit = VM default)

# Behavior
confirm_before_launch = true

# Multi-GPU passthrough (Looking Glass)
enable_multi_gpu_passthrough = false
default_ivshmem_size_mb = 64
show_gpu_warnings = true
looking_glass_client_path = ""       # Path to Looking Glass client
looking_glass_auto_launch = true     # Auto-launch client when VM starts

# Single GPU passthrough
single_gpu_enabled = false
single_gpu_auto_tty = false          # Experimental: auto switch TTY
single_gpu_dm_override = ""          # Override display manager detection
```

### VM Library Structure

VMs are expected in your library directory (default `~/vm-space/`) with this structure:

```
~/vm-space/
├── windows-95/
│   ├── launch.sh      # QEMU launch script (required)
│   ├── disk.qcow2     # Disk image (qcow2 recommended for snapshots)
│   └── qemu.log       # Output of the last launch (command + stdout/stderr); qemu.log.old = the one before
├── linux-debian/
│   ├── launch.sh
│   ├── disk.qcow2
│   └── install.iso    # Optional: installation media
└── macos-tiger/
    ├── launch.sh
    └── disk.qcow2
```

The `launch.sh` script should invoke QEMU. VM Curator parses this script to extract configuration and can generate new scripts via the creation wizard.

### OS Profiles

The creation wizard includes 130+ pre-configured profiles organized into 16 OS families:

**Microsoft**: DOS, Windows 1.x–3.x, Windows 95/98/ME, Windows NT/2000/XP/Vista, Windows 7/8/10/11, Server editions

**Apple**: Classic Mac OS (System 6–9), Mac OS X PowerPC (Cheetah–Tiger), Mac OS X Intel (Leopard–El Capitan), macOS (Sierra–Tahoe)

**Linux**: Arch, Manjaro, EndeavourOS, Garuda, CachyOS, Debian, Ubuntu, Mint, Pop!_OS, Fedora, RHEL, Rocky, Alma, Bazzite, openSUSE, Slackware, Gentoo, Void, NixOS, Alpine, and more

**BSD**: FreeBSD, GhostBSD, OpenBSD, NetBSD, DragonFly BSD

**Unix**: Solaris, OpenIndiana, illumos, HP-UX, IRIX, MINIX, QNX

**IBM**: OS/2, eComStation, ArcaOS, AIX

**Commodore**: AmigaOS, AROS, MorphOS

**Be / Haiku**: BeOS, Haiku

**NeXT**: NeXTSTEP, OpenStep

**Research**: Plan 9, 9front, Inferno

**Alternative**: SerenityOS, Redox, TempleOS, KolibriOS, MenuetOS, ReactOS

**Retro**: Atari TOS, CP/M, FreeDOS, DR-DOS, GEOS, RISC OS

**Mobile**: Android-x86, LineageOS, Bliss OS

**Infrastructure**: pfSense, OPNsense, OpenWrt, TrueNAS, Proxmox, ESXi

**Utilities**: GParted, Clonezilla, Memtest86+

**Other**: Catch-all for uncategorized VMs

Each profile includes optimal QEMU settings for that OS (emulator, machine type, CPU model, VGA, audio, network, disk interface, and more).

### Metadata Customization

**OS Information**: Override or add OS metadata in `~/.config/vm-curator/metadata/`:

```toml
# ~/.config/vm-curator/metadata/my-os.toml
[my-custom-os]
name = "My Custom OS"
publisher = "My Company"
release_date = "2024-01-01"
architecture = "x86_64"

[my-custom-os.blurb]
short = "A brief description"
long = "A longer description with history and details."

[my-custom-os.fun_facts]
facts = ["Fact 1", "Fact 2"]
```

**ASCII Art**: Add custom ASCII art in `~/.config/vm-curator/ascii/`.

**QEMU Profiles**: Override profiles in `~/.config/vm-curator/qemu_profiles.toml`.

### Dependencies

- **Runtime**: QEMU, qemu-img, libudev
- **Build**: a recent Rust stable toolchain (see `rust-toolchain.toml`), libudev-dev (Debian/Ubuntu) or systemd-libs (Arch)
- **Optional**: OVMF/edk2 (UEFI), swtpm (Windows 11 TPM), virt-viewer (SPICE-app), passt (networking), dnsmasq + nftables/iptables (managed virtual networks), Looking Glass client (multi-GPU), polkit (bridge networking)

### Cross-Distribution Compatibility

VM Curator automatically detects OVMF/UEFI firmware as matched CODE+VARS pairs (preferring 4M builds, including Fedora's qcow2-format firmware) across Linux distributions:
- Arch Linux: `/usr/share/edk2/x64/OVMF_CODE.4m.fd`
- Debian/Ubuntu: `/usr/share/OVMF/OVMF_CODE_4M.fd`
- Fedora/RHEL: `/usr/share/edk2/ovmf/OVMF_CODE_4M.qcow2`
- NixOS and any other host without an FHS OVMF package: the edk2 firmware QEMU bundles in its own data directory (`qemu-system-x86_64 -L help`) — no libvirt required
- And more...

**Single-GPU passthrough on NixOS**: the one-time system setup (Settings → *Run Setup*) does not write to `/etc` or regenerate an initramfs on NixOS. It writes a ready-to-import module to `~/.config/vm-curator/single-gpu-vfio.nix` (IOMMU kernel params for your CPU, VFIO modules, `softdep`/`vfio_pci` options); copy it into your configuration repository, add it to `imports`, `nixos-rebuild switch`, reboot. The runtime `single-gpu-start.sh`/`-restore.sh` scripts work unchanged (they use `systemctl stop display-manager`, which is the NixOS unit name).

To pin a specific firmware build, or on a host whose firmware lives somewhere unusual, set `VM_CURATOR_OVMF_DIR` to a directory containing a CODE+VARS pair (`OVMF_CODE*.fd`/`OVMF_VARS*.fd` or QEMU's `edk2-x86_64-code.fd`/`edk2-i386-vars.fd`). It is checked before every built-in path. On NixOS, for example: `VM_CURATOR_OVMF_DIR="${pkgs.OVMF.fd}/FV"`.

---

### Contributing

Contributions are welcome! If you find a bug or have an idea for an improvement, feel free to open an issue or submit a Pull Request.

**Help Wanted: ASCII Art**
As a TUI application, `vm-curator` relies on visual flair to stand out. I am specifically looking for help with:
* **Logo/Banner Art:** A cool ASCII banner for the startup screen.
* **Iconography:** Small, recognizable ASCII/block character icons for the TUI menus (e.g., stylized hard drives, network cards, or GPU icons).

If you have a knack for terminal aesthetics, your PRs are highly appreciated!

### Support & Maintenance Status

**`vm-curator`** was built to solve a specific, painful problem: getting high-performance, 3D-accelerated Linux VMs (via QEMU) without the overhead and complexity of `libvirt` or `virt-manager`.

This is a **personal passion project** that I am sharing with the community. While I use this tool daily and will fix critical bugs as I encounter them, please note:

* **Development Pace:** This project is maintained in my spare time. Feature requests will be considered but are not guaranteed.
* **The "As-Is" Philosophy:** The goal is a lean, transparent TUI. I prioritize stability and performance over comprehensive enterprise feature parity.

**If this tool saved you time or helped you get 3D Acceleration working without having to resort to passthrough:**

If you'd like to say thanks, you can support the project below. **Donations are a "thank you" for existing work, not a payment for future support.**

* **[GitHub Sponsors](https://github.com/sponsors/mroboff):** Best for one-time contributions (Goes to the RTX-Pro 6000 fund!)
* **[Ko-fi](https://ko-fi.com/mroboff):** Buy me a coffee (or a generic energy drink).

---

### License

MIT
