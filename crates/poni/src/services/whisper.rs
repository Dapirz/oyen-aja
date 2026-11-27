//! Local Whisper (the `whisper` branch only, never on `main`; see CLAUDE.md):
//! Meeting notes' transcript made on this PC, and Upload's file read in as
//! a meeting.
//!
//! The add-on's files (`poni_core::addons::LOCAL_WHISPER`) are whisper.cpp's
//! own Windows build with CUDA, unzipped next to them on first use with
//! Windows' tar.exe, and Whisper large-v3 with Silero's voice activity
//! model. Each part of a meeting goes through `whisper-cli.exe` in turn, a
//! process of its own that exists only while it transcribes, so the
//! island's own memory doesn't move (the model's 3 GB sit on the GPU of
//! that process). It's tied to a job object that ends it with Hihu.
//!
//! Upload turns a video or audio file into the same five-minute 16 kHz WAV
//! parts a recording leaves, with Windows' own media transcoder (whatever
//! Windows plays: MP4, M4A, MP3, WAV, …), so the rest of Meeting notes
//! takes it like any meeting.

use crate::services::addons::{file_path, system_tool};
use poni_core::addons::{LOCAL_WHISPER, WHISPER_MODEL, WHISPER_VAD, WHISPER_ZIP};
use poni_core::notes::{Line, PART_SECS, RATE};
use poni_core::wav::WavWriter;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// The tools' folder beside the zip.
const BIN: &str = "bin";

/// The add-on's tools, unzipped and in place.
pub struct Tools {
    cli: PathBuf,
    model: PathBuf,
    vad: PathBuf,
}

/// The tools, unzipping whisper.cpp's build the first time (into a folder
/// of its own that's moved in only once whole). Err says what's missing.
pub fn tools() -> Result<Tools, String> {
    let (zip, model, vad) = (file_path(&LOCAL_WHISPER, WHISPER_ZIP), file_path(&LOCAL_WHISPER, WHISPER_MODEL), file_path(&LOCAL_WHISPER, WHISPER_VAD));
    if let Some(p) = [&zip, &model, &vad].into_iter().find(|p| !p.is_file()) {
        return Err(format!("{} isn't on this PC (is Local Whisper on and downloaded?)", p.display()));
    }
    let bin = zip.with_file_name(BIN);
    if let Some(cli) = find_cli(&bin) {
        return Ok(Tools { cli, model, vad });
    }
    let tmp = zip.with_file_name("bin.tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("no folder for the tools ({e})"))?;
    crate::pinfo!("whisper: unzipping its tools");
    let unzipped = Command::new(system_tool("tar.exe"))
        .arg("-xf")
        .arg(&zip)
        .arg("-C")
        .arg(&tmp)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("tar.exe didn't start ({e})"))?;
    if !unzipped.success() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("tar.exe couldn't unzip {WHISPER_ZIP} ({unzipped})"));
    }
    let _ = std::fs::remove_dir_all(&bin);
    std::fs::rename(&tmp, &bin).map_err(|e| format!("couldn't move the tools in ({e})"))?;
    let cli = find_cli(&bin).ok_or_else(|| format!("no whisper-cli.exe in {WHISPER_ZIP}"))?;
    Ok(Tools { cli, model, vad })
}

/// `whisper-cli.exe` in `dir` or a folder in it (the zip keeps it in `Release\`).
fn find_cli(dir: &Path) -> Option<PathBuf> {
    let here = dir.join("whisper-cli.exe");
    if here.is_file() {
        return Some(here);
    }
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path().join("whisper-cli.exe")).find(|p| p.is_file())
}

/// A part's length, from its size: 16-bit mono at [`RATE`] after a 44-byte
/// header (a part cut short by a crash has 0 in its header, not its size).
fn wav_secs(p: &Path) -> f64 {
    std::fs::metadata(p).map(|m| m.len().saturating_sub(44) as f64 / 2.0 / RATE as f64).unwrap_or(0.0)
}

