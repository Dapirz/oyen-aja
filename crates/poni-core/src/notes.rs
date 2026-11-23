//! Meeting notes, the Meeting notes add-on: one click and the island records
//! the microphone and the PC's own sound together, so a call in Meet, Zoom,
//! Teams or WhatsApp is heard with nobody joining it; after Stop, Google's
//! Gemini (with the person's own API key) hears the meeting and writes the
//! notes and the transcript, which are saved as a Markdown file on this PC.
//!
//! This is the part without Windows: the two sources mixed onto one track
//! ([`Mixer`]), the Gemini calls' addresses, bodies and answers, the page's
//! moments ([`Taker`]), what went wrong in plain words ([`Problem`]), and the
//! notes as Markdown. The capture, the uploads and the file are
//! `services/notes.rs`. The recorder and the page come from Oyenza's Meeting
//! notes (removed in e031e17), which sent the audio to Oyenza's server.

use crate::datetime::{LocalDateTime, MONTH_SHORT, WEEKDAY_SHORT};
use serde_json::{json, Value};
use std::collections::VecDeque;

pub use crate::wav::RATE;

/// Gemini's API.
pub const API: &str = "https://generativelanguage.googleapis.com";
/// Each part of the audio: five minutes (9.6 MB), written and uploaded in turn.
pub const PART_SECS: u32 = 300;
/// The longest meeting taken: two hours, whose transcript still fits in one
/// answer. The recording stops itself there.
pub const MAX_SECS: f64 = 2.0 * 3600.0;
/// Where people make a key.
pub const KEY_URL: &str = "https://aistudio.google.com/apikey";

// ---------------------------------------------------------------- the mix

/// How far behind the clock the mix runs, so each source's late packets
/// still land where they were heard (a capture wakes every 40 ms).
const LAG_SAMPLES: u64 = RATE as u64 / 5;
/// A source this far ahead of the mix (its clock running fast against the
/// PC's) loses its oldest samples, so the two never drift apart.
const MAX_QUEUE: usize = RATE as usize * 3 / 5;

/// The microphone and the PC's sound (each already 16 kHz mono) mixed onto
/// one track, in step with the clock. The PC's sound stops coming while
/// nothing plays (Windows sends no loopback packets then), so the clock,
/// not either source, says how much track there is: a source with nothing
/// to give adds silence.
#[derive(Default)]
pub struct Mixer {
    mic: VecDeque<i16>,
    pc: VecDeque<i16>,
    /// Samples mixed out so far.
    out: u64,
}

impl Mixer {
    pub fn push_mic(&mut self, pcm: &[i16]) {
        push(&mut self.mic, pcm);
    }

    pub fn push_pc(&mut self, pcm: &[i16]) {
        push(&mut self.pc, pcm);
    }

    /// The track up to `clock` samples since the start (less the lag),
    /// appended to `out`.
    pub fn take(&mut self, clock: u64, out: &mut Vec<i16>) {
        let due = clock.saturating_sub(LAG_SAMPLES);
        while self.out < due {
            let m = self.mic.pop_front().unwrap_or(0) as i32;
            let p = self.pc.pop_front().unwrap_or(0) as i32;
            out.push((m + p).clamp(i16::MIN as i32, i16::MAX as i32) as i16);
            self.out += 1;
        }
    }

    /// Everything still waiting, at the end of the recording.
    pub fn drain(&mut self, out: &mut Vec<i16>) {
        let left = self.mic.len().max(self.pc.len()) as u64;
        self.take(self.out + LAG_SAMPLES + left, out);
    }
}

fn push(q: &mut VecDeque<i16>, pcm: &[i16]) {
    q.extend(pcm.iter().copied());
    if q.len() > MAX_QUEUE {
        let extra = q.len() - MAX_QUEUE;
        q.drain(..extra);
    }
}

// ---------------------------------------------------------------- settings

/// The notes' language (Settings): Indonesian, or English.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    Id,
    En,
}

impl Lang {
    pub fn from_key(s: &str) -> Lang {
        match s {
            "en" => Lang::En,
            _ => Lang::Id,
        }
    }

    fn rule(self) -> &'static str {
        match self {
            Lang::Id => "Write the title, summary, decisions, actions and topics in Indonesian (Bahasa Indonesia), whatever language is spoken. Keep the transcript as it was spoken.",
            Lang::En => "Write the title, summary, decisions, actions and topics in English, whatever language is spoken. Keep the transcript as it was spoken.",
        }
    }
}

