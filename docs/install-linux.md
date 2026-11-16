# Installing on Linux (x86_64)

Fantasy 4X contains none of the original Master of Magic's pictures, sounds or music. You make them
once from your own copy of the original; nothing is uploaded.

You need a 64-bit Intel/AMD Linux with OpenGL and X11 or Wayland, and your own copy of Master of
Magic: a folder with its `.LBX` files (copy it from a Steam install on another machine, or from your
disc). Tested on: not yet (the author's test machine is Linux Mint on a 2013 MacBook Pro).

## Install

1. Download `Fantasy4X-<version>-linux-x86_64.tar.gz` from the GitHub releases page.
2. Unpack it where you like:

   ```
   tar xzf Fantasy4X-*-linux-x86_64.tar.gz
   cd Fantasy4X-*-linux-x86_64
   ```

## Set up the pictures, sounds and music

```
./f4x-setup
```

1. Choose the folder with your copy of the original. The setup says what is missing if it is not
   complete.
2. Music is made with an emulation of the Roland MT-32 sound module, which needs two ROM files
   (Roland's copyright). Choose **Download** (from the Internet Archive, after you confirm), **I have
   them** (pick the folder with `mt32_ctrl_2_07.rom` and `mt32_pcm.rom`), or **No music**.
3. Wait about a minute. The files go to `~/.local/share/fantasy4x/` (or `$XDG_DATA_HOME/fantasy4x`).

The windows need the desktop portal service (GNOME, KDE, Cinnamon and most others have it). Without
it, use the terminal version:

```
./f4x-setup --cli
```

Options: `--game DIR` (the original), `--data DIR` (another data folder), `--check` (only check the
copy), `--roms DIR`, `--download-roms`, `--no-music`.

## Play

```
./f4x-client
```

If you start the game first, it offers to start the setup (needs `zenity`; without it, it prints a
hint). Saves are in the data folder above. Delete that folder to remove everything.

## Building it yourself

On Debian, Ubuntu or Mint:

```
sudo apt install build-essential pkg-config libasound2-dev libx11-dev libxi-dev libgl1-mesa-dev zenity
git submodule update --init
scripts/make_linux.sh
```

This needs Rust (rustup.rs) and writes `target/Fantasy4X-<version>-linux-<arch>.tar.gz`. Design:
[installer-design.md](installer-design.md).
