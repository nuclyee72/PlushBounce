## What's new
- **Clearer macOS update help.** If macOS is running PlushBounce from a temporary read-only copy, the app can't update itself. It now explains how to fix that instead of showing a "Read-only file system" error.

## Install
**Windows:** if you already have 0.4.0 or later, the app offers this update by itself. Otherwise download **PlushBounce_…_x64-setup.exe** below and run it. Your existing dolls and settings are kept.
Windows may show "Windows protected your PC" because the app is not code-signed. Click **More info → Run anyway**.

**macOS:** download **PlushBounce_…_aarch64.dmg** (Apple Silicon: M1 and later) or **PlushBounce_…_x64.dmg** (Intel Macs), open it, and drag PlushBounce into Applications.
The app is not notarized by Apple, so the first time you open it macOS blocks it. Open **System Settings → Privacy & Security**, scroll down, and click **Open Anyway**. When asked, allow PlushBounce to record system audio — that is how it hears the music.
So that future updates can install from inside the app, run this once in Terminal after moving the app to Applications:
```
xattr -dr com.apple.quarantine /Applications/PlushBounce.app
```
If updating 0.5.0 fails with "Read-only file system", quit PlushBounce, run the command above, and open it again.

## Requirements
- Windows 10/11 (x64) with the WebView2 runtime (already on Windows 11; the installer fetches it if missing)
- macOS 14.2 or later