/// The models Settings offers: Google's names that always point at the
/// newest of each kind, so they keep working as models come and go.
/// Pro isn't on Gemini's free tier.
pub const MODELS: [(&str, &str); 3] = [("gemini-flash-latest", "Flash"), ("gemini-flash-lite-latest", "Flash-Lite"), ("gemini-pro-latest", "Pro (paid)")];
pub const DEFAULT_MODEL: &str = MODELS[0].0;
/// Asked when the chosen model is busy (503) or not there for the key:
/// on the free tier too, and usually less in demand.
pub const FALLBACK_MODEL: &str = "gemini-flash-lite-latest";

/// The models to ask in turn: the chosen one, then the fallback.
pub fn models_to_try(chosen: &str) -> Vec<&str> {
    if chosen == FALLBACK_MODEL {
        vec![chosen]
    } else {
        vec![chosen, FALLBACK_MODEL]
    }
}

// ---------------------------------------------------------------- the calls

/// Start a resumable upload of one part (`POST`, the file's metadata).
pub fn upload_start_url() -> String {
    format!("{API}/upload/v1beta/files")
}

pub fn upload_start_body(name: &str) -> String {
    json!({ "file": { "display_name": name } }).to_string()
}

/// An uploaded file's own address (`GET` its state, `DELETE` it), from its
/// name (`files/abc123`).
pub fn file_url(name: &str) -> String {
    format!("{API}/v1beta/{name}")
}

pub fn generate_url(model: &str) -> String {
    format!("{API}/v1beta/models/{}:generateContent", crate::urlenc::encode(model))
}

/// An uploaded file, as Gemini answers an upload or a look at it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    /// `files/abc123`.
    pub name: String,
    pub uri: String,
    /// `PROCESSING`, `ACTIVE` or `FAILED`.
    pub state: String,
}

/// The file in an upload's answer (`{"file": {…}}`) or a look's (`{…}`).
pub fn parse_file(body: &str) -> Option<File> {
    let v = obj(body)?;
    let f = if v["file"].is_object() { &v["file"] } else { &v };
    let name = f["name"].as_str()?.to_string();
    let uri = f["uri"].as_str()?.to_string();
    let state = f["state"].as_str().unwrap_or("ACTIVE").to_string();
    (!name.is_empty() && !uri.is_empty()).then_some(File { name, uri, state })
}

/// What Gemini is asked for: the shape of [`Notes`], with timestamps as
/// "MM:SS" from the start of the meeting.
fn prompt(lang: Lang, parts: usize) -> String {
    let split = if parts > 1 {
        format!(" The meeting comes as {parts} audio files in order, each {} minutes long except the last; timestamps count from the start of the first one.", PART_SECS / 60)
    } else {
        String::new()
    };
    format!(
        "This is the audio of a meeting or a call: one microphone in the room and the computer's own sound (the other people on the call), mixed together.{split}\n\
         Write meeting notes and a transcript as JSON.\n\
         - title: a short title for the meeting (a few words, no date).\n\
         - summary: 3 to 6 short bullet points of what was discussed.\n\
         - decisions: what was decided, if anything.\n\
         - actions: tasks someone agreed to do: who (a name, if said), what, and when (if a time was said).\n\
         - topics: the main topics, each with its key points.\n\
         - transcript: every turn of speech in order, as t (\"MM:SS\", or \"H:MM:SS\" past an hour, from the start), who (the speaker's name if it can be told from the talk, else \"Speaker 1\", \"Speaker 2\", …) and text (what they said, as said, in its own language).\n\
         {}\n\
         Leave a list empty rather than inventing anything. If nobody speaks in the audio, give an empty transcript.",
        lang.rule()
    )
}

/// The answer's shape, so Gemini answers with JSON that reads.
fn schema() -> Value {
    let strings = json!({ "type": "ARRAY", "items": { "type": "STRING" } });
    json!({
        "type": "OBJECT",
        "properties": {
            "title": { "type": "STRING" },
            "summary": strings,
            "decisions": strings,
            "actions": { "type": "ARRAY", "items": { "type": "OBJECT", "properties": {
                "who": { "type": "STRING" }, "what": { "type": "STRING" }, "when": { "type": "STRING" } }, "required": ["what"] } },
            "topics": { "type": "ARRAY", "items": { "type": "OBJECT", "properties": {
                "title": { "type": "STRING" }, "points": strings }, "required": ["title", "points"] } },
            "transcript": { "type": "ARRAY", "items": { "type": "OBJECT", "properties": {
                "t": { "type": "STRING" }, "who": { "type": "STRING" }, "text": { "type": "STRING" } }, "required": ["t", "who", "text"] } }
        },
        "required": ["title", "summary", "decisions", "actions", "topics", "transcript"],
        "propertyOrdering": ["title", "summary", "decisions", "actions", "topics", "transcript"]
    })
}

