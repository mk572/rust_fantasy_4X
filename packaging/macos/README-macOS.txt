Fantasy 4X for macOS (Apple silicon)
====================================

1. Drag Fantasy4X.app and Fantasy 4X Setup.app to your Applications folder.

2. The apps are not signed by Apple (the author has no developer account), so macOS blocks
   the first start. Open each once with a right click (or Control-click) on the app, choose
   Open, then Open again in the dialog. If macOS says an app is damaged, open Terminal
   and run:

       xattr -cr "/Applications/Fantasy4X.app" "/Applications/Fantasy 4X Setup.app"

3. This download contains none of the original Master of Magic's pictures, sounds or music.
   The game starts without them, in plain colours and silent. To get the pictures and sound,
   open Fantasy 4X Setup and point it at your own copy of the original game (for example the
   Steam folder). Nothing is uploaded; the pictures are made on your computer.

Your saves and the pictures and sounds the setup makes are in
~/Library/Application Support/Fantasy4X/ . Delete that folder to remove them.

The game and its rules are public domain (CC0); see LICENSE.