/// A job object that ends whisper-cli with Hihu (a crash or Quit mid
/// transcript), so nothing goes on holding the GPU. Made once, kept open
/// for the app's life; its handle as a number, for a static.
fn kill_with_hihu(child: &std::process::Child) {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    static JOB: OnceLock<usize> = OnceLock::new();
    let job = *JOB.get_or_init(|| unsafe {
        let Ok(job) = CreateJobObjectW(None, None) else { return 0 };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let set = SetInformationJobObject(job, JobObjectExtendedLimitInformation, &info as *const _ as *const _, std::mem::size_of_val(&info) as u32);
        if set.is_err() {
            return 0;
        }
        job.0 as usize
    });
    if job != 0 {
        unsafe {
            let _ = AssignProcessToJobObject(HANDLE(job as *mut _), HANDLE(child.as_raw_handle()));
        }
    }
}

/// Transcribe a meeting's `parts` in order, each a whisper-cli run whose
/// JSON goes in `work`; `progress` hears 0..1 of the whole. The first part
/// finds the language (Whisper's own detection) and the rest keep it, so a
/// part that opens with a few English words stays Indonesian. Lines whose
/// time is the part's last second are dropped: there Whisper makes up a
/// closing line ("Terima kasih", "selamat menikmati") over silence.
pub fn transcribe(t: &Tools, parts: &[PathBuf], work: &Path, mut progress: impl FnMut(f32)) -> Result<Vec<Line>, String> {
    let secs: Vec<f64> = parts.iter().map(|p| wav_secs(p)).collect();
    let total = secs.iter().sum::<f64>().max(1.0);
    let (mut lines, mut at, mut lang) = (Vec::new(), 0.0, "auto".to_string());
    let cwd = t.cli.parent().map(Path::to_path_buf).unwrap_or_default();
    for (i, part) in parts.iter().enumerate() {
        let base = work.join(format!("whisper-{i:03}"));
        let json_path = base.with_extension("json");
        let _ = std::fs::remove_file(&json_path);
        let mut child = Command::new(&t.cli)
            .arg("-m")
            .arg(&t.model)
            .arg("-f")
            .arg(part)
            .args(["-l", lang.as_str(), "-bs", "5", "-mc", "0", "-pp", "-oj", "-of"])
            .arg(&base)
            .arg("--vad")
            .arg("-vm")
            .arg(&t.vad)
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("whisper-cli.exe didn't start ({e})"))?;
        kill_with_hihu(&child);
        // Its log: "…: progress =  45%" now and then, the rest kept for a failure.
        let mut tail: Vec<String> = Vec::new();
        if let Some(err) = child.stderr.take() {
            for line in BufReader::new(err).split(b'\n').map_while(Result::ok) {
                let line = String::from_utf8_lossy(&line).trim().to_string();
                if let Some(pct) = line.split("progress =").nth(1).and_then(|s| s.trim().trim_end_matches('%').trim().parse::<f64>().ok()) {
                    progress(((at + secs[i] * pct / 100.0) / total) as f32);
                } else if !line.is_empty() {
                    tail.push(line);
                    if tail.len() > 6 {
                        tail.remove(0);
                    }
                }
            }
        }
        let status = child.wait().map_err(|e| format!("whisper-cli.exe: {e}"))?;
        if !status.success() {
            return Err(format!("whisper-cli.exe ended with {} ({})", status.code().unwrap_or(-1), tail.join(" | ")));
        }
        let text = std::fs::read_to_string(&json_path).map_err(|e| format!("no transcript from whisper-cli.exe ({e})"))?;
        let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("whisper-cli.exe's JSON didn't read ({e})"))?;
        if i == 0 {
            if let Some(l) = v["result"]["language"].as_str().filter(|l| !l.is_empty() && *l != "auto") {
                crate::pinfo!("whisper: the meeting is in {l}");
                lang = l.to_string();
            }
        }
        for seg in v["transcription"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let from = seg["offsets"]["from"].as_f64().unwrap_or(0.0) / 1000.0;
            let said = seg["text"].as_str().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
            // Nothing said, a sound marked ("[Musik]", "(tertawa)"), or the made-up closing line.
            let marked = (said.starts_with('[') && said.ends_with(']')) || (said.starts_with('(') && said.ends_with(')'));
            if said.is_empty() || marked || from >= secs[i] - 1.0 {
                continue;
            }
            lines.push(Line { t: at + from, who: String::new(), text: said });
        }
        let _ = std::fs::remove_file(&json_path);
        at += secs[i];
        progress((at / total) as f32);
    }
    Ok(lines)
}

