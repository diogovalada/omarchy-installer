# Portable startup foreground handoff

The later [verified cache implementation](portable-cache-2026-09-07.md) retains
the payload between runs and moves the final activation after banner closure.
The temporary-payload cleanup measurements below describe the earlier build.

The previous launcher removed its extraction banner before launching the app.
The app could briefly receive focus and then end up behind another window.
The foreground startup check reproduced that on portable build `4bf09ba1`.

The launcher now retains the banner while starting the app with normal window
visibility. It identifies a visible, unowned, titled window belonging to the
exact child process, activates it once, and removes the banner. It does not set
the app always on top. The launcher retains the child process handle and waits
for exit before cleaning up the temporary payload. Window discovery is limited
to two minutes; a slow app remains running afterward without the banner.

The implementation uses the ordinary Windows foreground activation API, which
respects Windows restrictions if the user is interacting with another window:
[SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow).

`verify-portable-startup.ps1` now checks foreground focus after window creation
and checks it remains there after the extraction handoff, along with normal
visibility, absence of always-on-top, exit status and temporary-file cleanup.

Portable build `a8db5dea` passed this check: banner at 0.93 seconds, app window
at 11.70 seconds, retained foreground focus, normal exit and complete cleanup.
Its extracted payload hashes and native media file-write check also passed.
The unchanged application binary was reused only after matching the previous
verified portable record; this fix changes the launcher rather than the app.
