# Audit Hihu Island — lembar kerja perbaikan

Untuk coding agent yang melanjutkan: dokumen ini adalah hasil audit statis atas repo ini
(HEAD `c8643c3`, 24 Nov 2026). Isinya 20 temuan, urutan kerja yang disarankan, langkah
reproduksi, dan kriteria "selesai" per temuan. Dokumen ini **tidak** mengubah kode apa pun.

Sebelum mulai: `git rev-parse --short HEAD`. Kalau bukan `c8643c3`, nomor baris di bawah
mungkin bergeser — cari berdasarkan nama fungsi, bukan nomor baris.

Laporan lengkap (bukti dan pembahasan tiap temuan) ada di
`D:\Data Z\apk vibecoding\hihu-audit-report.md`; peta file yang diaudit di
`D:\Data Z\apk vibecoding\oyen-aja-audit-map.md`. Keduanya di luar repo dan tidak perlu
dikirim balik.

---

## 0. Status perbaikan (diperbarui 25 Nov 2026)

18 dari 20 temuan sudah diperbaiki dan ada di `main`. Dua sengaja tidak diubah.

Keterangan: **selesai** = sudah di `main` dan di-push · **branch** = selesai di `audit-rest`,
belum merge · **dilewati** = diputuskan tidak diubah · **⚠** = baru diuji lewat build/unit test,
belum dicoba dengan kejadian nyata.

Diverifikasi ulang oleh auditor (25 Nov 2026): hashes cocok dengan `git log`, tiap perbaikan
ditemukan di kode (`Due::anchor_day`, `Server::busy`, `Msg::Cmd(cmd, app_id)`, `names::place`,
`Msg::Key`, `is_blip`, `READY_WAIT`, `Mutex<Option<File>>`, `screen_writable`, `Listener::pictures`,
`KillTimer(TIMER_WAKE)`), dan `cargo test --workspace --release` → 253 + 377 + 21 lulus, 0 gagal,
exit 0.

| # | Status | Commit | Catatan |
|---|---|---|---|
| A1 | selesai | `cf7bcc8` | `Due.anchor`; 31 Jan → 28 Feb → 31 Mar → 30 Apr → 31 Mei; tahunan 29 Feb kembali 29 di tahun kabisat |
| A2 | selesai | `cf7bcc8` | `Event::on` memakai aturan yang sama dengan task (helper `in_month`) |
| A3 | selesai | `6b08a33` | `busy()` membersihkan transfer basi; dipanggil dari `TIMER_MINUTE` yang sudah ada |
| A4 | selesai | `4b7955f` | `Banners::load` memeriksa `LoadOutcome`; file tak terbaca tidak ditimpa/dihapus |
| A5 | selesai | `0653e8e` | flag `screen_writable` seperti `settings_writable` |
| A6 | dilewati | — | Pencarian jendela sekarang terbukti bekerja di PC pemakai; mengubahnya berisiko men-minimize jendela explorer yang salah |
| A7 | selesai | `e2941e0` | `names::place()` mengklaim nama dengan `create_new` sebelum rename; tes `placing_never_replaces_a_file` · ⚠ |
| A8 | selesai | `9159ecb` | id baru = `max(next_id, max id + 1)`; tes `ids_stay_apart_when_next_id_fell_behind` |
| B1 | selesai | `ddd5718` | drain mencatat `MicChanged`, rebind dijalankan setelahnya · ⚠ |
| B2 | selesai | `c1427f0` | `Msg::Key` ke thread audio; tombol saat rebind diantrekan; fallback ke lookup lama. Dites langsung 0,06 → 0,08 → 0,06 |
| B3 | selesai | `4cef32a` | Dokumentasi saja: `ARCHITECTURE.md` menyebut Step aside tidak ditawarkan lagi; kodenya dibiarkan |
| B4 | selesai | `6061459` | listener hanya mengirim `Seen`; foto/logo diambil lewat `Listener::pictures` setelah lolos filter Who/Message · ⚠ |
| C1 | selesai | `e29baca` | `Msg::Cmd` membawa `app_id` yang tampil; seek clamp & `pending_seek` hanya untuk sesi yang diikuti · ⚠ |
| C2 | selesai | `a6170c8` (+`d030b7f`) | Beda dari saran: `is_blip` = tanpa artis && (< 20 dtk ‖ judul nama situs), berlaku untuk semua app — lagu pendek ber-artis sekarang tampil |
| D1 | selesai | `1f11549` | `KillTimer(TIMER_WAKE)` saat tidak ada jadwal |
| E1 | selesai | `83d514f` | `File` dipegang di `Mutex`, dibuka sekali di `init` |
| E2 | selesai | `83d514f` | Komentar menyebut `%LOCALAPPDATA%\Hihu`; doc-comment tertukar dipindah ke `apple_music_token_path` |
| E3 | dilewati | — | `%APPDATA%` kosong sangat jarang di Windows normal |
| N1 | selesai | `b802925` | `READY_WAIT` 15 menit; habis waktu = "Gemini sibuk", rekaman tetap disimpan |
| N2 | selesai | `acb79f3` | `run()` membaca kunci sekali; `upload`/`ready`/`hand_over` menerima `&str` |

