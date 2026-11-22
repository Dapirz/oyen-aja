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
- Inti island: hover membesar/mengecil, click-through, tampilan saat diam, pop-up volume/baterai/Bluetooth, perilaku layar penuh, Settings, tray, start with Windows.
- Music: SMTC, lirik sinkron (LRCLIB), bacaan latin, lirik Apple Music.
- Today: kalender saja (tanpa Google Calendar, Gmail, tagihan).
- Shelf + Phone drop + pairing HP.
- Tasks, Focus, Script, Bacaan Jepang: tetap ada, bisa dimatikan lewat toggle. Semua toggle fitur dikumpulkan di satu tempat (halaman Add-ons), dan fitur yang mati tidak jalan sama sekali (hemat memori).
- Waktu sholat: lokasi otomatis dan pilih manual.
- Tema: menunggu keputusan (ingin mirip tema Poni Bos Digital).
- Notifications: dipertahankan, cari cara supaya jalan di build non-Store.

Dihapus:
- Semua login dan layanan Oyenza: tab Oyenza (uang), Health, IELTS, Meeting notes versi Oyenza, bagian Today yang butuh login.
- AI usage, System, Show over games, Stocks, Caffeine, Record.
- Updater otomatis ke studio.oyenza.com (matikan atau ganti).

Identitas sendiri (branch pertama): nama app, ikon, folder data, mutex, kelas window, Run value dan uninstall key baru, supaya bisa dipasang berdampingan dengan Poni dan Oyenza tanpa bentrok.

Nanti (belum dikerjakan):
- Clipboard history: jumlah item dan lama simpan bisa diatur.
- Screentime.
- Island menyingkir saat didekati mouse (supaya tab browser tidak tertutup), dan baru terbuka kalau mouse didorong mentok ke tepi atas layar. Seperti di Poni.
- Downloads (yt-dlp).
- Meeting notes versi sendiri.

Tidak dibuat: radio.

## Aturan kerja

- Target RAM sedekat mungkin dengan 15–30 MB (Oyenza resmi sekitar 30–60 MB). Jangan tambah WebView atau dependency berat.
- Satu perubahan per branch. Setelah edit selalu `cargo build -p poni --release` (toolchain MSVC).
- Jangan hapus LICENSE, THIRD-PARTY-NOTICES.txt atau credit pembuat asli (MIT).
- Jangan pakai nama, ikon atau aset Poni, ReyLand, Nusa, maupun brand Oyenza/Oyen (lihat README bagian License).
- Tes setiap perubahan: click-through di sekitar notch, animasi hover mulus, kontrol media SMTC.