/// `generateContent`'s body: each part's file (with where it starts), then
/// the ask.
pub fn generate_body(files: &[File], lang: Lang) -> String {
    let mut parts = Vec::new();
    for (i, f) in files.iter().enumerate() {
        if files.len() > 1 {
            parts.push(json!({ "text": format!("Audio file {} of {}, from {}:", i + 1, files.len(), fmt_length((i as u32 * PART_SECS) as f64)) }));
        }
        parts.push(json!({ "file_data": { "mime_type": "audio/wav", "file_uri": f.uri } }));
    }
    parts.push(json!({ "text": prompt(lang, files.len()) }));
    json!({
        "contents": [{ "role": "user", "parts": parts }],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseSchema": schema(),
            "temperature": 0.2,
            "maxOutputTokens": 65536
        }
    })
    .to_string()
}

fn obj(body: &str) -> Option<Value> {
    serde_json::from_str::<Value>(body).ok().filter(Value::is_object)
}

fn text(v: &Value) -> String {
    v.as_str().map(|s| s.split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_default()
}

fn texts(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().map(text).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
}

/// "12:34" or "1:02:03" (or a plain number of seconds) as seconds.
fn stamp(v: &Value) -> f64 {
    if let Some(n) = v.as_f64() {
        return if n.is_finite() { n.max(0.0) } else { 0.0 };
    }
    let s = v.as_str().unwrap_or("").trim();
    let mut secs = 0.0;
    for part in s.split(':') {
        match part.trim().parse::<f64>() {
            Ok(n) if n.is_finite() && n >= 0.0 => secs = secs * 60.0 + n,
            _ => return 0.0,
        }
    }
    secs
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Action {
    pub who: String,
    pub what: String,
    pub when: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Topic {
    pub title: String,
    pub points: Vec<String>,
}

/// One turn of the transcript: `t` seconds from the start.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    pub t: f64,
    pub who: String,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notes {
    pub title: String,
    pub summary: Vec<String>,
    pub decisions: Vec<String>,
    pub actions: Vec<Action>,
    pub topics: Vec<Topic>,
    pub transcript: Vec<Line>,
    /// The meeting's length, in seconds.
    pub secs: u32,
}

/// The notes in `generateContent`'s answer, or why there are none.
pub fn parse_notes(body: &str) -> Result<Notes, Problem> {
    let v = obj(body).ok_or(Problem::Server)?;
    if v["promptFeedback"]["blockReason"].is_string() {
        return Err(Problem::Blocked);
    }
    let c = &v["candidates"][0];
    match c["finishReason"].as_str().unwrap_or("STOP") {
        "STOP" => {}
        "MAX_TOKENS" => return Err(Problem::TooLong),
        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => return Err(Problem::Blocked),
        _ => return Err(Problem::AiFailed),
    }
    let answer: String = c["content"]["parts"].as_array().map(|a| a.iter().filter_map(|p| p["text"].as_str()).collect()).unwrap_or_default();
    let n = obj(answer.trim()).ok_or(Problem::AiFailed)?;
    let notes = Notes {
        title: Some(text(&n["title"])).filter(|t| !t.is_empty()).unwrap_or_else(|| "Meeting".into()),
        summary: texts(&n["summary"]),
        decisions: texts(&n["decisions"]),
        actions: n["actions"]
            .as_array()
            .map(|a| a.iter().map(|a| Action { who: text(&a["who"]), what: text(&a["what"]), when: text(&a["when"]) }).filter(|a| !a.what.is_empty()).collect())
            .unwrap_or_default(),
        topics: n["topics"]
            .as_array()
            .map(|a| a.iter().map(|t| Topic { title: text(&t["title"]), points: texts(&t["points"]) }).filter(|t| !t.title.is_empty()).collect())
            .unwrap_or_default(),
        transcript: n["transcript"]
            .as_array()
            .map(|a| a.iter().map(|l| Line { t: stamp(&l["t"]), who: text(&l["who"]), text: text(&l["text"]) }).filter(|l| !l.text.is_empty()).collect())
            .unwrap_or_default(),
        secs: 0,
    };
    if notes.transcript.is_empty() && notes.summary.is_empty() {
        return Err(Problem::NothingHeard);
    }
    Ok(notes)
}

// ---------------------------------------------------------------- what went wrong

/// Why nothing could be recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureIssue {
    /// Windows' privacy switch keeps desktop apps from the microphone.
    MicDenied,
    /// Neither the microphone nor the PC's sound would open.
    NoSound,
    Failed,
}

/// What went wrong, as the page says it: never a code or a status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// No Gemini key on this PC yet.
    NoKey,
    /// Gemini didn't take the key (400 `API_KEY_INVALID`, 401, 403).
    KeyRefused,
    /// 429: the key's limit for now (the free tier's per minute or per day),
    /// with the wait when Gemini said.
    Quota(Option<u64>),
    /// 500/503: Gemini is busy.
    Busy,
    /// 404: the model in Settings isn't there (for this key).
    NoModel,
    /// Gemini wouldn't write about it (its safety filters).
    Blocked,
    /// Gemini answered, but not with notes that read.
    AiFailed,
    /// Nobody spoke, as far as Gemini could hear.
    NothingHeard,
    /// The transcript ran past what one answer holds.
    TooLong,
    /// No answer: no internet, or Gemini didn't answer in time.
    Offline,
    /// The app closed on a meeting (a crash, a restart, the PC turned off)
    /// before its notes were written: its recording was kept.
    Unfinished,
    /// Any other status, or an answer that couldn't be read, or the notes
    /// file couldn't be written.
    Server,
    Capture(CaptureIssue),
}

/// The problem a failed call's status and body (Google's
/// `{"error": {"status", "details": [{"reason"}]}}`) stand for.
pub fn problem(status: u16, body: &str, retry_after: Option<u64>) -> Problem {
    let e = obj(body).unwrap_or_default();
    let err = &e["error"];
    let reason = err["details"].as_array().and_then(|d| d.iter().find_map(|x| x["reason"].as_str())).unwrap_or("");
    // A 429's RetryInfo: "retryDelay": "17s".
    let wait = err["details"]
        .as_array()
        .and_then(|d| d.iter().find_map(|x| x["retryDelay"].as_str()))
        .and_then(|s| s.trim_end_matches('s').parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s >= 0.0)
        .map(|s| s.ceil() as u64)
        .or(retry_after);
    match status {
        _ if reason == "API_KEY_INVALID" || reason == "API_KEY_SERVICE_BLOCKED" => Problem::KeyRefused,
        401 | 403 => Problem::KeyRefused,
        404 => Problem::NoModel,
        429 => Problem::Quota(wait),
        500..=599 => Problem::Busy,
        _ => Problem::Server,
    }
}

impl Problem {
    pub fn title(self) -> &'static str {
        match self {
            Problem::NoKey => "Add your Gemini key first",
            Problem::KeyRefused => "Gemini didn't take the key",
            Problem::Quota(_) => "The Gemini key's limit is reached",
            Problem::Busy => "Gemini is busy",
            Problem::NoModel => "That Gemini model isn't there",
            Problem::Blocked => "Gemini wouldn't write these notes",
            Problem::AiFailed | Problem::Server => "The notes couldn't be written",
            Problem::NothingHeard => "Nobody was heard",
            Problem::TooLong => "The meeting is too long for one answer",
            Problem::Unfinished => "Your last meeting isn't written up",
            Problem::Offline => "Can't reach Gemini",
            Problem::Capture(CaptureIssue::MicDenied) => "The microphone is blocked",
            Problem::Capture(CaptureIssue::NoSound) => "No sound to record",
            Problem::Capture(CaptureIssue::Failed) => "Recording stopped",
        }
    }

    /// What to do about it. The recording stays on this PC for Try again
    /// until the notes are saved or it is thrown away.
    pub fn line(self) -> String {
        match self {
            Problem::NoKey => "Make one free at aistudio.google.com, copy it, then paste it in Settings \u{203a} Meeting notes.".into(),
            Problem::KeyRefused => "Check the key in Settings \u{203a} Meeting notes. Your recording is kept.".into(),
            Problem::Quota(Some(s)) if s > 1 && s < 600 => format!("Your recording is kept. Try again in {s} seconds."),
            Problem::Quota(_) => "Your recording is kept. Try again later, or tomorrow on the free tier.".into(),
            Problem::Busy => "Your recording is kept. Try again in a minute.".into(),
            Problem::NoModel => "Pick another model in Settings \u{203a} Meeting notes. Your recording is kept.".into(),
            Problem::Blocked => "Its safety filters stopped it. Your recording is kept.".into(),
            Problem::AiFailed | Problem::Server => "Your recording is kept. Try again.".into(),
            Problem::NothingHeard => "Was the call muted? Check that the bars move next time.".into(),
            Problem::TooLong => "Try the Pro model in Settings, or keep meetings shorter. Your recording is kept.".into(),
            Problem::Unfinished => "Its recording was kept when Hihu closed. Write it up now, or throw it away.".into(),
            Problem::Offline => "Your recording is kept. Check the internet, then try again.".into(),
            Problem::Capture(CaptureIssue::MicDenied) => "Turn on microphone access for desktop apps in Windows Settings.".into(),
            Problem::Capture(CaptureIssue::NoSound) => "Plug in a microphone or speakers, then try again.".into(),
            Problem::Capture(CaptureIssue::Failed) => "A sound device went away. What was heard is kept.".into(),
        }
    }

    /// Try again can help (and the recording is there for it).
    pub fn retryable(self) -> bool {
        !matches!(self, Problem::NothingHeard | Problem::Capture(CaptureIssue::MicDenied) | Problem::Capture(CaptureIssue::NoSound))
    }

    /// The problem is the key or the model: Settings is where to fix it.
    pub fn in_settings(self) -> bool {
        matches!(self, Problem::NoKey | Problem::KeyRefused | Problem::NoModel | Problem::TooLong)
    }
}