**Belum dicoba dengan kejadian nyata** (hanya build + unit test): C1 (blip WhatsApp/HP saat lagu
main), B1 (colok headset), B4 (notifikasi dari app yang disembunyikan), A7 (kiriman HP dengan
nama yang sama). Kalau salah satu dari itu muncul lagi di pemakaian, itu petunjuk pertama bahwa
perbaikannya kurang.

**Perubahan perilaku yang perlu diketahui:** C2 — track di bawah 20 detik yang punya artis
sekarang tampil (dulu tidak pernah). A1/A2 — tanggal 29/30/31 yang jatuh di bulan pendek
sekarang memakai hari terakhir bulan itu lalu kembali ke tanggal aslinya; event bulanan ikut
aturan yang sama (sebelumnya event tanggal 31 tidak muncul sama sekali di bulan pendek).

**Sisa yang belum dikerjakan dari temuan ini:** B3 hanya memperbaiki dokumentasi, jadi ± 200
baris jalur "Step aside" di `fullscreen.rs` masih ada dan tetap tidak pernah berjalan; dan
`audio.rs` masih menyimpan fungsi lookup lama sebagai fallback B2 (dipakai bila thread audio
tidak jalan) — keduanya sengaja dibiarkan, bukan terlewat.

---

## 1. Aturan repo yang wajib diikuti

Baca `CLAUDE.md` dan `AGENTS.md`; yang paling sering dilanggar saat memperbaiki bug:

- **Satu perubahan per branch**, nama branch `<topik>` atau `remove-<fitur>`; setelah dites,
  merge `--ff-only` ke `main`. Push hanya setelah ditanya.
- **Jangan tambah thread, timer, atau polling yang jalan saat island diam.** Fitur yang mati
  tidak boleh jalan sama sekali. Kalau butuh pengulangan, beri alasan di komentar dan
  hentikan begitu tidak perlu.
- **File memakai CRLF.** Skrip yang menulis file harus mempertahankan CRLF.
- **`cargo build -p poni --release`** untuk setiap perubahan; tutup Hihu dulu. Tutup dengan
  `WM_CLOSE` ke jendela kelas `Hihu.Island` (sama dengan Quit), **jangan** `Stop-Process` —
  kalau dimatikan paksa, banner notifikasi Windows dan bar volume/brightness tidak
  dikembalikan.
- **Pesan commit dalam bahasa Inggris**, gaya repo: apa yang berubah, apa yang tetap jalan
  untuk file settings/data lama, lalu baris `Cost:` (ukuran `hihu.exe` sebelum → sesudah,
  dan apa yang berjalan saat diam).
- **Jangan rusak data lama**: `settings.json` versi 1..7 dan `data.json` lama harus tetap
  terbaca. Setiap field baru butuh `#[serde(default)]`.
- Verifikasi akhir: `cargo test --workspace --release` (10 warning lama di `poni` bukan
  regresi) dan `cargo check -p poni --release`.

---

## 2. Urutan kerja

| Urutan | Temuan | Kenapa didahulukan |
|---|---|---|
| 1 | **A1** | Satu-satunya bug yang mengubah data pengguna diam-diam; sudah ada tes pembuktinya |
| 2 | **A3** | State "nyangkut" yang tidak bisa dipulihkan pengguna tanpa restart |
| 3 | **C1** | Tombol media salah sasaran; bisa melepas lagu dari island |
| 4 | **N1** | Job bisa menggantung berjam-jam tanpa jalan batal |
| 5 | **A4, A5, B1** | Kehilangan data pemulihan / perubahan yang tidak tercatat |
| 6 | sisanya | Polish, aman dikerjakan kapan saja |

Kerjakan berurutan, satu commit per temuan, dengan tes/skenario reproduksi dijalankan
sebelum dan sesudah.

---

## 3. Temuan prioritas

### A1 — Task berulang bulanan kehilangan tanggal jangkar (sedang, terbukti)

