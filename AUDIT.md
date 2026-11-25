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

---

# Putaran 2 — audit statis lanjutan (HEAD `426b549`)

Putaran ini menutup bagian yang di putaran 1 tercatat "belum diaudit". Tidak ada kode yang
diubah. Nomor temuan memakai huruf area: **F** kalender/task/pengingat/Hijriah, **G** musik/lirik,
**H** settings/backup, **I** waktu sholat, **J** sisanya.

Kondisi verifikasi sebelum menulis: `cargo test --workspace --release` → **253 + 377 + 21 lulus,
0 gagal, exit 0**. Temuan di bawah **tidak** mengubah angka itu (tidak ada kode yang disentuh).

Keyakinan tiap temuan: **terbukti** = ada tes/langkah yang bisa dijalankan dan hasilnya jelas;
**baca kode** = alurnya ditelusuri di kode, belum dijalankan.

## F — Kalender, task, pengingat, Hijriah

Berkas: `pages/today.rs` (519), `pages/tasks.rs` (823), `pages/mod.rs` (847),
`poni-core/hijri/{mod,astro}.rs`, `poni-core/agenda.rs` (setelah perbaikan A1/A2).

### Temuan prioritas

#### F1 — `cal_editing` tidak pernah direset saat island menutup atau pindah tab (baca kode)

**Lokasi:** `crates/poni/src/pages/today.rs:359` (diisi saat pensil diklik), `:412-423` (dipakai
saat Enter), `crates/poni/src/app.rs:2511-2517` (`collapse()` mereset `cal_month`, `cal_day`,
`task_pick_month` — **bukan** `cal_editing`), `crates/poni/src/state.rs:367`.

**Mekanisme:** menekan pensil pada sebuah event/task mengisi `st.ui.cal_editing = Some((jenis, id))`.
Nilai itu hanya dibersihkan dua tempat: Esc (`today.rs:419`) dan Enter (`:423`) — keduanya hanya
terjangkau selama halaman Today terbuka. `collapse()` dan perpindahan tab tidak menyentuhnya, dan
`PageUiState` hidup selama proses.

**Akibat 1 (perubahan data tanpa disadari):** buka Today → klik pensil pada sebuah event → pindah
ke tab Music (atau tutup island) → buka Today lagi. Baris tambah masih berisi teks event lama
dengan placeholder "Edit, Enter to save". Mengetik judul baru lalu Enter **mengubah event lama
itu**, bukan menambah event baru — dan event lama bisa berada di tanggal yang tidak sedang
ditampilkan, karena `cal_editing` hanya menyimpan `(jenis, id)`.

**Akibat 2 (input hilang tanpa pesan):** kalau event/task itu sudah dihapus sementara
`cal_editing` masih menunjuk ke sana, Enter mengambil state (`take()`), `find` tidak menemukan
apa pun, lalu `return` di `:443` — judul yang baru diketik dibuang tanpa notice.

**Reproduksi (dari baca kode, belum dijalankan):**
1. Today → tambah event "Rapat" besok, tekan Enter.
2. Klik pensil pada "Rapat" (baris tambah berubah jadi "Edit, Enter to save").
3. Pindah ke tab Music, lalu kembali ke Today.
4. Ketik "Kontrol dokter", Enter.
5. Harapan: event baru bertambah. Kenyataan (menurut kode): "Rapat" berubah judulnya, dan tidak
   ada event baru.

**Kriteria selesai:**
- Setelah `collapse()` atau pindah tab, `cal_editing` bernilai `None`, dan baris tambah kembali
  ke placeholder "Add an event"/"Add a task due this day".
- Enter saat target `cal_editing` sudah tidak ada **menambah** event/task baru (atau menampilkan
  pesan bahwa targetnya hilang) — tidak menelan input.

**Arah perbaikan:** reset `cal_editing` di `collapse()` bersama field Today lainnya, dan saat
`st.page` berubah di `pages/mod.rs`; di `today.rs:423`, ketika `find` mengembalikan `None`,
jatuhkan ke jalur "add" alih-alih `return`.

**Keyakinan:** baca kode (alurnya jelas; tidak ada tes yang bisa menangkapnya tanpa menjalankan UI).

### Polish

#### F2 — Tanggal Hijriah dihitung untuk 42 sel tiap frame meski fiturnya mati (baca kode, perf)

**Lokasi:** `crates/poni/src/pages/today.rs:143-146`.