// ---------------------------------------------------------------- the page

/// Where the notes stand.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Phase {
    /// Nothing under way: Take notes.
    #[default]
    Idle,
    /// Listening since `since` (mono seconds).
    Listening { since: f64 },
    /// The meeting is over and its audio goes up: `sent` of `parts`.
    Sending { sent: u32, parts: u32 },
    /// Gemini hears it and writes, since `since`.
    Writing { since: f64 },
    /// The notes are saved (`Taker::last`).
    Ready,
    /// Something went wrong; the recording is kept unless thrown away.
    Failed(Problem),
}

/// The notes saved last, for the page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Saved {
    pub notes: Notes,
    /// The Markdown file.
    pub file: String,
    /// When the meeting started (local time).
    pub started: LocalDateTime,
}

/// What the worker reports while it hands a meeting over.
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    Sent { sent: u32, parts: u32 },
    Writing,
    Saved(Box<Saved>),
    Failed(Problem),
}

#[derive(Clone, Debug, Default)]
pub struct Taker {
    pub phase: Phase,
    /// The microphone's and the PC's sound's loudness, 0..1, while listening.
    pub mic: f32,
    pub pc: f32,
    /// Which sources record (one can be missing: no microphone, say).
    pub mic_on: bool,
    pub pc_on: bool,
    /// The recording's length, once stopped.
    pub secs: f64,
    pub last: Option<Saved>,
}