/// Upload: ask for a video or audio file (Windows' own Open dialog, on
/// this thread, which it makes single-threaded for the dialog). None:
/// cancelled.
pub fn pick_file() -> Option<PathBuf> {
    use windows::core::w;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, SIGDN_FILESYSPATH};
    unsafe {
        let inited = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
        let picked = (|| -> windows::core::Result<PathBuf> {
            let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            let kinds = [
                COMDLG_FILTERSPEC { pszName: w!("Video or audio"), pszSpec: w!("*.mp4;*.m4a;*.mp3;*.wav;*.mkv;*.mov;*.webm;*.aac;*.wma;*.flac;*.ogg;*.3gp") },
                COMDLG_FILTERSPEC { pszName: w!("All files"), pszSpec: w!("*.*") },
            ];
            dlg.SetFileTypes(&kinds)?;
            dlg.SetTitle(w!("Pick a meeting's video or audio"))?;
            dlg.SetOptions(dlg.GetOptions()? | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM)?;
            dlg.Show(None)?;
            let name = dlg.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
            let path = PathBuf::from(name.to_string().unwrap_or_default());
            CoTaskMemFree(Some(name.0 as *const _));
            Ok(path)
        })();
        if inited {
            CoUninitialize();
        }
        picked.ok().filter(|p| !p.as_os_str().is_empty())
    }
}

/// Upload: `src` (anything Windows plays) as five-minute 16 kHz mono WAV
/// parts in `dir`, named as a recording's are: the parts and their length
/// in seconds. Windows' media transcoder writes one WAV (`import.wav`, gone
/// after), which is cut into parts 64 KB at a time. Err: no sound Windows
/// can read in it. Runs on a thread of its own (COM's multithreaded
/// apartment, set up by the caller).
pub fn import(src: &Path, dir: &Path) -> Result<(Vec<PathBuf>, f64), String> {
    use windows::core::HSTRING;
    use windows::Media::MediaProperties::{AudioEncodingQuality, MediaEncodingProfile};
    use windows::Media::Transcoding::MediaTranscoder;
    use windows::Storage::{CreationCollisionOption, StorageFile, StorageFolder};
    let whole = dir.join("import.wav");
    let transcoded = (|| -> windows::core::Result<bool> {
        let input = StorageFile::GetFileFromPathAsync(&HSTRING::from(src.as_os_str()))?.get()?;
        let folder = StorageFolder::GetFolderFromPathAsync(&HSTRING::from(dir.as_os_str()))?.get()?;
        let output = folder.CreateFileAsync(&HSTRING::from("import.wav"), CreationCollisionOption::ReplaceExisting)?.get()?;
        let profile = MediaEncodingProfile::CreateWav(AudioEncodingQuality::Low)?;
        let audio = profile.Audio()?;
        audio.SetSampleRate(RATE)?;
        audio.SetChannelCount(1)?;
        audio.SetBitsPerSample(16)?;
        let prepared = MediaTranscoder::new()?.PrepareFileTranscodeAsync(&input, &output, &profile)?.get()?;
        if !prepared.CanTranscode()? {
            crate::pwarn!("whisper: Windows can't read the sound in that file ({:?})", prepared.FailureReason());
            return Ok(false);
        }
        prepared.TranscodeAsync()?.get()?;
        Ok(true)
    })();
    match transcoded {
        Ok(true) => {}
        Ok(false) => {
            let _ = std::fs::remove_file(&whole);
            return Err("no sound Windows can read".into());
        }
        Err(e) => {
            let _ = std::fs::remove_file(&whole);
            return Err(format!("Windows' transcoder: {e}"));
        }
    }
    let cut = split_wav(&whole, dir);
    let _ = std::fs::remove_file(&whole);
    cut
}