**Mekanisme:** `let hijri = poni_core::hijri::date(cal, day);` dipanggil untuk setiap sel
sebelum diketahui apakah fiturnya dipakai; hasilnya baru berguna bila `cal_hijri` atau
`ramadan_mode` nyala. `date()` memanggil `astro::lunation_at` (rumus Meeus) lalu mengunci
`Mutex` cache `STARTS` (`hijri/mod.rs:305-325`), jadi 42 panggilan per frame selama halaman
Today terbuka.

**Dampak:** puluhan mikrodetik per frame — kecil, tetapi melanggar aturan "yang mati tidak
jalan", dan halaman ini digambar ulang setiap kali island beranimasi.

**Arah perbaikan:** hitung hanya bila `st.settings.cal_hijri || st.settings.ramadan_mode`
(dan `fasting` memakai hasil yang sama), atau cache `(cal, day) → HijriDate` sekali per hari.

#### F3 — "Tomorrow" dan "Next Mon" tidak tunduk pada batas `MAX_DUE_DAYS` (baca kode, info)

**Lokasi:** `crates/poni/src/pages/tasks.rs:601` (sel kalender dibatasi `day <= today + MAX_DUE_DAYS`)
vs `:636-642` (quick pick tanpa batas).

**Mekanisme:** pada hari terakhir jendela pemilih, "Tomorrow" menghasilkan `task_due =
MAX_DUE_DAYS + 1`, yang tidak bisa dipilih lewat kalender.

**Dampak:** satu task bisa jatuh sehari di luar jendela; tidak berbahaya, hanya tidak konsisten.

**Arah perbaikan:** clamp quick pick seperti sel kalender, atau nonaktifkan bila melewati batas.

### Diperiksa dan bersih (F)

- `hijri/mod.rs` — `month_index` benar untuk lunasi acuan (315 = Muharram 1447); `date()` mundur
  satu lunasi bila bulannya belum mulai; tabel isbat Kemenag dan KHGT menang atas hitungan; cache
  `STARTS` per (kalender, lunasi) dan tidak tumbuh tanpa batas yang wajar; `label()`/`MONTHS`
  ter-clamp; `is_ramadan()` konsisten dengan `date()`.
- `today.rs` — nama bulan dihitung dengan `div_euclid`/`rem_euclid` (tahun negatif pun benar);
  kisi 42 sel mulai Senin; panel hari memakai `Due::falls_on` (aturan A1) dan hanya mencentang
  task yang jatuh pada tanggal tenggat sekarang (`:331`), jadi task berulang tidak bisa
  dicentang dari hari lain.
- `tasks.rs` — `task_due` disimpan sebagai offset hari dari "hari ini" tetapi selalu dihitung
  ulang saat submit (`:308`), jadi pergantian hari tidak menggeser tanggal; `restore()` menolak
  id duplikat dan berhenti di `MAX_TASKS`; `make_room()` hanya berjalan ketika daftar benar-benar
  penuh (`:128-129`), bukan setiap kali halaman dibuka.
- Pengingat — `Due::remind_at`/`Event::remind` dihitung ulang setiap kali, dan satu-satunya
  timer adalah `TIMER_AGENDA` sekali-jalan (putaran 1); tidak ada timer baru di area ini.

### Belum tercakup di area F

- `poni-core/hijri/astro.rs` (matematika Meeus, 385 baris): tidak dibaca baris per baris, hanya
  lewat pemakaiannya di `hijri/mod.rs`.
- `pages/mod.rs` bagian animasi geser tab dan pemilihan tab: dibaca sebagian (baris tab sudah
  diaudit di putaran 1 lewat temuan D1/§4); animasinya tidak.

## G — Musik dan lirik

Berkas: `pages/media.rs` (1352), `services/lyrics.rs` (1079), `poni-core/lrclib.rs` (816),
`poni-core/lrc.rs` (223), `services/applemusic.rs` (235), `services/romanize.rs` (403, hanya
pemakaiannya).

### Temuan prioritas

Tidak ada temuan prioritas di area ini: alur lirik/lirik-sinkron/cache terbaca benar, termasuk
fitur "Wrong lyrics?" yang baru.

### Polish

#### G1 — "Wrong lyrics?" bisa menawarkan salinan yang sama-sama rusak (baca kode)