**Di mana:** `crates/poni-core/src/agenda.rs:168-184` (`Due::next`), dipakai
`crates/poni-core/src/todo.rs:68` (`TaskList::toggle`) dan `agenda.rs:149-163` (`Due::falls_on`).

**Mekanisme:** `Due::next()` menghitung bulan berikutnya dari `self.day`, padahal `self.day`
sudah dijepit ke akhir bulan pada langkah sebelumnya. Sekali terjepit (31 Jan → 28 Feb),
tanggal aslinya hilang selamanya.

**Bukti (tes probe, sudah dihapus):**

```
Due { 2026-01-31, Every::Months(1) }
NEXT:      2026-01-31 2026-02-28 2026-03-28 2026-04-28 2026-05-28
FALLS_ON:  02-28=true 03-28=true 03-31=false 04-28=true
```

**Reproduksi di app:** Tasks → pill tanggal → pilih 31 Januari → repeat `Every month` →
centang task. Task pindah ke 28 Feb (wajar), lalu **28 Mar** (salah; seharusnya 31 Mar).
Kalender juga menandai 28 Maret.

**Kriteria selesai:**

- `Due { 2026-01-31, Months(1) }` → 2026-02-28, **2026-03-31**, 2026-04-30, 2026-05-31.
- `falls_on(2026, 3, 31) == true` dan `falls_on(2026, 3, 28) == false`.
- `data.json` lama (tanpa field baru) tetap terbaca dan menjadi jangkar = `day` saat itu.
- `Every::Years` mengikuti aturan yang sama (29 Feb → 28 Feb tahun berikutnya, lalu kembali 29
  pada tahun kabisat berikutnya).

**Arah perbaikan:** tambah `#[serde(default)] anchor_day: u8` pada `Due` (`0` = pakai `day`),
hitung `next()` dari `anchor_day` lalu jepit ke akhir bulan: `day = anchor.clamp(1, days_in_month(y, m))`.
Tulis ulang `falls_on` untuk memakai jalur yang sama. Jangan menulis `anchor_day` ke JSON bila
sama dengan `day` (`skip_serializing_if`) supaya file lama tidak berubah bentuk.

**Tes permanen — tempel sebagai `crates/poni-core/tests/audit_due.rs`, lalu hapus `#[ignore]`
setelah A1 diperbaiki:**

```rust
use poni_core::agenda::{Due, Every, Event};

#[test]
#[ignore = "reproduksi A1: hapus baris ini setelah Due::next memakai hari jangkar"]
fn monthly_task_keeps_its_day_of_month() {
    let mut d = Due { year: 2026, month: 1, day: 31, time: None, remind: None, every: Every::Months(1) };
    let mut days = Vec::new();
    for _ in 0..4 {
        days.push((d.year, d.month, d.day));
        d = d.next().unwrap();
    }
    assert_eq!(days, vec![(2026, 1, 31), (2026, 2, 28), (2026, 3, 31), (2026, 4, 30)]);

    let t = Due { year: 2026, month: 1, day: 31, ..Due::on(2026, 1, 31) };
    let monthly = Due { every: Every::Months(1), ..t };
    assert!(monthly.falls_on(2026, 3, 31));
    assert!(!monthly.falls_on(2026, 3, 28));
}

#[test]
#[ignore = "A2: keputusan desain, lihat laporan"]
fn event_and_task_agree_on_short_months() {
    let e = Event { id: 1, title: "x".into(), year: 2026, month: 1, day: 31, every: Every::Months(1), time: None, remind: None };
    // Hari ini event tidak jatuh di 28 Feb, task jatuh; pilih satu aturan lalu samakan.
    assert_eq!(e.on(2026, 2, 28), false);
}
```

---

### A3 — Transfer HP yang tak pernah mulai membuat server "busy" selamanya

**Di mana:** `crates/poni-core/src/phonedrop/server.rs:414` (satu-satunya pemanggil
`drop_stale_transfers`), `:479-487` (pembersih 90 dtk), `:210-213` (`busy`),
`crates/poni/src/app.rs:4730` (panel menutup bila `!busy() && idle >= PHONE_IDLE_S`).

**Mekanisme:** `drop_stale_transfers` hanya dipanggil dari `serve_ask`. Transfer yang sudah
diterima tapi tidak pernah mengirim `PUT` pertama (`writing == false`) tidak pernah dihapus,
jadi `busy()` selalu `true`, panel HP tidak pernah auto-close, dan server LAN tetap terbuka.