impl Taker {
    pub fn listening(&self) -> bool {
        matches!(self.phase, Phase::Listening { .. })
    }

    /// Something to show on the closed island: listening, or the notes on their way.
    pub fn live(&self) -> bool {
        matches!(self.phase, Phase::Listening { .. } | Phase::Sending { .. } | Phase::Writing { .. })
    }

    /// The recording's time now.
    pub fn elapsed(&self, now: f64) -> f64 {
        match self.phase {
            Phase::Listening { since } => (now - since).max(0.0),
            _ => self.secs,
        }
    }

    /// Seconds until the time on show moves to its next second, while listening.
    pub fn next_second(&self, now: f64) -> Option<f64> {
        self.listening().then(|| 1.0 - self.elapsed(now).fract())
    }

    pub fn started(&mut self, now: f64, mic: bool, pc: bool) {
        self.phase = Phase::Listening { since: now };
        (self.mic_on, self.pc_on, self.mic, self.pc, self.secs) = (mic, pc, 0.0, 0.0, 0.0);
    }

    /// Stopped with `parts` parts of `secs` seconds: on their way up.
    pub fn stopped(&mut self, parts: u32, secs: f64) {
        self.secs = secs;
        self.phase = Phase::Sending { sent: 0, parts };
        (self.mic, self.pc) = (0.0, 0.0);
    }

    pub fn reply(&mut self, r: Reply, now: f64) {
        self.phase = match r {
            Reply::Sent { sent, parts } => Phase::Sending { sent, parts },
            Reply::Writing => match self.phase {
                Phase::Writing { since } => Phase::Writing { since },
                _ => Phase::Writing { since: now },
            },
            Reply::Saved(s) => {
                self.last = Some(*s);
                Phase::Ready
            }
            Reply::Failed(p) => Phase::Failed(p),
        };
    }

    pub fn failed(&mut self, p: Problem) {
        self.phase = Phase::Failed(p);
        (self.mic, self.pc) = (0.0, 0.0);
    }

    /// Back to Take notes (after the notes, or a recording thrown away).
    pub fn reset(&mut self) {
        self.phase = Phase::Idle;
        (self.mic, self.pc, self.secs) = (0.0, 0.0, 0.0);
    }
}