**Lokasi:** `crates/poni-core/src/lrclib.rs:150-165` (`ranked` tidak menyaring `spans_song`),
`:171-181` (`next_copy`), `crates/poni/src/services/lyrics.rs:452-484` (`other_copy`),
`:857-870` (`lyrics_for` menurunkan sinkron → unsynced).

**Mekanisme:** `ranked()` hanya menyaring skor kemiripan, bukan apakah lirik sinkron menjangkau
40 % lagu (`spans_song`). Jadi salinan yang "dijejal" (bug yang diperbaiki di `14128c4b`) tetap
ikut ditawarkan oleh "Wrong lyrics?". Setelah dipilih, `lyrics_for` menurunkannya menjadi lirik
**tanpa sinkron** — aman (tidak balapan), tetapi bagi pengguna hasilnya tampak sama saja: lirik
tetap tidak mengikuti lagu, hanya berhenti menyorot baris.

**Dampak:** fitur terasa "tidak bekerja" pada lagu yang semua salinannya jejal; tidak ada
kerusakan data.

**Arah perbaikan:** urutkan `ranked()` dengan salinan yang `spans_song` lebih dulu (atau tandai
"salinan ini pun tidak tersinkron" di label tombol) supaya klik berikutnya memberi hasil yang
berbeda secara kasatmata.

#### G2 — Tiap klik "Wrong lyrics?" mengulang seluruh pencarian jaringan (baca kode, info)

**Lokasi:** `crates/poni/src/services/lyrics.rs:462-471`.

**Mekanisme:** `other_copy` membangun ulang daftar `searches` (query utama + `title_guesses` untuk
browser) dan memanggil `search_all` untuk setiap varian pada **setiap** klik; hanya salinan yang
terpilih yang di-cache, hasil pencariannya tidak.

**Dampak:** beberapa permintaan ke LRCLIB per klik, berulang kalau tombolnya diklik berkali-kali.
Dibatasi oleh klik pengguna (bukan loop otomatis), jadi hanya catatan; LRCLIB layanan publik
gratis.

**Arah perbaikan (kalau mau):** simpan id hasil pencarian terakhir di `CacheEntry` dan pakai ulang
selama lagunya sama.

#### G3 — Dedupe `ranked()` melewatkan id 0 (baca kode, info)

**Lokasi:** `crates/poni-core/src/lrclib.rs:164`.

**Mekanisme:** filter `r.id == 0 || (!seen.contains(&r.id) && { seen.push(r.id); true })` membuat
record ber-id 0 selalu lolos dan tidak pernah masuk `seen`, jadi beberapa record tanpa id bisa
muncul sebagai salinan terpisah. LRCLIB selalu memberi id, dan fixture tes memakai id ≠ 0, jadi
tidak pernah terjadi pada data nyata.

**Arah perbaikan:** perlakukan id 0 seperti id lain (atau saring record tanpa id sebelum dedupe).

### Diperiksa dan bersih (G)

- `lrc.rs` — parser menangani `[mm:ss]`, `.xx`, `.xxx`, `:xx`, beberapa stempel per baris,
  `[offset:±ms]` (diterapkan setelah parsing, jadi boleh di mana saja), metadata tag diabaikan,
  enhanced-LRC `<mm:ss.xx>` dibuang; `partition_point` di `line_index_at` aman karena baris
  diurutkan stabil (`sort_by`); `strip_word_timings` tidak bisa panik pada `<` yang tak tertutup;
  tidak ada `unwrap` pada input.
- `lrclib.rs` — `spans_song` hanya berlaku untuk lagu ≥ 60 dtk (lagu pendek/panjang tak diketahui
  lolos), `timing_fits` mentoleransi 6 dtk, `cache_key` FNV atas artis+judul+durasi, `next_copy`
  berputar balik setelah semua salinan dicoba.
- `lyrics.rs` — cache menyimpan `tried` sehingga salinan yang ditolak tidak ditawarkan lagi pada
  pemuatan berikutnya (`:416`); salinan yang ditemukan lewat `/api/get` tetap divalidasi
  `lyrics_for` sebelum ditampilkan; retry jaringan dibatasi `MAX_RETRIES` dengan jeda 30 dtk/120 dtk
  (`:320-327`), bukan loop tanpa ujung; prunning cache (`prune_victims`) menghapus yang tertua
  lebih dulu dan tidak pernah menghapus saat jumlah file masih di bawah batas.
