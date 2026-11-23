# Hihu Island privacy

Hihu Island needs no account, and has no ads, no analytics and no tracking. It has no server of its own. What it sends, and only when, is below.

Hihu Island is a fork of Oyenza Studio. A few features still use servers that Oyenza Studio runs (paired phones, cities for prayer times, add-on downloads); they are named below.

## What leaves your PC

**Lyrics lookups.** When a song plays and **Synced lyrics** is on, Hihu Island asks [LRCLIB](https://lrclib.net), a free public lyrics library, for that song's lyrics. The request carries the song's title, artist, album and length, plus a User-Agent that names Hihu Island and links to this project. LRCLIB is run by other people and has its own terms. Answers are cached on your PC, so a song isn't looked up again every time it plays. To stop lookups, right-click the island or the tray icon and turn off **Synced lyrics**.

**Apple Music lyrics, only if you turn them on.** If you add your Apple Music token (right-click the island, then **Apple Music lyrics › Use token from clipboard**), Hihu Island also asks Apple Music for lyrics the way Apple's web player does: it checks the token, searches Apple Music's catalog for the playing song's artist and title, and downloads that song's lyrics. These requests go to Apple (`music.apple.com` and `amp-api.music.apple.com`) and carry your token; the token goes nowhere else. To stop, choose **Remove token** in the same menu.

**Phone drop, only while its panel is open.** When you open Phone drop on the Shelf, Hihu Island serves a small web page on your local network so your phone can send files to the Shelf and save Shelf files. Only a device on the same network with that session's secret link, which only the QR code carries, can open it. Files go straight between your phone and your PC, never through the internet. Nothing from an upload is read until you click **Accept**. The page stops working when you click **Done**, or after 10 minutes without a phone.

**Paired phones, only after you pair one.** **Pair a phone** (Shelf › Phone) shows a QR code with a pairing secret made on your PC. The phone that scans it keeps the secret, and so does your PC, sealed so only your Windows account can read it. From then on, while Hihu Island runs, it keeps one request open at Oyenza Studio's meeting server, `drop.oyenza.com`, so the phone can find the PC from any network. The server sees your PC's and phone's IP addresses, when they are online, and an address for the pair derived from the secret; it never sees the secret itself, a device name, a file name or a file. Every message through it is signed with the secret. Files then go straight between phone and PC, encrypted by WebRTC with keys made on the two devices. When both networks block a direct link, they pass through a relay run by Cloudflare that forwards the encrypted bytes and can't read them. To find its own internet address, Hihu Island also asks a public STUN server (Cloudflare's or Google's) when a phone connects. **Remove** a phone in **Paired phones** and the pair stops working on both sides; with no paired phones, nothing is sent.

**Add-ons, only the ones you switch on.** Switching an add-on on downloads its files from Oyenza Studio's releases on GitHub, or, for the Downloader's yt-dlp and QuickJS-ng, from those projects' own releases there (`github.com`, served from `release-assets.githubusercontent.com`). Nothing about you goes with the request beyond what any download carries (your IP address and a User-Agent naming Hihu Island). Each file is checked against the size and fingerprint built into the app and refused if it differs. Switching the add-on off deletes its files. While every add-on is off, nothing is downloaded.

**Prayer times' location, only while that add-on is on.** To pick the nearest kab/kota or city, Hihu Island asks Windows where the PC is, through Windows' own location service (Settings › Privacy & security › Location decides whether it may). It asks when the add-on starts and again every few hours. The point is used on your PC to choose the nearest place, then dropped: it isn't saved and is sent nowhere. Kemenag's kab/kota are in the add-on; cities outside Indonesia are looked up online, a small file at a time, from Oyenza Studio's website, `studio.oyenza.com/places/` (hosted by GitHub Pages). Away from Kemenag's places, the app asks for the cities of the 5-degree squares (about 550 km across) around it; typing a place asks for the cities whose names start with the first two letters typed. Those requests carry nothing about you beyond what any page view carries (your IP address and a User-Agent). Only the chosen place is saved, in your settings. Picking a place by hand stops the asking.

**Downloader, only while a download runs.** The Downloader add-on saves the video or sound of a link you paste with yt-dlp, a program that runs on your PC only while a download does. yt-dlp asks the site of that link for its page and downloads the video or sound from wherever that site keeps it. These requests go straight from your PC to the site, as your browser's would: the site sees your IP address, and it has its own terms. They carry none of your browser's cookies or sign-ins. Finished files go to your Downloads folder and onto the Shelf. Switching the Downloader off deletes yt-dlp and its other tools; the files you downloaded stay.

**The Glass theme reads the screen behind the island, only while it's on.** To bend what's under the island, Hihu Island copies the part of your screen behind its window whenever something there changes. It doesn't copy while the island is hidden, while a full-screen app is in front, or while the PC is locked. The copy stays in your PC's memory and on its graphics card, and each new copy replaces the last. It is never saved, shown anywhere else or sent anywhere. While Glass is on, Windows leaves Hihu Island out of screenshots, screen recordings and screen sharing. Switch to another theme to stop both.

**Notifications, only once you allow it in Windows.** Windows lets only an app with a package identity read other apps' notifications: a Microsoft Store copy, or a copy you built after `tools\identity.ps1` registered its identity package. Such a copy reads nothing until you allow it when Windows asks; then it reads each new notification as it arrives (the app's name and the text), shows the ones from the apps you picked on the island for a few seconds, and sends nothing anywhere. Take it back in Windows Settings > Privacy & security > Notifications, switch Notifications off in Add-ons, or unregister the identity package.

Nothing else is sent anywhere. Music, volume, Bluetooth battery levels and the calendar are read from Windows on your PC.

## What stays on your PC

| What | Where |
|---|---|
| Settings, tasks, focus count and the Shelf's list of files (the files themselves stay where they are) | `%APPDATA%\Hihu\` |
| Lyrics cache and a log file (the log names songs that were looked up, to help fix problems) | `%LOCALAPPDATA%\Hihu\` |
| Your Apple Music token, if you added one, sealed so only your Windows account on this PC can read it | `%LOCALAPPDATA%\Hihu\` |
| Paired phones (their pairing secrets and names), sealed the same way | `%LOCALAPPDATA%\Hihu\` |
| Files of the add-ons you switched on, deleted when you switch the add-on off | `%LOCALAPPDATA%\Hihu\addons\` |
| The Glass theme's copy of the screen behind the island, only while Glass is on | Memory only, never written to disk |
| Files you accepted from your phone, and files your paired phones sent | `Downloads\Hihu Island\` |
| Files you saved with the Downloader | Your Downloads folder |
