The notes for every coding agent live in AGENTS.md:

@AGENTS.md

# Fork pribadi (Dapirz/oyen-aja)

Repo ini fork pribadi dari `lucacadalora/oyenza-studio` (remote `upstream`). Tujuannya app notch Windows yang ringan, hanya berisi fitur di bawah. Aturan di bagian ini menang atas AGENTS.md kalau bertentangan.

## Sebutan

- **Oyenza** = Oyenza Studio, app asli repo ini (dulu Oyen Poni; di kode masih `poni` untuk nama crate). Exe fork ini `hihu.exe` (sejak 23 Nov 2026).
- **Poni** = Poni buatan Bos Digital (poni.web.id). App lain, closed source, WebView2.
- **Oyen** = karakter kucing dan tema Oyen di Oyenza, bukan nama app.
- `poni` dalam huruf kode (crate, folder) = kode Oyenza.

## Alur branch

- `main` = versi fork ini. `upstream/main` = acuan Oyenza, hanya dibaca.
- Satu fitur per branch dari `main`, merge kembali ke `main` setelah dites.
- Ambil update Oyenza: `git fetch upstream`, lalu `git cherry-pick` commit yang dibutuhkan (merge penuh akan sering konflik karena banyak fitur dihapus).

## Yang tidak berlaku dari AGENTS.md

- Rilis, tag, website (`site/`), `latest.json`, Mac signing, creator links: tidak dipakai di fork ini.
- "Names that stay": tidak berlaku. Nama folder data, mutex, registry dan uninstall key harus diganti ke nama fork supaya tidak bentrok dengan Poni (Bos Digital) atau Oyenza Studio resmi. Keduanya memakai `%APPDATA%\Poni` dan `%LOCALAPPDATA%\Poni`, dan uninstaller Oyenza menghapus `%LOCALAPPDATA%\Poni` seluruhnya.
- Aturan "Keep it light" tetap berlaku.

## Fitur

Per area. Semua add-on punya switch di halaman Add-ons; yang mati tidak jalan sama sekali (tab, section Settings, thread, timer).

### Island dan tema
- Inti: hover atau klik membuka (Settings → Island → Opens on: Hover/Click, `open_on_click`), click-through di sekitarnya, tampilan saat diam (Clock/Eyes/Minimal/Hidden), Position dan Lock position, Size when closed (90/100/115 %), Make way for the pointer (menyusut jadi garis tipis di tepi atas; terbuka saat mouse didorong ke tepi atas), layar penuh Hide atau Stay on top, Show in screenshots, tray, Start with Windows.
- Tema: Classic, Smoke (hitam semi transparan, Transparency 40/55/75 %), Glass, Pill, Diorama. Tema Oyen sudah dihapus.
- Tab: Settings → Tabs mengurutkan semua tab (drag pegangan atau panah) dan menyalakannya; `tab_order` di settings.json, Add-ons selalu terakhir. Notifications tidak punya tab.
- Menu tray ringkas: Settings…, Keep island open, Start with Windows, Quit.

### Pop-up (Settings → Pop-ups)
- Volume, baterai/charger, Bluetooth, Caps/Num/Scroll Lock, bahasa keyboard (Win+Space, Alt+Shift, Ctrl+Shift; dicek 3x tiap 300 ms lewat `TIMER_LANGUAGE` di jendela island), brightness (`GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS` lewat `RegisterPowerSettingNotification`, tanpa WMI), mikrofon on/off (mute endpoint rekam lewat callback `IAudioEndpointVolume` di thread audio), kamera on/off (thread `poni-camera` menunggu `RegNotifyChangeKeyValue` di `CapabilityAccessManager\ConsentStore\webcam`; LastUsedTimeStop = 0 berarti dipakai; nama app dari FileDescription exe). Penutup kamera geser (kill switch) tidak bisa dideteksi tanpa menyalakan kamera, Poni juga tidak.
- Volume dan brightness tampil seperti Poni: ikon + nama, bar pendek, angka; ditumpuk di atas baris lirik saat lagu main.
- Take over the volume keys: hook `WH_KEYBOARD_LL` (`services/keys.rs`, ada hanya selama opsi keyboard nyala) menahan tombol volume lalu `audio::volume_key` 2 %.
- Hide Windows' volume and brightness pop-ups (eksperimental, `services/osd.rs`): jendela explorer `XamlExplorerHostIslandWindow` tanpa judul berukuran kecil di tengah bawah di-minimize, seperti HideVolumeOSD dan mod Windhawk; hanya saat start dan saat volume/brightness berubah (8x tiap 120 ms), bukan polling; dikembalikan saat opsi mati, Quit, dan shutdown/logout Windows.
- Show over full-screen apps: pop-up tetap muncul saat island sembunyi di layar penuh (`sync_fullscreen_hud`), tombol volume tetap diambil alih.