**Reproduksi:** Shelf → Phone → pindai QR → terima permintaan kirim dari HP → matikan Wi-Fi HP
sebelum file pertama terkirim → tunggu > 90 dtk. Panel tetap "menunggu", dan HP berikutnya
mendapat "busy" sekali.

**Kriteria selesai:** setelah `TRANSFER_STALE` tanpa IO, transfer dibuang tepat satu kali
(`Event::Failed` dikirim sekali), `busy()` kembali `false`, panel tertutup pada tick menit
berikutnya.

**Arah perbaikan:** tambah `Server::sweep(&self)` yang memanggil `drop_stale_transfers`
(atau jalankan pembersihan dari `busy()`), lalu panggil `sweep` dari `TIMER_MINUTE` di
`app.rs` — timer itu sudah ada, jadi tidak ada thread/timer baru.

---

### C1 — Tombol media menembak sesi yang salah saat "blip"

**Di mana:** `crates/poni/src/app.rs:2188-2205` (blip ditahan, event diabaikan),
`app.rs:4337-4353` (`Command::Media` + flip optimistis),
`crates/poni/src/services/media.rs:659-660` (`chosen` = sesi "current" Windows),
`media.rs:950-951` (perintah memakai `self.chosen`), `crates/poni/src/state.rs:66-68`
(`holds_over` mensyaratkan `self.is_playing()`).

**Mekanisme:** worker memilih sesi menurut Windows; UI menahan lagu sendiri. Saat WhatsApp Web
memutar ping 2–3 dtk, `chosen` = sesi situs, tetapi island tetap menampilkan lagu. Tombol
mengirim perintah ke `chosen` (situs). Flip optimistis membuat lagu `playing = false`,
sehingga `holds_over` bernilai `false` pada publish berikutnya → island melepas lagu.

**Reproduksi:** putar lagu ber-lirik; kirim pesan WhatsApp Web (browser) sehingga ping berbunyi;
dalam ~2 dtk tekan pause di island. Lagu hilang, kartu berubah jadi `web.whatsapp.com`.

**Kriteria selesai:** selama blip, play/pause mengubah lagu (bukan situs) dan kartu tidak
pernah berubah menjadi blip; setelah blip selesai, state island sama dengan state player.

**Arah perbaikan:** pindahkan aturan blip ke worker (`pick_session`/`refresh`: jangan pilih
sesi `is_blip` saat ada track non-blip yang sedang main), atau kirim `app_id` yang sedang
ditampilkan bersama `Command::Media` dan pilih sesi berdasarkan itu.

---

### N1 — Menunggu Gemini "memproses" tidak punya batas waktu nyata

**Di mana:** `crates/poni/src/services/notes.rs:574-592` (`ready`), konstanta `READY_POLL` 2 dtk /
`READY_TRIES` 90 (`:55-57`), `call()` dengan `TRIES = 3` dan `CALL_TIMEOUT` 30 dtk (`:50`, `:59`).

**Mekanisme:** loop dibatasi jumlah percobaan, bukan waktu; tiap percobaan menunggu 2 dtk dan
satu GET yang bisa 30 dtk dan diulang 3× → batas atas ± 2,5 jam.

**Kriteria selesai:** setelah tenggat (usul: 15 menit) tanpa status `ACTIVE`, job berakhir
dengan `Problem::Busy`, rekaman tetap disimpan untuk "Try again", dan UI berhenti di status
"menulis catatan".

**Arah perbaikan:** ganti `for i in 0..READY_TRIES` dengan tenggat `Instant::now() + Duration::from_secs(15 * 60)`.

---

### A4, A5, B1 — kegagalan yang tidak tercatat

- **A4** `crates/poni/src/services/banners.rs:34` — `Banners::load` membuang `LoadOutcome`.
  Kalau `banners.json` rusak, `was` kosong dan nilai `ShowBanner` asli tidak pernah dikembalikan,
  lalu `save()` menghapus filenya. **Selesai bila:** file yang rusak/tak terbaca tidak dihapus,
  ada peringatan di log, dan aplikasi tetap jalan. **Arah:** periksa outcome; untuk `Unreadable`
  jangan simpan/hapus.
- **A5** `crates/poni/src/app.rs:3016` (baca) dan `:3150-3160` (simpan) — `screentime.json`
  ditimpa walau tak terbaca. **Selesai bila:** flag `screentime_writable` seperti
  `settings_writable`, dan file tak terbaca tidak ditimpa.
- **B1** `crates/poni/src/services/audio.rs:318-331` — drain antrean membuang `Msg::MicChanged`.
  **Selesai bila:** perubahan default output + input bersamaan tetap me-rebind mic.
  **Arah:** kuras sambil mencatat flag, jalankan rebind setelahnya.