/// "0:07", "12:34", "1:02:03".
pub fn fmt_length(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

// ---------------------------------------------------------------- the file

/// "2026-10-29 14.05 Launch plan.md": sorts by date, and keeps to what
/// Windows allows in a name.
pub fn file_name(started: &LocalDateTime, title: &str) -> String {
    let clean: String = title.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { ' ' } else { c }).collect();
    let mut clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() > 60 {
        clean = clean.chars().take(60).collect::<String>().trim_end().to_string();
    }
    let clean = clean.trim_end_matches('.').trim().to_string();
    let title = if clean.is_empty() { "Meeting".to_string() } else { clean };
    format!("{:04}-{:02}-{:02} {:02}.{:02} {title}.md", started.year, started.month, started.day, started.hour, started.minute)
}

/// The notes as Markdown: what Copy puts on the clipboard and the file holds.
pub fn markdown(n: &Notes, started: &LocalDateTime, with_transcript: bool, lang: Lang) -> String {
    let id = lang == Lang::Id;
    let h = |en: &'static str, ind: &'static str| if id { ind } else { en };
    let mut md = format!("# {}\n\n", n.title);
    let mins = ((n.secs as f64) / 60.0).round().max(1.0) as u32;
    md += &format!(
        "{} {} {} {} · {:02}:{:02} · {mins} min\n",
        WEEKDAY_SHORT[started.weekday() as usize % 7],
        started.day,
        MONTH_SHORT[(started.month.clamp(1, 12) - 1) as usize],
        started.year,
        started.hour,
        started.minute
    );
    let list = |md: &mut String, head: &str, items: &[String]| {
        if !items.is_empty() {
            *md += &format!("\n## {head}\n\n");
            for i in items {
                *md += &format!("- {i}\n");
            }
        }
    };
    list(&mut md, h("Summary", "Ringkasan"), &n.summary);
    list(&mut md, h("Decisions", "Keputusan"), &n.decisions);
    if !n.actions.is_empty() {
        md += &format!("\n## {}\n\n", h("Action items", "Tugas"));
        for a in &n.actions {
            let by = [a.who.as_str(), a.when.as_str()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ");
            md += &if by.is_empty() { format!("- [ ] {}\n", a.what) } else { format!("- [ ] {} ({by})\n", a.what) };
        }
    }
    if !n.topics.is_empty() {
        md += &format!("\n## {}\n", h("Notes", "Catatan"));
        for t in &n.topics {
            md += &format!("\n### {}\n\n", t.title);
            for p in &t.points {
                md += &format!("- {p}\n");
            }
        }
    }
    if with_transcript && !n.transcript.is_empty() {
        md += &format!("\n## {}\n\n", h("Transcript", "Transkrip"));
        for l in &n.transcript {
            md += &format!("**[{}] {}:** {}\n\n", fmt_length(l.t), l.who, l.text);
        }
    }
    md += h("\n---\nNotes by Gemini, in Hihu Island\n", "\n---\nCatatan oleh Gemini, di Hihu Island\n");
    md
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(h: u8, m: u8) -> LocalDateTime {
        LocalDateTime { year: 2026, month: 10, day: 29, hour: h, minute: m, second: 0 }
    }

    #[test]
    fn the_mix_follows_the_clock_and_fills_a_quiet_source_with_silence() {
        let mut m = Mixer::default();
        let mut out = Vec::new();
        m.push_mic(&[100; 4000]);
        // The PC plays nothing: no loopback packets at all.
        m.take(LAG_SAMPLES + 4000, &mut out);
        assert_eq!(out.len(), 4000);
        assert!(out.iter().all(|&s| s == 100));
        // Then both, added together, and a peak clipped rather than wrapped.
        m.push_mic(&[30_000; 10]);
        m.push_pc(&[10_000; 10]);
        out.clear();
        m.take(LAG_SAMPLES + 4010, &mut out);
        assert_eq!(out, vec![i16::MAX; 10]);
        // The clock moved on with nothing from either: silence, not a gap.
        out.clear();
        m.take(LAG_SAMPLES + 4110, &mut out);
        assert_eq!(out, vec![0; 100]);
    }

    #[test]
    fn a_source_running_ahead_loses_its_oldest_samples() {
        let mut m = Mixer::default();
        m.push_pc(&vec![1; MAX_QUEUE]);
        m.push_pc(&[2; 500]);
        let mut out = Vec::new();
        m.take(LAG_SAMPLES + MAX_QUEUE as u64, &mut out);
        assert_eq!(out.len(), MAX_QUEUE);
        assert_eq!(out[..MAX_QUEUE - 500], vec![1; MAX_QUEUE - 500][..]);
        assert_eq!(out[MAX_QUEUE - 500..], [2; 500][..], "the newest are kept");
    }

    #[test]
    fn the_end_of_a_recording_takes_what_still_waits() {
        let mut m = Mixer::default();
        m.push_mic(&[5; 300]);
        m.push_pc(&[7; 100]);
        let mut out = Vec::new();
        m.drain(&mut out);
        assert_eq!(out.len(), 300);
        assert_eq!(out[..100], [12; 100][..]);
        assert_eq!(out[100..], [5; 200][..]);
    }

    #[test]
    fn addresses_and_bodies() {
        assert_eq!(upload_start_url(), "https://generativelanguage.googleapis.com/upload/v1beta/files");
        assert_eq!(file_url("files/abc"), "https://generativelanguage.googleapis.com/v1beta/files/abc");
        assert_eq!(generate_url("gemini-flash-latest"), "https://generativelanguage.googleapis.com/v1beta/models/gemini-flash-latest:generateContent");
        assert_eq!(upload_start_body("part 1"), r#"{"file":{"display_name":"part 1"}}"#);
        let f = File { name: "files/a".into(), uri: "https://x/files/a".into(), state: "ACTIVE".into() };
        let g = File { name: "files/b".into(), uri: "https://x/files/b".into(), state: "ACTIVE".into() };
        let one: Value = serde_json::from_str(&generate_body(std::slice::from_ref(&f), Lang::Id)).unwrap();
        let parts = one["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 2, "the file, then the ask");
        assert_eq!(parts[0]["file_data"]["file_uri"], "https://x/files/a");
        assert!(parts[1]["text"].as_str().unwrap().contains("Indonesian"));
        assert_eq!(one["generationConfig"]["responseMimeType"], "application/json");
        let two: Value = serde_json::from_str(&generate_body(&[f, g], Lang::En)).unwrap();
        let parts = two["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[2]["text"], "Audio file 2 of 2, from 5:00:");
        assert!(parts[4]["text"].as_str().unwrap().contains("2 audio files in order"));
    }

    #[test]
    fn a_busy_model_gives_way_to_flash_lite() {
        assert_eq!(models_to_try("gemini-flash-latest"), ["gemini-flash-latest", "gemini-flash-lite-latest"]);
        assert_eq!(models_to_try("gemini-flash-lite-latest"), ["gemini-flash-lite-latest"]);
    }

    #[test]
    fn uploads_are_read() {
        let up = r#"{"file":{"name":"files/abc","uri":"https://generativelanguage.googleapis.com/v1beta/files/abc","mimeType":"audio/wav","state":"PROCESSING"}}"#;
        assert_eq!(parse_file(up).map(|f| (f.name, f.state)), Some(("files/abc".into(), "PROCESSING".into())));
        let look = r#"{"name":"files/abc","uri":"https://x","state":"ACTIVE"}"#;
        assert_eq!(parse_file(look).map(|f| f.state), Some("ACTIVE".into()));
        assert_eq!(parse_file(r#"{"error":{}}"#), None);
    }

    #[test]
    fn notes_are_read_whole_and_tidy() {
        let notes = r#"{"title":"Launch  plan","summary":["Rilis Jumat",""],"decisions":["Harga 49rb"],
            "actions":[{"who":"Budi","what":"Kirim deck","when":"Kamis"},{"who":"","what":""}],
            "topics":[{"title":"Harga","points":["49rb"]},{"title":"","points":["x"]}],
            "transcript":[{"t":"00:12","who":"Speaker 1","text":"Halo  semua"},{"t":"1:02:03","who":"Budi","text":"oke"},{"t":"x","who":"A","text":""}]}"#;
        let body = json!({ "candidates": [{ "content": { "parts": [{ "text": notes }] }, "finishReason": "STOP" }] }).to_string();
        let n = parse_notes(&body).unwrap();
        assert_eq!(n.title, "Launch plan");
        assert_eq!(n.summary, vec!["Rilis Jumat"]);
        assert_eq!(n.actions, vec![Action { who: "Budi".into(), what: "Kirim deck".into(), when: "Kamis".into() }]);
        assert_eq!(n.topics.len(), 1);
        assert_eq!(n.transcript, vec![Line { t: 12.0, who: "Speaker 1".into(), text: "Halo semua".into() }, Line { t: 3723.0, who: "Budi".into(), text: "oke".into() }]);
        // Nobody spoke; cut short; filtered.
        let empty = json!({ "candidates": [{ "content": { "parts": [{ "text": r#"{"title":"","summary":[],"transcript":[]}"# }] } }] }).to_string();
        assert_eq!(parse_notes(&empty), Err(Problem::NothingHeard));
        let cut = json!({ "candidates": [{ "content": { "parts": [{ "text": "{\"title\":" }] }, "finishReason": "MAX_TOKENS" }] }).to_string();
        assert_eq!(parse_notes(&cut), Err(Problem::TooLong));
        assert_eq!(parse_notes(r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#), Err(Problem::Blocked));
        assert_eq!(parse_notes("<html>"), Err(Problem::Server));
    }

    #[test]
    fn problems_in_plain_words() {
        let bad_key = r#"{"error":{"code":400,"message":"API key not valid.","status":"INVALID_ARGUMENT","details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"API_KEY_INVALID"}]}}"#;
        assert_eq!(problem(400, bad_key, None), Problem::KeyRefused);
        assert_eq!(problem(403, "", None), Problem::KeyRefused);
        let quota = r#"{"error":{"code":429,"status":"RESOURCE_EXHAUSTED","details":[{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"17s"}]}}"#;
        assert_eq!(problem(429, quota, None), Problem::Quota(Some(17)));
        assert_eq!(problem(404, "", None), Problem::NoModel);
        assert_eq!(problem(503, "", None), Problem::Busy);
        assert_eq!(problem(400, "{}", None), Problem::Server);
        assert!(Problem::Busy.retryable() && Problem::Offline.retryable() && Problem::KeyRefused.retryable());
        assert!(!Problem::NothingHeard.retryable());
        assert!(Problem::NoKey.in_settings() && !Problem::Busy.in_settings());
        assert_eq!(Problem::Quota(Some(30)).line(), "Your recording is kept. Try again in 30 seconds.");
    }

    #[test]
    fn the_page_follows_the_meeting() {
        let mut t = Taker::default();
        assert!(!t.live());
        t.started(100.0, true, true);
        assert!(t.listening() && t.live());
        assert_eq!(fmt_length(t.elapsed(165.4)), "1:05");
        assert!((t.next_second(165.4).unwrap() - 0.6).abs() < 1e-9);
        t.stopped(3, 742.0);
        assert_eq!(t.phase, Phase::Sending { sent: 0, parts: 3 });
        assert_eq!(t.next_second(800.0), None);
        t.reply(Reply::Sent { sent: 3, parts: 3 }, 900.0);
        t.reply(Reply::Writing, 901.0);
        t.reply(Reply::Writing, 905.0);
        assert_eq!(t.phase, Phase::Writing { since: 901.0 }, "a second word keeps the first time");
        t.reply(Reply::Saved(Box::new(Saved { file: "x.md".into(), ..Default::default() })), 950.0);
        assert_eq!(t.phase, Phase::Ready);
        assert!(!t.live());
        assert_eq!(t.last.as_ref().unwrap().file, "x.md");
        assert_eq!(fmt_length(3723.0), "1:02:03");
    }

    #[test]
    fn file_names_sort_by_date_and_keep_to_what_windows_allows() {
        assert_eq!(file_name(&at(14, 5), "Launch plan: Q4 / pricing?"), "2026-10-29 14.05 Launch plan Q4 pricing.md");
        assert_eq!(file_name(&at(9, 0), "  "), "2026-10-29 09.00 Meeting.md");
        assert_eq!(file_name(&at(9, 0), "Ends with dots..."), "2026-10-29 09.00 Ends with dots.md");
        assert!(file_name(&at(9, 0), &"x".repeat(200)).chars().count() <= 80);
    }

    #[test]
    fn markdown_has_every_part_that_has_something() {
        let n = Notes {
            title: "Launch plan".into(),
            summary: vec!["Rilis Jumat".into()],
            actions: vec![Action { who: "Budi".into(), what: "Kirim deck".into(), when: "Kamis".into() }, Action { what: "Cek harga".into(), ..Default::default() }],
            topics: vec![Topic { title: "Harga".into(), points: vec!["49rb".into()] }],
            transcript: vec![Line { t: 65.0, who: "Budi".into(), text: "Oke.".into() }],
            secs: 1834,
            ..Default::default()
        };
        let md = markdown(&n, &at(14, 5), true, Lang::En);
        assert!(markdown(&n, &at(14, 5), true, Lang::Id).contains("\n## Ringkasan\n\n- Rilis Jumat\n"));
        assert!(md.starts_with("# Launch plan\n\nThu 29 Oct 2026 · 14:05 · 31 min\n"), "{md}");
        assert!(md.contains("\n## Summary\n\n- Rilis Jumat\n"));
        assert!(!md.contains("## Decisions"), "nothing decided: no heading");
        assert!(md.contains("- [ ] Kirim deck (Budi, Kamis)\n- [ ] Cek harga\n"));
        assert!(md.contains("### Harga\n\n- 49rb\n"));
        assert!(md.contains("**[1:05] Budi:** Oke.\n"));
        assert!(!markdown(&n, &at(14, 5), false, Lang::En).contains("Transcript"));
    }
}