### Music
- SMTC, lirik sinkron (LRCLIB), bacaan latin, lirik Apple Music (token ditempel di Settings → Music and lyrics).
- Two rows (bawaan nyala) di Classic, Smoke, Glass (`smoke::two_rows`): cover, jam (bisa dimatikan: Clock), equalizer, lalu lirik; progress bar di tepi bawah dengan warna equalizer. Ukuran 300–680 × 60.
- Lebar ikut baris lirik sekarang: membesar 0,45 dtk sebelum baris yang lebih panjang (`lyric::upcoming`), mengecil bersama baris pendek; maks 680 (`pill::media::MAX_W`), jendela island ikut selebar itu.
- Setting: Frame rate, ukuran dan bold lirik, After pausing (0/5/10/30 dtk/Keep), Title without lyrics, running text untuk judul panjang.
- Suara notifikasi situs (misalnya WhatsApp Web di browser, judulnya "web.whatsapp.com") tidak dianggap musik: `MediaSession::is_blip` (< 20 dtk, atau judul nama situs tanpa artis).
- Notice atau countdown sholat yang muncul saat lagu ber-lirik main ditumpuk dengan baris lirik di bawahnya (`activities::stacks`, bukan tema Pill).

### Today (kalender)
- Kalender sebulan (Senin pertama, panah ‹ ›): hari ini diisi, hari dipilih dicincin, tanggal merah libur nasional + Minggu, cuti bersama merah muda, titik untuk hari dengan event/task. Hanya bangun saat tengah malam.
- Libur: `poni_core::agenda::HOLIDAYS` dari SKB 3 Menteri di setneg.go.id, lengkap 2026–2027; setelah itu hanya tanggal tetap (lihat Nanti). Countdown sampai 3 tanggal merah dalam 60 hari.
- Event sendiri (sekali atau tiap N hari/minggu/bulan/tahun) di `data.json` (`store.agenda`); task bertanggal (`Task.due`: hari, jam, pengingat) dibuat dari Today lewat tombol jenis Event/Task, tampil juga di Tasks ("Today 09:30", merah kalau lewat).
- Jam dari pemilih (klik → kelipatan 5 menit berikutnya, −/+ atau scroll) atau diketik di depan judul ("09:30 Rapat"). Pengingat Off/On time/10m/30m/1h (event seharian jam 07:00) jadi notice + chime lewat satu timer sekali-jalan `TIMER_AGENDA` (maks 6 jam lalu dijadwal ulang; `agenda::reminders` untuk event dan task).
- Daftar hari dipilih: satu daftar event + task yang bisa di-scroll, dengan edit (pensil) dan hapus saat kursor di atasnya; klik task mencentangnya.