- `pages/media.rs` — `lyric_browse` (scroll lirik) dikembalikan ke baris yang dinyanyikan setelah
  `BROWSE_HOLD_S` dan di-clamp ke jumlah baris; indeks baris untuk klik/sepak `lines[row.r - off]`
  dijaga `is_line = row.r >= off`, jadi tidak bisa keluar batas; klik baris hanya aktif saat
  `fade > 0.4` sehingga tidak kena baris yang pudar di tepi.
- `applemusic.rs` — tidak ada loop/retry/sleep di dalamnya; kegagalan storefront jatuh ke default.

## H — Settings dan backup

Berkas: `poni-core/backup.rs` (63, dibaca penuh), `app.rs:2835-2887` (Export/Import),
`app.rs:1976-2045` (`settings_changed`), `settings_window.rs` (2.713, bagian Export/Import dan
baris yang dipakai), `settings_host.rs` (441), `pages/settings.rs` (395, pemakaiannya).

### Temuan prioritas

#### H1 — Export menimpa backup hari yang sama tanpa memberi tahu (baca kode, rendah–sedang)

**Lokasi:** `crates/poni/src/app.rs:2836-2855`, `crates/poni-core/src/backup.rs:39-41`
(`file_name` hanya tahun-bulan-tanggal).

**Mekanisme:** nama berkasnya `Hihu backup YYYY-MM-DD.json`, dan penyimpanan memakai
`std::fs::write(&path, text)` yang **menimpa** berkas yang sudah ada. Ekspor kedua pada hari yang
sama menghapus ekspor pertama tanpa peringatan; catatan di Settings tetap berbunyi
"Saved as Hihu backup …" seolah-olah itu berkas baru.

**Skenario nyata:** pengguna mengekspor pagi hari, lalu salah Import (data kembali ke versi
backup), lalu mengekspor lagi untuk "menyelamatkan" keadaan sekarang — backup pagi itu hilang,
padahal justru itu yang ingin disimpan.

**Kriteria selesai:** dua ekspor pada hari yang sama menghasilkan dua berkas (mis.
`Hihu backup 2026-11-25.json` dan `Hihu backup 2026-11-25 (2).json`), tanpa menimpa; catatan di
Settings menyebut nama berkas yang benar-benar ditulis.

**Arah perbaikan:** pakai pola pengklaiman nama yang sudah ada di repo — `phonedrop::names::place`
(dipakai A7) mengklaim nama dengan `create_new` dan menomori berikutnya; `store::set_aside` juga
punya penomoran `<stem>.corrupt-<unix>[-n].json`. Alternatif lebih sederhana: tambahkan jam-menit
ke `file_name`.

**Keyakinan:** baca kode (alurnya tunggal dan jelas; tidak ada tes yang menutupinya karena
menyentuh filesystem + UI).

#### H2 — Import memilih berkas "terbaru" menurut nama, bukan waktu (baca kode, rendah)

**Lokasi:** `crates/poni/src/app.rs:2860-2863`.

**Mekanisme:** `read_dir(...).filter(extension == "json").max()` memilih `PathBuf` terbesar secara
leksikografis, bukan berkas yang paling baru ditulis. Selama semua berkas bernama
`Hihu backup YYYY-MM-DD.json` hasilnya kebetulan benar (tanggal terurut sebagai teks), tetapi
berkas JSON lain di folder yang sama bisa menang — mis. `notes.json` (`n` > `H`) — sehingga Import
gagal dengan "That file isn't a Hihu backup" padahal backup sah ada di folder itu. Ekstensi juga
dibandingkan peka huruf besar (`Backup.JSON` hasil rename dilewati).

**Dampak:** pesan error menyesatkan; pengguna menyimpulkan backup-nya rusak padahal tidak.

**Kriteria selesai:** Import memakai berkas dengan `modified` terbaru **dan** melewati berkas yang
tidak terbaca/tidak sah, mencoba yang berikutnya sampai ada yang cocok; ekstensi dibandingkan
tanpa peduli huruf besar.

**Arah perbaikan:** `read_dir` → `Vec<(SystemTime, PathBuf)>` diurutkan menurun, lalu coba
`backup::read` satu per satu; laporkan hanya bila tidak ada satu pun yang sah.

### Polish

#### H3 — `backup::make` bisa menghasilkan berkas kosong yang dilaporkan "berhasil" (info)

