# dmgbuild settings for the Altrune installer image (read by tools/crear-dmg.sh).
#
# No absolute path lives here: everything that depends on the machine comes from -D defines
# (dmgbuild puts them in `defines`):
#   app      the Altrune.app to copy into the image (already built, and signed or not)
#   icono    the volume icon (assets/branding/Altrune.icns)
#   fondo    the HiDPI background (assets/dmg/fondo.tiff: 660x400 at 1x plus 1320x800 at 2x)
#   formato  UDZO (zlib, readable by every macOS); tools/crear-dmg.sh passes it
#   tamano   size of the volume in bytes (the app plus some room; dmgbuild would use 128 MiB more)
# The window, the two icon positions and the background are one design: change them together
# with assets/dmg/fondo.svg (see tools/dmg/generar-fondo.sh).

volume_name = "Altrune"
format = defines["formato"]  # noqa: F821  (defines is injected by dmgbuild)
filesystem = "HFS+"
size = int(defines["tamano"])  # noqa: F821

# Exactly the app and the shortcut to /Applications. The app is never touched afterwards: no
# SetFile on it (that would write com.apple.FinderInfo and break codesign --strict), so there is
# no hide_extensions here on purpose.
files = [defines["app"]]  # noqa: F821
symlinks = {"Applications": "/Applications"}
icon = defines["icono"]  # noqa: F821
background = defines["fondo"]  # noqa: F821

# The window frame is 660 x 432 points: Finder counts the title bar (32 points on macOS 27,
# measured with tools/dmg/captura.sh) inside the frame, so the content area is 660 x 400. The
# icons sit on the quarter points of the width, level with each other (positions are in
# content coordinates); the labels go below the 128-point icons. assets/dmg/fondo.svg is 16
# points taller than the content on purpose, for a title bar that is a few points smaller.
window_rect = ((200, 120), (660, 432))
default_view = "icon-view"
icon_size = 128
text_size = 13
icon_locations = {
    "Altrune.app": (165, 190),
    "Applications": (495, 190),
}

show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
