The notes for every coding agent live in AGENTS.md:

@AGENTS.md

# Fork pribadi (Dapirz/oyen-aja)

Repo ini fork pribadi dari `lucacadalora/oyenza-studio` (remote `upstream`). Tujuannya app notch Windows yang ringan, hanya berisi fitur di bawah. Aturan di bagian ini menang atas AGENTS.md kalau bertentangan.

## Sebutan

- **Oyenza** = Oyenza Studio, app asli repo ini (dulu Oyen Poni; di kode masih `poni`, `poni.exe`, folder `Poni`).
- **Poni** = Poni buatan Bos Digital (poni.web.id). App lain, closed source, WebView2.
- **Oyen** = karakter kucing dan tema Oyen di Oyenza, bukan nama app.
- `poni` dalam huruf kode (crate, exe, folder) = kode Oyenza.

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
- Today: kalender saja. Hari, tanggal, tahun, nomor minggu, dan lima minggu mulai Senin ini. Tidak membaca apa pun dan hanya bangun saat tengah malam. Tile CPU/RAM/disk sudah dihapus.
- Shelf + Phone drop + pairing HP.
- Tasks, Focus, Script, Bacaan Jepang: tetap ada, bisa dimatikan lewat toggle. Semua toggle fitur dikumpulkan di satu tempat (halaman Add-ons), dan fitur yang mati tidak jalan sama sekali (hemat memori).
- Waktu sholat: lokasi otomatis dan pilih manual.
- Downloader (yt-dlp): add-on, tidak perlu Pro lagi. Nyala setelah diaktifkan di Add-ons dan tools-nya terunduh.
- Tema: tidak diganti (keputusan 22 Nov 2026). Classic, Glass, Pill, Diorama. Tema Oyen sudah dihapus.
- Notifications: dipertahankan, cari cara supaya jalan di build non-Store (sekarang hanya jalan di copy versi Store).

Sudah dihapus (selesai 22 Nov 2026, satu commit per fitur di `main`, `git log 1790ee2..ba747ac`):
- Stocks, Caffeine, Health, Meeting notes, IELTS, AI usage (termasuk crate `studio-usage`), Screen recorder (Record), System.
- Show over games, beserta pembaca CPU/GPU (`sysmon`, `psu`) dan penghitung frame.
- Tab Oyenza, login dan sign-in, notifikasi uang dan tagihan, Studio Pro (`Addon::pro`), dan bagian Today yang butuh login.
- Updater ke studio.oyenza.com. Update Hihu cukup dengan build ulang.
- Ukuran poni.exe: 9,55 MB (sebelum Stocks) → 6,61 MB.
- Kalau butuh kode yang sudah dihapus (misalnya perekaman Meeting notes untuk versi sendiri), ambil dari commit sebelum penghapusannya, contoh `git show 0800e50:crates/poni/src/services/notes.rs`.

Sisa brand Oyenza/Oyen di app sudah dibersihkan (`2e062b5`): tema Oyen, `oyen::app_icon` diganti ikon Hihu (`logo.rs`), `docs/brand`, `res/poni.ico`, User-Agent, README dan PRIVACY. Yang masih ada: `installer/art`, `site/` dan tools rilis/website milik Oyenza (tidak dipakai fork ini), komentar kode yang menyebut asal kode, dan ARCHITECTURE.md.

Fork ini masih memakai server Oyenza: `drop.oyenza.com` (pairing HP), `studio.oyenza.com/places` (kota sholat di luar Indonesia), release GitHub `lucacadalora/oyenza-studio` (file add-on), `quran.oyenza.com` (link).

Identitas sendiri sudah selesai (`1790ee2`): nama app Hihu Island, ikon, folder data, mutex, kelas window, Run value dan uninstall key baru.

Nanti (belum dikerjakan):
- Glass saat musik jalan sudah dihemat (`git log -1 --grep Glass`): salinan layar paling banyak 30x/detik, 10x saat tidak berubah. Ukuran 22 Nov 2026 (144 Hz, 60 fps, Mcycles/s): Glass 1137 -> 390, Classic sekitar 330. Sisa biaya per frame (~1,8 juta siklus) adalah Direct2D draw + present, sama di semua tema; yang bisa menekannya hanya setting Frame rate. Ukur dengan `tools\measure_cpu.ps1`.
- RAM saat jalan sekitar 85–105 MB working set (Glass lebih tinggi), jauh dari target 15–30 MB. Cari penyebabnya.
- Clipboard history: jumlah item dan lama simpan bisa diatur.
- Screentime.
- Island menyingkir saat didekati mouse (supaya tab browser tidak tertutup), dan baru terbuka kalau mouse didorong mentok ke tepi atas layar. Seperti di Poni.
- Downloads: Downloader sudah ada (lihat di atas). Tinggal dicoba dan dirapikan.
- Meeting notes versi sendiri.

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
- Pesan commit dalam bahasa Inggris, gaya repo: apa yang hilang atau berubah, apa yang tetap jalan untuk file settings atau data lama, lalu baris `Cost:` (ukuran poni.exe sebelum → sesudah, dan apa yang berubah saat diam).
- Sebelum commit: `cargo build -p poni --release` tanpa warning baru (sekarang ada 10 warning lama), lalu `cargo test --workspace --release`.
- `target\release\poni.exe` tidak bisa ditimpa saat app jalan. Tutup Hihu dulu sebelum build, lalu jalankan lagi.
- File di repo memakai CRLF. Skrip edit harus mempertahankan CRLF, atau keluarkan file dari working copy lalu `git checkout` lagi setelah commit.

