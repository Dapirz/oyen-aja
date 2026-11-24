//! The default microphone and the PC's own sound, opened for Meeting notes
//! (`services/notes.rs`): WASAPI shared mode in Windows' mix format, which
//! `poni_core::wav::Downmix` turns into 16 kHz mono itself, the same on
//! every PC. From Oyenza's IELTS microphone (removed in 382c41a).
//!
//! Why WASAPI and not WinRT's AudioGraph: `IAudioClient` and
//! `IAudioCaptureClient` come with Core Audio, which the volume watcher
//! (`services/audio.rs`) already loads; AudioGraph would bring Media
//! Foundation's pipeline (several MB of DLLs and a graph thread).

use poni_core::wav::{Downmix, SampleFormat};
use windows::core::{w, GUID, HRESULT};
use windows::Win32::Foundation::E_ACCESSDENIED;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, EDataFlow, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_E_DEVICE_INVALIDATED,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};

/// The device buffer: 500 ms, so a capture that wakes late loses nothing.
const BUFFER_HNS: i64 = 5_000_000;

/// `HRESULT_FROM_WIN32(ERROR_NOT_FOUND)`: no default recording device.
const E_NOTFOUND: HRESULT = HRESULT(0x8007_0490_u32 as i32);
/// The mix format's tags and sub-formats (Multimedia / KernelStreaming,
/// which the `windows` crate puts behind features this app doesn't need).
const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MicError {
    Denied,
    NoDevice,
    Failed(String),
}

impl std::fmt::Display for MicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MicError::Denied => f.write_str("microphone access is off for desktop apps"),
            MicError::NoDevice => f.write_str("no microphone"),
            MicError::Failed(s) => f.write_str(s),
        }
    }
}

/// Windows' privacy switches: "Microphone access" off, or "Let desktop apps
/// access your microphone" off. Shared-mode capture then either fails with
/// E_ACCESSDENIED or delivers silence, depending on the Windows version, so
/// ask first.
pub fn privacy_denies() -> bool {
    let base = w!("Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone");
    let nonpackaged = w!("Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone\\NonPackaged");
    [base, nonpackaged].into_iter().any(|key| consent(key).is_some_and(|v| v.eq_ignore_ascii_case("Deny")))
}

fn consent(key: windows::core::PCWSTR) -> Option<String> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let mut buf = [0u16; 16];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, key, w!("Value"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut core::ffi::c_void), Some(&mut size)) };
    if r.is_err() {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

pub fn mic_error(what: &str, e: &windows::core::Error) -> MicError {
    match e.code() {
        E_ACCESSDENIED => MicError::Denied,
        E_NOTFOUND | AUDCLNT_E_DEVICE_INVALIDATED => MicError::NoDevice,
        c => MicError::Failed(format!("{what} (0x{:08X})", c.0 as u32)),
    }
}

/// The open device, ready to start.
pub struct Device {
    pub client: IAudioClient,
    pub capture: IAudioCaptureClient,
    pub downmix: Downmix,
    pub block: usize,
}

/// The default device of `flow`, opened in shared mode in its own format:
/// a recording device (`eCapture`), or the playback device's own sound
/// (`eRender`, WASAPI loopback: what the PC plays, a call's voices
/// included). `what` names it in the log.
pub fn open_endpoint(flow: EDataFlow, what: &str, id: Option<&str>) -> Result<Device, MicError> {
    let loopback = flow != eCapture;
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| mic_error("no Core Audio", &e))?;
        // A device picked in Settings, else Windows' default.
        let picked = id.and_then(|id| devices.GetDevice(&windows::core::HSTRING::from(id)).ok());
        let device = match picked {
            Some(d) => d,
            None => devices.GetDefaultAudioEndpoint(flow, eConsole).map_err(|e| mic_error(if loopback { "no playback device" } else { "no recording device" }, &e))?,
        };
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(|e| mic_error("can't open the microphone", &e))?;
        let mix = client.GetMixFormat().map_err(|e| mic_error("no recording format", &e))?;
        let format = sample_format(mix);
        let wf = std::ptr::read_unaligned(mix);
        let flags = if loopback { AUDCLNT_STREAMFLAGS_LOOPBACK } else { 0 };
        let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, BUFFER_HNS, 0, mix, None);
        CoTaskMemFree(Some(mix as *const core::ffi::c_void));
        init.map_err(|e| mic_error("can't start the microphone", &e))?;
        let format = format.ok_or_else(|| MicError::Failed(format!("recording format {} with {} bits isn't one this app reads", { wf.wFormatTag }, { wf.wBitsPerSample })))?;
        let capture: IAudioCaptureClient = client.GetService().map_err(|e| mic_error("can't read the microphone", &e))?;
        let (rate, channels, block) = (wf.nSamplesPerSec, wf.nChannels, wf.nBlockAlign as usize);
        if block != format.bytes() * channels as usize {
            return Err(MicError::Failed(format!("recording frames of {block} bytes for {channels} channels")));
        }
        crate::pinfo!("{what} at {rate} Hz, {channels} channel(s), {format:?}");
        Ok(Device { client, capture, downmix: Downmix::new(rate, channels, format), block })
    }
}

/// The microphones Windows has on now: each one's id and name, for
/// Settings > Meeting notes.
pub fn microphones() -> Vec<(String, String)> {
    use windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE;
    use windows::Win32::System::Com::STGM_READ;
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
    // PKEY_Device_FriendlyName.
    const FRIENDLY: PROPERTYKEY = PROPERTYKEY { fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0), pid: 14 };
    let mut out = Vec::new();
    unsafe {
        let Ok(devices) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return out };
        let Ok(list) = devices.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) else { return out };
        for i in 0..list.GetCount().unwrap_or(0) {
            let Ok(d) = list.Item(i) else { continue };
            let Ok(p) = d.GetId() else { continue };
            let id = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.0 as *const _));
            let name = d
                .OpenPropertyStore(STGM_READ)
                .and_then(|s| s.GetValue(&FRIENDLY))
                .and_then(|v| PropVariantToStringAlloc(&v))
                .map(|s| {
                    let n = s.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(s.0 as *const _));
                    n
                })
                .unwrap_or_default();
            if !id.is_empty() {
                out.push((id, if name.is_empty() { "Microphone".into() } else { name }));
            }
        }
    }
    out
}

/// The mix format's samples, if this app reads them.
unsafe fn sample_format(f: *const WAVEFORMATEX) -> Option<SampleFormat> {
    let wf = std::ptr::read_unaligned(f);
    let tag = if wf.wFormatTag == WAVE_FORMAT_EXTENSIBLE && wf.cbSize >= 22 {
        let ext = std::ptr::read_unaligned(f as *const WAVEFORMATEXTENSIBLE);
        // KSDATAFORMAT_SUBTYPE_PCM / _IEEE_FLOAT: the old tag in the GUID's first field.
        let sub: GUID = ext.SubFormat;
        sub.data1 as u16
    } else {
        wf.wFormatTag
    };
    match (tag, wf.wBitsPerSample) {
        (WAVE_FORMAT_IEEE_FLOAT, 32) => Some(SampleFormat::F32),
        (WAVE_FORMAT_PCM, 16) => Some(SampleFormat::I16),
        (WAVE_FORMAT_PCM, 24) => Some(SampleFormat::I24),
        (WAVE_FORMAT_PCM, 32) => Some(SampleFormat::I32),
        _ => None,
    }
}
