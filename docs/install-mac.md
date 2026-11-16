# Installing on a Mac (Apple silicon)

Fantasy 4X contains none of the original Master of Magic's pictures, sounds or music. You make them
once from your own copy of the original; nothing is uploaded.

You need an Apple silicon Mac (M1 or later), macOS 11 or newer, and your own copy of Master of Magic
(for example the Steam version, "Master of Magic Classic").

## Install

1. Download `Fantasy4X-<version>-macos-arm64.dmg` (or the `.zip`) from the GitHub releases page.
2. Open it and drag **Fantasy4X.app** and **Fantasy 4X Setup.app** to **Applications**.
3. The apps are not signed by Apple (the author has no developer account), so macOS blocks the
   first start. Right-click (or Control-click) each app, choose **Open**, then **Open** again.
   If macOS says an app is damaged, run this once in Terminal:

   ```
   xattr -cr "/Applications/Fantasy4X.app" "/Applications/Fantasy 4X Setup.app"
   ```

## Set up the pictures, sounds and music

1. Open **Fantasy 4X Setup** (or open the game: it offers to start the setup).
2. Choose the folder with your copy of the original (the Steam folder `Master of Magic Classic`, or the
   folder inside it with the `.LBX` files). The setup says what is missing if the copy is not complete.
3. Music is made with an emulation of the Roland MT-32 sound module, which needs two ROM files
   (Roland's copyright). Choose **Download** (from the Internet Archive, after you confirm), **I have
   them** (pick the folder with `mt32_ctrl_2_07.rom` and `mt32_pcm.rom`), or **No music**.
4. Wait about a minute. The pictures and sounds are written to
   `~/Library/Application Support/Fantasy4X/`.

Then open **Fantasy 4X**. Saves go in the same folder. Running the setup again replaces the pictures
and sounds; delete the folder to remove everything.

## Terminal version

```
"/Applications/Fantasy 4X Setup.app/Contents/MacOS/f4x-setup" --cli
```

Options: `--game DIR` (the original), `--data DIR` (another data folder), `--check` (only check the
copy), `--roms DIR`, `--download-roms`, `--no-music`.

## Building it yourself

`scripts/make_app.sh --dist` builds both apps, a dmg and a zip (needs Rust). Design:
[installer-design.md](installer-design.md).
