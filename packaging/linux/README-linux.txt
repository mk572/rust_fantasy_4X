Fantasy 4X for Linux (x86_64)
=============================

1. Unpack the archive anywhere you like, for example your home folder:

       tar xzf Fantasy4X-*-linux-x86_64.tar.gz

2. This download contains none of the original Master of Magic's pictures, sounds or music.
   Run the setup once and point it at your own copy of the original game (for example the
   Steam folder). Nothing is uploaded; the pictures and sounds are made on your computer:

       ./f4x-setup

   The setup opens windows (they need a desktop with the xdg-desktop-portal service, which
   GNOME, KDE and most others have). On a bare system use the terminal version:

       ./f4x-setup --cli

3. Play:

       ./f4x-client

   If you start the game before the setup, it offers to start the setup (needs zenity;
   without it, it only prints a hint).

Your saves and the pictures and sounds the setup makes are in ~/.local/share/fantasy4x/
(or $XDG_DATA_HOME/fantasy4x). Delete that folder to remove them.

The game needs OpenGL, X11 or Wayland (through XWayland) and, for sound, ALSA or PulseAudio's
ALSA layer. The setup's music step runs the program f4x-music (LGPL, see legal/).

The game and its rules are public domain (CC0); see LICENSE.
