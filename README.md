<h1 align="center">Hihu Island</h1>

<h3 align="center">A light notch for Windows</h3>

<p align="center">A small island hangs from the top of the screen, shows what's playing and what's coming up, and opens into quick pages when you point at it.</p>

Hihu Island is a personal fork of [Oyenza Studio](https://github.com/lucacadalora/oyenza-studio) by Jatevo Labs (MIT). It keeps only the features listed below, and aims to stay as light as it can: native Rust, drawn with Direct2D, no web view, no account.

It is not Oyenza Studio and is not made by its authors. It doesn't use Oyenza's name, logo or the Oyen character.

## Features

- **The island.** Grows when you hover, stays click-through around it, and shows a clock, eyes or nothing at rest. Pop-ups for volume, battery and Bluetooth. Hides or stays on top over full-screen apps. Settings, a tray icon and Start with Windows.
- **Music.** Any player Windows' media controls know (SMTC), synced lyrics from [LRCLIB](https://lrclib.net), Latin readings under lyrics in other scripts, and Apple Music lyrics with your own token.
- **Today.** A calendar: the day, date, year, week number and five weeks starting this Monday. It reads nothing and only wakes at midnight.
- **Shelf and phone drop.** Keep files on the island, send files from your phone over Wi-Fi, or pair a phone to send from any network.
- **Add-ons**, every feature's switch in one place: Tasks, Focus and Notifications (on until switched off), Clipboard history (as many copies and for as long as you set), the Script reader, Prayer times (your location or a place you pick), Japanese readings and the Downloader (yt-dlp). Off, a feature doesn't run.
- **Notifications** from the apps you pick, on the island. Windows lets only an app with a package identity read them: see [Notifications](#notifications).
- **Themes:** Classic, Glass, Pill and Diorama.

## Your data stays on your computer

No account, no ads, no analytics. Lyrics lookups go to LRCLIB while a song plays. Everything else that goes online (Apple Music lyrics, paired phones, add-on downloads, cities for prayer times) does so only after you switch it on. [PRIVACY.md](PRIVACY.md) lists every request.

Settings and data live in `%APPDATA%\Hihu\`, the cache and the log in `%LOCALAPPDATA%\Hihu\`.

## Apple Music lyrics

Lyrics come from LRCLIB by default. With an Apple Music subscription, Hihu Island can show Apple Music's own synced lyrics instead, whichever app is playing. LRCLIB stays the backup for songs Apple has no synced lyrics for.

It needs your Apple Music token, copied once from the web player:

1. Open [music.apple.com](https://music.apple.com) in Chrome or Edge and sign in.
2. Press F12, open **Application** → **Cookies** → `https://music.apple.com`, and copy the value of `media-user-token`.
3. Right-click the island (or the tray icon) → **Apple Music lyrics** → **Use token from clipboard**.

Hihu Island checks the token with Apple and says whether it worked. It stores the token encrypted for your Windows account (`%LOCALAPPDATA%\Hihu\apple-music-token.bin`) and clears it from the clipboard. If Windows clipboard history (Win+V) is on, delete it there too. When Apple stops accepting the token, the app says so and you copy a fresh one. **Remove token** in the same menu turns this off. It uses the web player's own API, which Apple doesn't publish for other apps, so it can stop working when Apple changes its site.

## Notifications

Windows lets only an app with a package identity read other apps' notifications. A copy you build has none, so `tools\identity.ps1` registers a small identity package for it (`installer\identity`): the exe stays where it is, its data stays in `%APPDATA%\Hihu`, and nothing is installed or copied. It needs Developer Mode (Settings → System → For developers), not a certificate or admin rights.

1. Close Hihu Island.
2. `powershell -ExecutionPolicy Bypass -File tools\identity.ps1 register`
3. Start it again, open Add-ons, and allow Notifications when Windows asks. Then pick the apps whose notifications show on the island.

Rebuilding keeps the registration; `tools\identity.ps1 unregister` removes it.

## Build

Windows 10 or 11, Rust stable with the MSVC toolchain:

```
cargo build -p poni --release        # target\release\poni.exe
cargo test --workspace --release
```

The exe and the crates keep Oyenza's `poni` names. Close Hihu Island before you build: Windows can't replace `poni.exe` while it runs.

| Crate | What |
|---|---|
| `crates/poni-core` | Logic with no Windows in it (springs, lyrics, Pomodoro, tasks, settings…), Phone drop's small web server and paired phone drop. Unit-tested on any OS. |
| `crates/studio-diorama` | The Diorama theme's picture, in plain Rust with no dependencies. |
| `crates/poni` | The app: Win32, Direct2D and WinRT through [windows-rs](https://github.com/microsoft/windows-rs). The Mac code in `src/mac/` comes from Oyenza and isn't kept up in this fork. |

`poni.exe --selftest <dir>` runs a scripted tour of every state with fake data, writes a screenshot of each, and a `report.json` with layout and timing checks. [ARCHITECTURE.md](ARCHITECTURE.md) explains how the pieces fit (it still describes Oyenza Studio).

## Credits

- Hihu Island is built on [Oyenza Studio](https://github.com/lucacadalora/oyenza-studio) by Jatevo Labs and its contributors (MIT).
- Synced lyrics come from [LRCLIB](https://lrclib.net), a free and open lyrics database.
- With your token, lyrics can also come from [Apple Music](https://music.apple.com).
- Phone drop follows the design of [Windfall](https://github.com/UvejsGj/Windfall)'s phone page (MIT). QR codes come from [qrcodegen](https://github.com/nayuki/QR-Code-generator) (MIT).
- Romaji under Japanese lyrics comes from Windows' own reading analyzer or, on PCs without Windows' Japanese data, from [mecab-ipadic](https://taku910.github.io/mecab/) read with [vibrato](https://github.com/daac-tools/vibrato) (MIT/Apache-2.0).
- Paired phones follow the idea of [PairDrop](https://github.com/schlagmichdoch/PairDrop)'s paired devices (no code taken). The WebRTC link is [str0m](https://github.com/algesten/str0m) (MIT or Apache-2.0).
- Prayer times use the sun's position and calculation methods from [Adhan](https://github.com/batoulapps/adhan-js) (MIT), Kemenag RI's rules, and reference points fitted to Kemenag's published schedule. Cities outside Indonesia come from [GeoNames](https://www.geonames.org) ([CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)), their time zones from the IANA time zone database. The adzan is "The Adhan - Muslim Call to Prayer - Aaqib Azeez" by Atcovi, from [Wikimedia Commons](https://commons.wikimedia.org/wiki/File:The_Adhan_-_Muslim_Call_to_Prayer_-_Aaqib_Azeez.mp3), [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/).
- Built with [windows-rs](https://github.com/microsoft/windows-rs) and [serde](https://serde.rs).

## License

The code is [MIT](LICENSE), copyright its original authors and this fork's changes. The Oyenza Studio name, its logo and the Oyen character are not covered by the MIT license and are not part of Hihu Island. The Japanese reading dictionary (`crates/poni-core/data/ja-readings.dic.zst`, the Japanese readings add-on) is derived from mecab-ipadic and keeps its own license, which the add-on's `NOTICE.txt` carries. [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt) holds that license and those of the Unicode data and the Rust crates the app is built from.
