# Security

## Reporting a vulnerability

Please report security problems privately, through GitHub: on the repository's **Security** tab, choose **Report a vulnerability**. Don't open a public issue for them.

Say what you found, how to reproduce it, and which platform and version of Snappy it affects. You'll get an answer within a week.

## What Snappy has access to

Snappy moves and resizes other apps' windows, so it holds broad permissions: Accessibility on macOS, global shortcuts and input hooks on Windows, and a GNOME Shell extension or a KWin script on Linux. It never uses the network, and never reads or stores window titles, window contents, or keystrokes. A way to make it do any of these, or to use its permissions to do something it shouldn't, is a vulnerability worth reporting.

## Supported versions

Only the latest release gets fixes.