**Lokasi:** `crates/poni-core/src/backup.rs:26` (`serde_json::to_string_pretty(&b).unwrap_or_default()`).

**Mekanisme:** bila serialisasi gagal (tidak terjadi untuk `Settings`/`Store` saat ini),
`make` mengembalikan string kosong, dan `backup_export` menulisnya sebagai berkas 0 byte lalu
menampilkan "Saved as …".

**Arah perbaikan:** kembalikan `Result<String, …>` dan tampilkan kegagalan.

#### H4 — Import tidak menata ulang halaman yang sedang terbuka (baca kode, info)

**Lokasi:** `crates/poni/src/app.rs:2867-2879`; pembanding perilaku di `app.rs:2511-2526`
(`collapse` menata ulang `st.page`).

**Mekanisme:** Import mengganti `settings` lalu memanggil `settings_changed`, yang menangani
add-on, tema, sholat, dan lirik — tetapi tidak menyentuh `st.page`. Kalau backup berisi
Tasks/Clipboard/Screen time **mati** sedangkan island sedang membuka halaman itu, halamannya tetap
terbuka tanpa tab sampai island ditutup (menutup island memanggil `pages::next_open` yang
membereskannya).

**Dampak:** tampilan tidak konsisten sesaat; pulih sendiri. Catat saja.

### Diperiksa dan bersih (H)

- `backup.rs` — `read` menolak JSON yang bukan backup (`kind`), menerapkan `Settings::sanitized()`
  pada hasil baca (jadi nilai hasil edit tangan tidak masuk mentah), dan `Backup` mewajibkan
  `version`/`made`/`settings`/`store` sehingga berkas separuh jadi tidak diterima; ada tes
  round-trip.
- Import tidak merusak kunci DPAPI: token Apple Music, kunci Gemini, dan pasangan HP berada di
  `%LOCALAPPDATA%\Hihu` dan tidak ikut di berkas backup (`backup.rs:1-5`), jadi setelah Import
  kunci lama tetap berlaku.
- `settings_changed` mengurus dampak Import dengan benar: add-on yang berubah status memicu
  unduh/hapus (`s.set_addon`), sholat di-resolve ulang bila tempatnya berubah, Pomodoro memakai
  konfigurasi baru, lirik diminta ulang bila opsi lirik dinyalakan.
- `settings_host.rs` — tidak ada `SendMessage`/loop/sleep/wait yang bisa menggantungkan UI;
  `AttachThreadInput` selalu dipasangkan dengan pelepasannya lewat guard (`:247-260`).
- `settings_window.rs` — Export/Import hanya mengirim `Command`, dan Import butuh dua tekanan
  (`:1028-1035`, diuji di `:2335-2336`).

## I — Waktu sholat

Berkas: `poni-core/prayer/mod.rs` (759), `poni-core/prayer/adhan.rs` (matematika, hanya lewat
pemakaian), `poni-core/prayer/world.rs` (410, pemakaian), `pages/prayer.rs` (682), dan jalur
`app.rs:4218-4300` (`prayer_tick`, `prayer_moment`, `locate`).

### Temuan prioritas

#### I1 — Jadwal sholat dihitung ulang setiap frame (terbukti lewat pengukuran)

**Lokasi:** `crates/poni/src/app.rs:4223-4247` (`prayer_tick`, dipanggil dari `tick()` setiap
frame), `:3547`; inti biayanya di `crates/poni-core/src/prayer/mod.rs:665-709`
(`moments_around`/`prayers_between`).

**Mekanisme:** setiap frame, `prayer_tick` membandingkan `handled_until` dengan `now`; karena
waktu selalu maju, syarat `now <= from` hampir tidak pernah benar, sehingga
`prayers_between(&site, from, now)` dijalankan **tiap frame**. Fungsi itu menyusuri 2–3 hari dan
memanggil `day_times()` untuk tiap hari **dan** hari berikutnya — masing-masing satu perhitungan
posisi matahari penuh (adhan) — lalu memeriksa 5 sholat.

**Bukti (tes sementara, dijalankan lalu dihapus):**

```
PRAYER_SCAN: 34.6 us per panggilan (200 panggilan, 0 hasil), setara 2.1 ms/detik pada 60 fps
```

