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

Dipertahankan:
- Inti island: hover membesar/mengecil, click-through, tampilan saat diam, pop-up volume/baterai/Bluetooth, perilaku layar penuh (Hide atau Stay on top), Settings, tray, start with Windows.
- Music: SMTC, lirik sinkron (LRCLIB), bacaan latin, lirik Apple Music.
- Today (24 Nov 2026): kalender sebulan (Senin pertama, panah bulan), tanggal merah libur nasional + Minggu, cuti bersama merah muda (`poni_core::agenda::HOLIDAYS` dari SKB 3 Menteri di setneg.go.id, lengkap 2026–2027; setelah itu hanya tanggal tetap, perlu ditambah tiap tahun), countdown sampai 3 tanggal merah dalam 60 hari, event sendiri (sekali atau tiap N hari/minggu/bulan/tahun) disimpan di `data.json` (`store.agenda`). Jam ditulis di depan judul ("09:30 Rapat"); pengingat per event (Off/On time/10m/30m/1h; event seharian jam 07:00) muncul sebagai notice + chime lewat satu timer sekali-jalan `TIMER_AGENDA` ke pengingat berikutnya (maks 6 jam lalu dijadwal ulang). Halaman hanya bangun saat tengah malam.
- Shelf + Phone drop + pairing HP.
- Tasks, Focus, Notifications, Clipboard, Script, Bacaan Jepang, Waktu sholat: semua toggle ada di halaman Add-ons (selesai 23 Nov 2026). Tasks, Focus, Notifications add-on bawaan yang nyala sejak awal; yang mati tidak jalan (tab, section Settings, timer Focus, listener Notifications). Settings → Tabs tinggal Music, Today, Shelf.
- Waktu sholat: lokasi otomatis dan pilih manual.
- Tema: Classic, Smoke (hitam semi transparan, island musik dua baris: cover, jam, equalizer; lirik; progress bar), Glass, Pill, Diorama. Tema Oyen sudah dihapus.
- Island: Position dan Lock position, Size when closed (90/100/115 %), Transparency Smoke (40/55/75 %), Make way for the pointer (menyusut jadi garis tipis di tepi atas saat didekati mouse, terbuka saat mouse didorong ke tepi atas).
- Clipboard history (add-on, mati sejak awal; selesai 23 Nov 2026): pengawas `WM_CLIPBOARDUPDATE` di thread sendiri yang hanya jalan saat nyala, menghormati format "jangan rekam" password manager; Settings → Clipboard: 10/25/50/100 item, 1 jam/1 hari/7 hari/Forever; disimpan terenkripsi DPAPI di `%LOCALAPPDATA%\Hihu\clipboard.bin`, dihapus saat add-on dimatikan. Kode dasarnya dari Oyenza sebelum `a30fcc4`.
- Screen time (add-on, mati sejak awal; selesai 23 Nov 2026): menumpang hook `EVENT_SYSTEM_FOREGROUND` milik `services/fullscreen.rs` (`report_foreground`), nama app dari FileDescription exe, app Store lewat anak jendela ApplicationFrameHost; berhenti saat terkunci/layar mati/idle 5 menit; cek tiap 60 dtk hanya saat nyala dan ada orang; 7 hari di `%APPDATA%\Hihu\screentime.json`, dihapus saat add-on dimatikan.
- Meeting notes (add-on, mati sejak awal; selesai 23 Nov 2026): rekam mic + suara PC (WASAPI loopback, kode lama Oyenza dari `e031e17^` dan mic IELTS dari `382c41a^`), bagian WAV 5 menit di `%LOCALAPPDATA%\Hihu\notes`, lalu Gemini API dengan kunci user sendiri (Files API resumable upload + `generateContent` dengan JSON schema; model `gemini-flash-latest`/`pro`/`flash-lite` di Settings → Meeting notes, bahasa catatan Indonesia/Inggris, bawaan Indonesia; transkrip tetap seperti diucapkan). Kunci ditempel dari clipboard, disimpan DPAPI di `%LOCALAPPDATA%\Hihu\gemini.key`. Upload dihapus setelah selesai; catatan Markdown di `Documents\Hihu Island\Notes`. Maks 2 jam. Exe +276 KB.
- Kartu notifikasi: foto pengirim (selesai 23 Nov 2026) dari gambar `appLogoOverride` toast, dibaca dari `wpndatabase.db` per id notifikasi lewat `winsqlite3.dll` bawaan Windows (`services/wpndb.rs`, exe hanya +18 KB); hanya file lokal, gambar internet tidak diambil.
- Notifications tidak punya tab lagi; semua setelannya di Settings → Notifications (akses Windows, Who/Message, Every app, New apps start on, Hide Windows' pop-ups, daftar app). Suara notifikasi tetap dari Windows.
- Notifications: Every app (bawaan nyala) dan Hide Windows' pop-ups (bawaan nyala, seperti `notifHideBanners` Poni): `ShowBanner=0` di `HKCU\...\Notifications\Settings\<AUMID>` untuk app yang tampil di Hihu, nilai asli di `%APPDATA%\Hihu\banners.json`, dikembalikan saat app/opsi/add-on mati dan saat Hihu quit (`services/banners.rs`). Dengan Every app, semua app yang dikenal Windows (subkey di sana, kecuali id bersarang `{…}`) ikut dimatikan sejak awal; alarm, panggilan, dan Phone Link dibiarkan (`banners::keeps_its_popup`) karena butuh tombolnya. App yang belum pernah kirim notif sama sekali masih muncul sekali di Windows.
- Settings → Tabs: semua tab (termasuk add-on) bisa diurutkan (drag pegangan atau panah) dan dinyalakan; `tab_order` di settings.json, Add-ons selalu terakhir.
- Menu tray ringkas: Settings…, Keep island open, Start with Windows, Quit. Token Apple Music pindah ke Settings → Music and lyrics.
- Pop-ups (selesai 23 Nov 2026): Take over the volume keys (hook `WH_KEYBOARD_LL` di `services/keys.rs`, menahan tombol volume lalu `audio::volume_key` 2 %; dilepas ke Windows saat island sembunyi di layar penuh), Caps/Num/Scroll Lock, bahasa keyboard (Win+Space, Alt+Shift, Ctrl+Shift, dicek 250 ms kemudian), brightness (`GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS` lewat `RegisterPowerSettingNotification`, tanpa WMI). Hook hanya ada selama salah satu opsi keyboard nyala. Pop-up volume dan brightness Windows disembunyikan (Hide Windows' volume and brightness pop-ups, eksperimental, `services/osd.rs`): jendela explorer `XamlExplorerHostIslandWindow` tanpa judul berukuran kecil di tengah bawah di-minimize, seperti HideVolumeOSD dan mod Windhawk; Hihu hanya melakukannya saat start dan saat brightness berubah (8x tiap 120 ms), bukan polling, dan mengembalikannya saat opsi mati atau quit.
- Seperti Poni (24 Nov 2026): Smoke lebih kecil (300–520 × 60, baris 16/38), progress bar di tepi bawah dengan warna equalizer; lebar island ikut baris lirik sekarang dan membesar 0,45 dtk sebelum baris yang lebih panjang (`lyric::upcoming`), mengecil bersama baris pendek (bukan lagi "terlebar sejauh ini"); notice dan countdown sholat ditumpuk dengan baris lirik di bawahnya saat lagu ber-lirik main (`activities::stacks`, bukan tema Pill); momen sholat dan awal countdown bercahaya oranye 4 dtk (`draw_glow`); Show over full-screen apps: pop-up tetap muncul saat island sembunyi di layar penuh (`sync_fullscreen_hud`), tombol volume tetap diambil alih.
- Music → Two rows (bawaan nyala): tata letak dua baris Smoke juga di Classic dan Glass (`smoke::two_rows`); Clock untuk menyembunyikan jam di tengahnya.
- Pop-up mikrofon (Microphone on and off): mute endpoint rekam bawaan lewat callback `IAudioEndpointVolume` di thread audio yang sudah ada.
- Notifikasi terakhir: 5 terbaru di memori saja (`st.recent`), lonceng di header island dengan angka baru, angka merah di island diam, halaman Notifications (tanpa tab) dengan Clear dan klik buka app.
- Jendela Settings: sidebar dikelompokkan ISLAND / FEATURES; ketik apa saja untuk mencari (`settings_window::search`), Esc menghapus pencarian dulu.
- Pop-up kamera (Camera on and off): thread `poni-camera` menunggu `RegNotifyChangeKeyValue` di `CapabilityAccessManager\ConsentStore\webcam` (LastUsedTimeStop = 0 berarti sedang dipakai), nama app dari FileDescription exe.
- Island → Opens on: Hover (bawaan) atau Click seperti Poni (`open_on_click`; hover tidak lagi membuka).
- General: Show in screenshots (mati = island tidak ikut screenshot, rekaman, screen share).
- Music: setting Frame rate, ukuran dan bold lirik, progress bar (Smoke), After pausing (0/5/10/30 dtk/Keep), Title without lyrics (Stays/A moment; Smoke jadi satu baris), running text untuk judul panjang.
- Notifications: jalan di build non-Store lewat paket identitas sparse (selesai 23 Nov 2026): `tools\identity.ps1 register` (butuh Developer Mode, tanpa sertifikat/admin), manifest di `installer\identity`, elemen `<msix>` di `res/poni.manifest`. `autostart::packaged()` = punya identitas (Notifications); `store_packaged()` = copy Store (startup task, tanpa Downloader). Build ulang tetap terdaftar.

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
- `target\release\hihu.exe` tidak bisa ditimpa saat app jalan. Tutup Hihu dulu sebelum build, lalu jalankan lagi.
- File di repo memakai CRLF. Skrip edit harus mempertahankan CRLF, atau keluarkan file dari working copy lalu `git checkout` lagi setelah commit.