---

## 4. Temuan polish (aman dikerjakan kapan saja)

| # | Lokasi | Satu baris |
|---|---|---|
| A2 | `agenda.rs:279` vs `:149-163` | Event dan task berbeda untuk tanggal 31; satukan aturan (lihat tes A2 di §3) |
| A6 | `services/osd.rs:19-41` | `host()` mengambil jendela terakhir yang cocok; `break` setelah ketemu |
| A7 | `phonedrop/names.rs:73` | Cek-lalu-`fs::rename` bisa menimpa (rename Windows menimpa); klaim nama dulu |
| A8 | `agenda.rs:296`, `todo.rs:36` | `next_id` 0 pada data lama bisa membuat id bertabrakan; naikkan ke `max(id)+1` |
| B2 | `services/audio.rs:33-59` | `volume_key` membuat objek COM di UI thread tiap tekan; kirim perintah ke worker |
| B3 | `settings.rs:410` + `fullscreen.rs:652-760` | Jalur "Step aside" dipaksa mati; ~200 baris tak pernah jalan, `ARCHITECTURE.md` masih menyebutnya hidup |
| B4 | `notifications.rs:246-259` | Foto pengirim dibaca untuk app yang tidak ditampilkan; minta setelah filter "Who" |
| C2 | `state.rs:51-56` | Track < 20 dtk tak pernah tampil; batasi ke sesi browser tanpa artis |
| D1 | `app.rs:4041-4046` | `TIMER_WAKE` lama tidak dimatikan saat tak ada jadwal; `KillTimer` saat `next` kosong |
| E1 | `plog.rs:24-39` | Tiap baris log = buka/tulis/tutup file, sering di UI thread; pegang `File` terbuka |
| E2 | `plog.rs:1`, `secret.rs:3`, `paths.rs:48` | Komentar menunjuk `%LOCALAPPDATA%\Poni`; doc-comment `screentime_path` tertukar |
| E3 | `paths.rs:17-22` | `%APPDATA%` kosong → data bisa mendarat di sebelah exe; pakai `SHGetKnownFolderPath` |
| N2 | `notes.rs:496-502` | Kunci dibaca ulang hanya untuk menghapus unggahan; pakai kunci yang sama |

---

## 5. Cara memverifikasi

```powershell
# baseline sebelum mengubah apa pun
cargo test --workspace --release
cargo check -p poni --release

# setelah perbaikan: tutup Hihu dulu (WM_CLOSE ke kelas Hihu.Island), lalu
cargo build -p poni --release
cargo test --workspace --release
```

Kondisi saat audit: `cargo test -p poni-core --release` → 374 lulus, 0 gagal;
`cargo check -p poni --release` → sukses; `cargo build -p poni --release` berhenti hanya
karena `target\release\hihu.exe` sedang dipakai app yang berjalan.

Untuk A1/A2: jalankan tes di §3 (hapus `#[ignore]` setelah diperbaiki). Untuk A3/C1/N1
perbaikannya tidak punya tes otomatis — pakai langkah reproduksi di atas, sebelum dan sesudah.

---

## 6. Cakupan dan batasannya

**Diaudit mendalam** (± 5.000 dari 62.568 baris): `poni-core/{agenda,todo,store,phonedrop/*,settings}`,
`services/{banners,clipboard,keys,osd,notifications,wpndb,audio,addons,secret,media,notes,fullscreen(aturan)}`,
`gfx.rs` (device loss), `glass.rs` (gating capture), `paths.rs`, `plog.rs`, `state.rs`
(gating + `MediaSession`), `pages/mod.rs`, dan jalur `app.rs` (timer, `schedule_wake`,
`free_to_collapse`, `Command::Media`, `AppEvent::Media`).

**Belum diaudit — jangan dianggap bersih:** `settings_window.rs` (2.581 baris), sisa `pages/*`
(`today`, `tasks`, `shelf`, `media`, `focus`, `clipboard`, `screentime`, `notes`, `script`,
`prayer`, `notifications`), `services/imaging.rs`, `services/bluetooth.rs`,
`services/applemusic*`, `services/http.rs`, `poni-core/prayer/*`, `poni-core/pairdrop/*`
(WebRTC), `crates/poni/src/mac/**` (tidak di-build), `selftest.rs`.

**Batas metode:** audit statis (baca kode + tes), bukan fuzzing atau pengujian runtime.
Klaim performa (B2, E1) belum diukur dengan pengukur CPU.