Satu panggilan `prayers_between(site, t, t+1)` memakan **34,6 µs**. Karena `tick()` ikut setiap
frame, pada island yang sedang beranimasi (musik 60 fps, morph hover) itu **± 2,1 ms CPU per
detik** — sekitar 0,2 % satu core — untuk memeriksa kejadian yang hanya ada 5 kali sehari. Pada
CPU yang lebih lambat dari mesin uji, angkanya bisa 2–4× lipat.

**Dampak:** melanggar aturan "Keep it light": kerja yang tidak berubah dihitung ulang terus-menerus
selama ada animasi, dan hasilnya nyaris selalu kosong (`0 hasil` di 200 panggilan).

**Kriteria selesai:** `prayers_between` tidak dipanggil lebih dari sekali per menit (atau sekali
per sholat). Cara paling sederhana: simpan `next: Option<PrayerMoment>` di `PrayerState` dan
hanya hitung ulang saat `now >= next.at` (atau saat tempat/`handled_until` berubah).

**Arah perbaikan:** dua langkah murah, pilih salah satu:
1. **Gerbang menit** — bandingkan `from / 60 == now / 60` dan lewati sisanya; sholat selalu jatuh
   pada menit bulat (`ceil_minute`/`nearest_minute`), jadi presisi menit cukup.
2. **Cache `DayTimes`** — simpan `(site, day) → DayTimes` di `PrayerState`; `moments_around` lalu
   hanya beberapa pencarian map, dan biaya per frame jatuh ke hitungan nano-detik.

- **Keyakinan:** terbukti (biaya diukur; alur per-frame dibaca di kode).

### Polish

#### I2 — Komentar `prayer::due` menjanjikan perilaku setelah restart yang tidak terjadi (info)

**Lokasi:** `crates/poni-core/src/prayer/mod.rs:727-744` (dokumentasi `due`), `crates/poni/src/app.rs:4232-4234`.

**Mekanisme:** dokumentasi `due` berbunyi "after a sleep or a restart (within 30 minutes) it shows
without a sound". Untuk *sleep* (proses tetap hidup) itu benar: `handled_until` lama, maka
`prayers_between(old, now)` mengembalikan sholat yang lewat dan `due` memberi `Quiet`. Untuk
*restart*, `handled_until` mulai `None` dan langsung diisi `now` (`:4232-4234`), jadi jendelanya
kosong dan sholat yang lewat saat app mati **tidak** muncul — senyap maupun berbunyi.

**Dampak:** perilaku menyimpang dari dokumentasinya sendiri; pengguna yang menutup app 10 menit
sebelum Maghrib dan membukanya 5 menit setelahnya tidak melihat apa pun.

**Arah perbaikan:** saat start, isi `handled_until = now - 30 * 60` (bukan `now`), sehingga
`due` memberi `Quiet` untuk sholat yang lewat dalam 30 menit terakhir, sesuai dokumentasinya.
Konsekuensinya: setelah app dibuka pertama kali, sholat terakhir dalam 30 menit tampil senyap
sekali — pastikan itu memang diinginkan.

#### I3 — `prayers_between` menghitung jadwal hari berikutnya dua kali (info, bagian dari I1)

**Lokasi:** `crates/poni-core/src/prayer/mod.rs:696-707`.

**Mekanisme:** untuk tiap `day` ia memanggil `day_times(day)` dan `day_times(day + 1)`; pada
iterasi berikutnya `day_times(day + 1)` dihitung lagi sebagai `t`. Jadi separuh perhitungan
matahari mubazir.

**Arah perbaikan:** bawa hasil `day_times(day + 1)` ke iterasi berikutnya sebagai `t`.

#### I4 — Pemilih tempat tetap terbuka setelah island ditutup (info, perilaku)

**Lokasi:** `crates/poni/src/pages/prayer.rs:118-131`, `app.rs:2511-2517` (`collapse`).

**Mekanisme:** `collapse()` tidak mereset `st.prayer.picking`, sehingga membuka Prayer lagi
menampilkan pemilih tempat yang tadi terbuka. Tidak berbahaya (tidak seperti F1 yang bisa
mengubah data), tetapi berbeda dari `cal_month`/`cal_day`/`task_pick_month` yang direset.

### Diperiksa dan bersih (I)

- Zona waktu: `Clock::Changes` memakai tabel perubahan dari daftar dunia (DST ditangani), bukan
  offset tetap; `utc_offset_min` hanya untuk tempat Kemenag/Riyadh yang memang tanpa DST.