/// One 16 kHz mono 16-bit WAV as `part-000.wav`, `part-001.wav`, … of
/// [`PART_SECS`] each: the PCM of its `data` chunk, read 64 KB at a time.
fn split_wav(whole: &Path, dir: &Path) -> Result<(Vec<PathBuf>, f64), String> {
    let mut f = std::fs::File::open(whole).map_err(|e| format!("the WAV didn't open ({e})"))?;
    let mut head = [0u8; 12];
    f.read_exact(&mut head).map_err(|_| "not a WAV".to_string())?;
    if &head[0..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return Err("not a WAV".into());
    }
    // The chunks up to `data`: `fmt ` says what the samples are.
    let data_len: u64;
    loop {
        let mut ch = [0u8; 8];
        f.read_exact(&mut ch).map_err(|_| "a WAV with no data".to_string())?;
        let len = u32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]]) as u64;
        match &ch[0..4] {
            b"fmt " => {
                let mut fmt = vec![0u8; len as usize];
                f.read_exact(&mut fmt).map_err(|_| "a WAV cut short".to_string())?;
                let channels = u16::from_le_bytes([fmt[2], fmt[3]]);
                let rate = u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]);
                let bits = u16::from_le_bytes([fmt[14], fmt[15]]);
                if (channels, rate, bits) != (1, RATE, 16) {
                    return Err(format!("the WAV is {rate} Hz, {channels} channels, {bits} bits"));
                }
                if len % 2 == 1 {
                    f.seek(SeekFrom::Current(1)).map_err(|e| e.to_string())?;
                }
            }
            b"data" => {
                // 0 or past the end: a writer that didn't come back to fill it in.
                let left = f.metadata().map(|m| m.len()).unwrap_or(0).saturating_sub(f.stream_position().unwrap_or(0));
                data_len = if len == 0 || len > left { left } else { len };
                break;
            }
            _ => {
                f.seek(SeekFrom::Current((len + len % 2) as i64)).map_err(|e| e.to_string())?;
            }
        }
    }
    let per_part = PART_SECS as u64 * RATE as u64;
    let (mut parts, mut writer, mut in_part, mut left) = (Vec::new(), None::<WavWriter>, 0u64, data_len / 2);
    let mut buf = vec![0u8; 64 * 1024];
    while left > 0 {
        let want = ((left * 2) as usize).min(buf.len());
        let got = f.read(&mut buf[..want]).map_err(|e| e.to_string())?;
        if got < 2 {
            break;
        }
        let mut pcm: Vec<i16> = buf[..got & !1].as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect();
        left -= pcm.len() as u64;
        while !pcm.is_empty() {
            if writer.is_none() {
                writer = Some(WavWriter::create(&dir.join(format!("part-{:03}.wav", parts.len()))).map_err(|e| e.to_string())?);
                in_part = 0;
            }
            let n = ((per_part - in_part) as usize).min(pcm.len());
            writer.as_mut().unwrap().write(&pcm[..n]).map_err(|e| e.to_string())?;
            in_part += n as u64;
            pcm.drain(..n);
            if in_part == per_part {
                parts.push(writer.take().unwrap().finish().map_err(|e| e.to_string())?.0);
            }
        }
    }
    if let Some(w) = writer.take() {
        parts.push(w.finish().map_err(|e| e.to_string())?.0);
    }
    let secs = (data_len / 2) as f64 / RATE as f64;
    if parts.is_empty() || secs < 1.0 {
        return Err("no sound in it".into());
    }
    Ok((parts, secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole of it on this PC, with Local Whisper's files in place:
    /// `PONI_WHISPER_FILE=<a video or audio file> cargo test -p poni whisper -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn a_file_is_read_in_and_transcribed_on_this_pc() {
        let Some(src) = std::env::var_os("PONI_WHISPER_FILE").map(PathBuf::from) else { return };
        let dir = std::env::temp_dir().join(format!("poni-whisper-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _com = crate::services::http::MtaGuard::new();
        let begun = std::time::Instant::now();
        let (parts, secs) = import(&src, &dir).expect("read in");
        println!("read in: {} parts, {secs:.1} s, in {} ms", parts.len(), begun.elapsed().as_millis());
        assert!((wav_secs(&parts[0]) - secs.min(PART_SECS as f64)).abs() < 0.5);
        let t = tools().expect("tools");
        let begun = std::time::Instant::now();
        let mut seen = Vec::new();
        let lines = transcribe(&t, &parts, &dir, |d| seen.push(d)).expect("transcribed");
        println!("transcribed: {} lines in {:.1} s; progress {:?}", lines.len(), begun.elapsed().as_secs_f64(), seen);
        for l in lines.iter().take(8) {
            println!("  [{:.1}] {}", l.t, l.text);
        }
        assert!(!lines.is_empty());
        assert!(seen.last().is_some_and(|d| (*d - 1.0).abs() < 1e-3), "ends at 100%");
        assert!(lines.windows(2).all(|w| w[0].t <= w[1].t), "in order");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