### Notifications (add-on bawaan, nyala)
- Jalan di build non-Store lewat paket identitas sparse: `tools\identity.ps1 register` (butuh Developer Mode, tanpa sertifikat/admin), manifest di `installer\identity`, elemen `<msix>` di `res/poni.manifest`. `autostart::packaged()` = punya identitas; `store_packaged()` = copy Store. Oyenza asli hanya bisa di copy Store.
- Kartu notifikasi di bawah aktivitas, dengan foto pengirim dari gambar `appLogoOverride` toast, dibaca dari `wpndatabase.db` per id lewat `winsqlite3.dll` bawaan Windows (`services/wpndb.rs`); hanya file lokal.
- Every app (bawaan nyala) dan Hide Windows' pop-ups (bawaan nyala, seperti `notifHideBanners` Poni): `ShowBanner=0` di `HKCU\...\Notifications\Settings\<AUMID>` untuk app yang tampil, termasuk semua app yang dikenal Windows (kecuali id bersarang `{…}`); alarm, panggilan, dan Phone Link dibiarkan (`banners::keeps_its_popup`). Nilai asli di `%APPDATA%\Hihu\banners.json`, dikembalikan saat opsi/add-on mati, Quit, dan shutdown/logout. Suara notifikasi tetap dari Windows.
- Notifikasi terakhir: 5 terbaru di memori saja (`st.recent`), lonceng di header island dengan angka baru (hanya saat island dibuka), halaman Notifications tanpa tab dengan Clear dan klik buka app.
- Semua setelan di Settings → Notifications (akses Windows, Who/Message, Every app, New apps start on, Hide Windows' pop-ups, daftar app).

### Add-on lain
- Tasks, Focus (bawaan nyala), Script reader, Bacaan Jepang, Waktu sholat (lokasi otomatis atau pilih manual; countdown "Dzuhur in 1:59" digambar ulang tiap detik hanya selama tampil; momen sholat dan awal countdown bercahaya oranye 4 dtk, `draw_glow`).
- Clipboard history (mati sejak awal): pengawas `WM_CLIPBOARDUPDATE` di thread sendiri, menghormati format "jangan rekam" password manager; 10/25/50/100 item, 1 jam/1 hari/7 hari/Forever; DPAPI di `%LOCALAPPDATA%\Hihu\clipboard.bin`, dihapus saat dimatikan. Kode dasar dari Oyenza sebelum `a30fcc4`.
- Screen time (mati sejak awal): menumpang hook `EVENT_SYSTEM_FOREGROUND` milik `services/fullscreen.rs`; nama app dari FileDescription exe, app Store lewat anak jendela ApplicationFrameHost; berhenti saat terkunci/layar mati/idle 5 menit; cek tiap 60 dtk hanya saat nyala; 7 hari di `%APPDATA%\Hihu\screentime.json`, dihapus saat dimatikan.
- Meeting notes (mati sejak awal): rekam mic (Windows' default atau dipilih lewat menu di Settings → Meeting notes) + suara PC (WASAPI loopback, kode lama Oyenza dari `e031e17^` dan mic IELTS dari `382c41a^`), bagian WAV 5 menit di `%LOCALAPPDATA%\Hihu\notes`. Setelah Stop ditanya dulu (Write notes / Throw away). Gemini dengan kunci user sendiri (Files API resumable upload + `generateContent` dengan JSON schema; Flash/Flash-Lite/Pro, Flash-Lite saat model sibuk; catatan Indonesia/Inggris), kunci DPAPI di `%LOCALAPPDATA%\Hihu\gemini.key`; upload dihapus setelah selesai; catatan Markdown di `Documents\Hihu Island\Notes`, daftar catatan terakhir di halaman Notes (buka, atau ke Recycle Bin). Maks 2 jam.

### Settings, data, backup
- Jendela Settings: sidebar ISLAND / FEATURES, ketik apa saja untuk mencari (`settings_window::search`; Esc menghapus pencarian dulu), dropdown (`Ctl::Menu`, menu Win32 `tray::pick`) untuk pilihan panjang seperti mic.
- Backup (Settings → About): Export ke `Documents\Hihu Island\Backups\Hihu backup YYYY-MM-DD.json` (settings + data.json, `poni_core::backup`); Import memakai backup terbaru di folder itu, ditekan dua kali. Kunci DPAPI (Gemini, Apple Music) dan pasangan HP tidak ikut.

Sudah dihapus (selesai 22 Nov 2026, satu commit per fitur di `main`, `git log 1790ee2..ba747ac`):
- Stocks, Caffeine, Health, Meeting notes, IELTS, AI usage (termasuk crate `studio-usage`), Screen recorder (Record), System.
- Show over games, beserta pembaca CPU/GPU (`sysmon`, `psu`) dan penghitung frame.
- Tab Oyenza, login dan sign-in, notifikasi uang dan tagihan, Studio Pro (`Addon::pro`), dan bagian Today yang butuh login.
- Updater ke studio.oyenza.com. Update Hihu cukup dengan build ulang.
- Downloader (yt-dlp) beserta halaman Downloads dan pill download (dihapus 23 Nov 2026; user sudah punya app downloader sendiri). Tool lama di `%LOCALAPPDATA%\Hihu\addons\downloader` dihapus sekali saat start.
- Ukuran poni.exe: 9,55 MB (sebelum Stocks) → 6,61 MB.
- Kalau butuh kode yang sudah dihapus (misalnya perekaman Meeting notes untuk versi sendiri), ambil dari commit sebelum penghapusannya, contoh `git show 0800e50:crates/poni/src/services/notes.rs`.

Sisa brand Oyenza/Oyen di app sudah dibersihkan (`2e062b5`): tema Oyen, `oyen::app_icon` diganti ikon Hihu (`logo.rs`), `docs/brand`, `res/poni.ico`, User-Agent, README dan PRIVACY. Tools rilis, website, CI, installer dan aset brand Oyenza juga sudah dihapus (23 Nov 2026); yang tersisa: `site/places` (data kota dunia, dipakai tes sholat), `server/` (kode drop.oyenza.com), komentar kode yang menyebut asal kode, dan ARCHITECTURE.md.

Fork ini masih memakai server Oyenza: `drop.oyenza.com` (pairing HP), `studio.oyenza.com/places` (kota sholat di luar Indonesia), release GitHub `lucacadalora/oyenza-studio` (file add-on), `quran.oyenza.com` (link).

Identitas sendiri sudah selesai (`1790ee2`): nama app Hihu Island, ikon, folder data, mutex, kelas window, Run value dan uninstall key baru.

Nanti (belum dikerjakan):
- Libur nasional 2028: setelah SKB 3 Menteri 2028 terbit (biasanya September 2027, di setneg.go.id), tambahkan tanggal libur dan cuti bersama 2028 ke `poni_core::agenda::HOLIDAYS` dan naikkan `LISTED_UNTIL`. Ulangi tiap tahun. Sebelum itu, 2028 hanya punya tanggal tetap (1 Jan, 1 Mei, 1 Jun, 17 Agu, 25 Des).
- Glass saat musik jalan sudah dihemat (`git log -1 --grep Glass`): salinan layar paling banyak 30x/detik, 10x saat tidak berubah. Ukuran 22 Nov 2026 (144 Hz, 60 fps, Mcycles/s): Glass 1137 -> 390, Classic sekitar 330. Sisa biaya per frame (~1,8 juta siklus) adalah Direct2D draw + present, sama di semua tema; yang bisa menekannya hanya setting Frame rate. Ukur dengan `tools\measure_cpu.ps1`.
- RAM: ukur private working set (kolom Memory di Task Manager), bukan working set yang ikut menghitung DLL bersama. 23 Nov 2026, Classic, baru dibuka: private working set 23,8 MB (working set 87 MB, private bytes 74 MB). Sudah masuk target 15–30 MB; jaga supaya tetap di situ. 24 Nov 2026 (Smoke, add-on Screen time + Sholat nyala, semua fitur Pop-ups dan Notifications nyala): private working set 24,2–24,3 MB; CPU saat diam 2 Mcycles/s (0,07 % satu core), saat musik 60 fps 391 Mcycles/s (thread `main` ~203 + driver grafis ~190; fitur lain masing-masing < 0,5). Ukur per thread dengan `QueryThreadCycleTime` + `GetThreadDescription`.

Tidak dibuat: radio.

## Aturan kerja

- Target RAM sedekat mungkin dengan 15–30 MB (Oyenza resmi sekitar 30–60 MB). Jangan tambah WebView atau dependency berat.
- Satu perubahan per branch. Setelah edit selalu `cargo build -p poni --release` (toolchain MSVC).
- Jangan hapus LICENSE, THIRD-PARTY-NOTICES.txt atau credit pembuat asli (MIT).
- Jangan pakai nama, ikon atau aset Poni, ReyLand, Nusa, maupun brand Oyenza/Oyen (lihat README bagian License).
- Tes setiap perubahan: click-through di sekitar notch, animasi hover mulus, kontrol media SMTC.
- Kode Mac (`crates/poni/src/mac/`) tidak dirawat: Hihu hanya untuk Windows, dan Mac tidak ikut di-build.

## Cara kerja yang sudah dipakai

- Nama branch `remove-<fitur>` atau `<topik>`. Setelah dites, merge ke `main` dengan `--ff-only` lalu hapus branch-nya. Push ke `origin` hanya setelah ditanyakan ke user.
- Pesan commit dalam bahasa Inggris, gaya repo: apa yang hilang atau berubah, apa yang tetap jalan untuk file settings atau data lama, lalu baris `Cost:` (ukuran hihu.exe sebelum → sesudah, dan apa yang berubah saat diam).
- Sebelum commit: `cargo build -p poni --release` tanpa warning baru (sekarang ada 10 warning lama), lalu `cargo test --workspace --release`.
- `target\release\hihu.exe` tidak bisa ditimpa saat app jalan. Tutup Hihu dulu sebelum build, lalu jalankan lagi. Tutup dengan `WM_CLOSE` ke jendela kelas `Hihu.Island` (sama dengan Quit), jangan `Stop-Process`: kalau dimatikan paksa, banner notifikasi Windows dan bar volume/brightness tidak dikembalikan, jadi selama Hihu mati notifikasi hanya berbunyi tanpa pop-up.
- File di repo memakai CRLF. Skrip edit harus mempertahankan CRLF, atau keluarkan file dari working copy lalu `git checkout` lagi setelah commit.