- `Site::local_day`, `midnight`, `hhmm` konsisten memakai `offset_min(utc)` di titik yang sama.
- `moments_around` menyusun 5 sholat × 3 hari dengan `end` yang benar (Subuh→Terbit,
  Dzuhur→Ashar, Ashar→Maghrib, Maghrib→Isya, Isya→Subuh besok), dan `next_prayer`/`current_prayer`
  memakai `at > utc`/`at <= utc` sehingga satu sholat tidak pernah dihitung dua kali pada detik
  yang sama.
- `countdown` dibatasi `before_min` dan ditutup per `m.at` (`state.rs:771`, `:784-793`), jadi OK
  pada satu countdown tidak mematikan countdown sholat berikutnya.
- `locate()` dijaga `Locate::Asking` dan langsung mengisi `located_at` (`app.rs:4218-4230`),
  sehingga kegagalan lokasi tidak memicu percobaan tiap frame.
- `prayer_moment` memakai tone untuk Subuh (rekaman adzan tidak memuat "ash-shalatu khairum minan
  naum") dan memilih senyap saat island tersembunyi layar penuh.
- Mengganti tempat mengisi `handled_until = now` (`pages/prayer.rs:536`), jadi sholat yang lewat
  untuk tempat lama tidak diputar untuk tempat baru.

## J — Sisanya

Berkas: `services/http.rs` (487 baris produksi), `services/imaging.rs` (732), `services/bluetooth.rs`
(910), `pages/shelf.rs` (1.004), `pages/clipboard.rs` (535), `pages/focus.rs` (433),
`pages/script.rs` (419), `pages/notes.rs` (361), `pages/notifications.rs` (127),
`pages/screentime.rs` (123), `pages/addons.rs` (186), `pages/card.rs` (75).

Metode: pemindaian otomatis seluruh berkas (unwrap/expect/panic/loop/sleep/timeout/`as usize`) lalu
baca bagian yang berisiko. **Tidak ada `unwrap`/`expect`/`panic!` di kode produksi** ke-12 berkas
ini — semuanya di modul tes.

### Temuan prioritas

Tidak ada temuan prioritas di area ini.

### Polish

#### J1 — Batas ukuran badan HTTP berlaku setelah seluruh badan diunduh (baca kode)

**Lokasi:** `crates/poni/src/services/http.rs:30` (`MAX_BODY_BYTES` 8 MB), `:294-324`
(`read_response`/`read_body`).

**Mekanisme:** `read_body` memanggil `content.ReadAsBufferAsync()` — satu buffer berisi **seluruh**
badan — dan baru setelah itu memeriksa `MAX_BODY_BYTES`. Ditambah `HttpClient` WinRT yang secara
bawaan memakai `ResponseContentRead` (badan sudah diunduh penuh saat `SendRequestAsync` selesai),
pemeriksaan `Content-Length` di dalam `read_body` praktis tidak mencegah alokasi. Server yang
besar (atau jahat) bisa membuat app mengalokasikan memori jauh di atas 8 MB sebelum ditolak, dan
pemeriksaan ukuran hanya menyelamatkan tahap parsing.

**Dampak:** bukan hang (tenggat tetap berlaku) tetapi lonjakan memori sesaat; pada mesin dengan
RAM kecil bisa memicu paging.

**Arah perbaikan:** buka respons dengan `HttpCompletionOption::ResponseHeadersRead` lalu baca
`IInputStream` bertahap, berhenti dan batalkan begitu melewati `MAX_BODY_BYTES`.

#### J2 — `decode_bytes` membatasi hasil, bukan ukuran sumber gambar (baca kode, info)

**Lokasi:** `crates/poni/src/services/imaging.rs:125-138` (`decode_with`), `:194-213` (`to_pbgra`).

**Mekanisme:** `source_size` membaca dimensi asli, `fit_within` menjepit **keluaran** ke
`MAX_DECODE_PX` (4096), lalu `to_pbgra` bekerja: untuk format yang tidak termasuk `scale_first`
(PNG dengan alpha, palet, CMYK, bit-depth tinggi) ia memasang `convert_pbgra` pada ukuran penuh
lebih dulu, baru kemudian penskala. Jadi gambar raksasa (mis. PNG 20000×20000) bisa memaksa
alokasi antara yang sangat besar sebelum akhirnya gagal sebagai error decode.

**Jalur masuk (semuanya input luar):** gambar kiriman HP (`phonedrop`), thumbnail Shelf, foto
pengirim dari `wpndatabase.db`, dan album art dari player lewat SMTC.

**Dampak:** biasanya berakhir sebagai `image decode failed` (aman), tetapi bisa menyendatkan mesin
sebentar karena paging. Bukan panic.

**Arah perbaikan:** tolak lebih awal bila `w * h` melebihi batas piksel yang wajar (mis.
`MAX_DECODE_PX² × 4`), sebelum WIC menyentuh piksel.

### Diperiksa dan bersih (J)

- `http.rs` — tiap permintaan punya tenggat (`Instant`), `wait` membatalkan operasi saat tenggat
  lewat, badan dibatasi 8 MB (dua kali: `Content-Length` dan panjang buffer), error WinINet
  (12001–12199) dipetakan ke pesan manusiawi, dan `Client` tidak melakukan retry sendiri (pemanggil
  yang membatasi).
- `imaging.rs` — semua decode lewat WIC dengan batas keluaran 4096 px, `checked_mul` untuk stride
  dan panjang buffer, `save_png` memvalidasi ukuran buffer sebelum menulis, dan ada guard COM MTA.
- `bluetooth.rs` — watcher dihentikan saat `Drop`, restart dibatasi lalu "giving up" (`:618`),
  tidak ada polling: worker menunggu pesan.
- `pages/shelf.rs` — tombol hapus hanya membuang **referensi** (`st.store.shelf.remove`), tidak
  menyentuh file di disk; nama berkas dari HP tetap lewat `safe_name`/`names::place`.
- `pages/card.rs` — murni tata letak, tanpa state.

---

# Ringkasan Putaran 2

| Area | Berkas | Prioritas | Polish/info |
|---|---|---|---|
| F — kalender/task/Hijriah | 5 | F1 | F2, F3 |
| G — musik/lirik | 6 | — | G1, G2, G3 |
| H — settings/backup | 4 | H1, H2 | H3, H4 |
| I — waktu sholat | 4 | I1 (terukur) | I2, I3, I4 |
| J — sisanya | 12 | — | J1, J2 |
| **Total** | **31** | **4** | **12** (16 temuan) |

Temuan yang paling layak dikerjakan lebih dulu: **H1** (backup tertimpa — kehilangan data),
**I1** (2,1 ms CPU/detik saat beranimasi — terukur), **F1** (edit nyangkut → data berubah tanpa
disadari), lalu **H2** dan **I2** (salah pesan / dokumen tidak sesuai perilaku).

# Belum tercakup (jujur, jangan dianggap bersih)

Putaran 1 dan 2 membaca penuh atau mendalam ± 6.500 dari 62.568 baris. Berkas yang **belum**
dibaca baris per baris:

- `settings_window.rs` (2.713) — hanya bagian Export/Import dan baris yang dipakai area H.
- `pages/shelf.rs` (1.004) — hanya alur hapus/drag/berkas; tata letak, panel HP, dan unggah belum.
- `services/bluetooth.rs` (910) — hanya struktur watcher dan pemulihan; pembacaan properti
  perangkat belum.
- `pages/prayer.rs` (682) — hanya `next_wake`/`pemilih/`lokasi; penggambaran jadwal dan glow belum.
- `pages/media.rs` (1.352) — hanya blok lirik/scroll/klik; tata letak, equalizer, dan tombol belum.
- `pages/clipboard.rs` (535), `pages/focus.rs` (433), `pages/script.rs` (419),
  `pages/notes.rs` (361), `pages/notifications.rs` (127), `pages/screentime.rs` (123),
  `pages/addons.rs` (186) — baru dipindai pola risikonya, belum dibaca alurnya.
- `poni-core/prayer/adhan.rs` (matematika) dan `poni-core/prayer/world.rs` — hanya lewat pemakaian.
- `poni-core/hijri/astro.rs` (385) — hanya lewat pemakaian.
- `services/http.rs` bagian `parse_retry_after`/`parse_http_date` — tidak dibaca rinci.
- `crates/poni/src/mac/**` (tidak di-build, sengaja dilewati) dan `selftest.rs` (harness).

**Batas metode:** audit statis (baca kode + dua pengukuran), bukan fuzzing atau uji runtime.
Keyakinan tiap temuan dicantumkan; hanya I1 yang diukur, A1 (putaran 1) yang dibuktikan tes.
