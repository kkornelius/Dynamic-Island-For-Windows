//! Line-timed lyrics from LRCLIB. Only track metadata is sent, never audio or credentials.
use crate::{
    event::{Bus, Event},
    media::MediaSnapshot,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use windows::{Win32::Networking::WinHttp::*, core::*};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackKey {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub seconds: u64,
}
impl TrackKey {
    pub fn from_media(m: &MediaSnapshot) -> Option<Self> {
        if !m.available || m.title.trim().is_empty() || m.artist.trim().is_empty() {
            return None;
        }
        Some(Self {
            title: m.title.chars().take(256).collect(),
            artist: m.artist.chars().take(256).collect(),
            album: m.album.chars().take(256).collect(),
            seconds: m.duration_ms / 1000,
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LyricLine {
    pub at_ms: u64,
    pub text: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Document {
    pub lines: Vec<LyricLine>,
    pub plain: Vec<String>,
    pub instrumental: bool,
}
impl Document {
    fn bounded(mut self) -> Self {
        let mut budget = 64 * 1024;
        let trim = |text: &mut String, budget: &mut usize| {
            let mut cut = text.len().min(*budget);
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
            *budget -= cut;
        };
        self.lines.truncate(1500);
        self.lines.sort_by_key(|l| l.at_ms);
        if !self.lines.is_empty()
            && self.lines.first().unwrap().at_ms == self.lines.last().unwrap().at_ms
        {
            if self.plain.is_empty() {
                self.plain = self.lines.iter().map(|l| l.text.clone()).collect();
            }
            self.lines.clear();
        }
        for line in &mut self.lines {
            trim(&mut line.text, &mut budget);
        }
        self.plain.truncate(1500);
        for line in &mut self.plain {
            trim(line, &mut budget);
        }
        self
    }
    pub fn active(&self, position: u64) -> Option<usize> {
        self.lines
            .partition_point(|l| l.at_ms <= position)
            .checked_sub(1)
    }
    pub fn next_delay(&self, position: u64) -> Option<u64> {
        self.lines
            .iter()
            .find(|l| l.at_ms > position)
            .map(|l| l.at_ms - position)
    }
}
#[derive(Clone, Debug)]
pub enum Status {
    Empty,
    Loading,
    Ready(Document),
    Unavailable,
    Offline,
    RateLimited,
    NetworkError,
}
pub struct LyricsState {
    pub key: Option<TrackKey>,
    pub status: Status,
    pub scroll: usize,
}
impl Default for LyricsState {
    fn default() -> Self {
        Self {
            key: None,
            status: Status::Empty,
            scroll: 0,
        }
    }
}
impl LyricsState {
    pub fn document(&self) -> Option<&Document> {
        if let Status::Ready(d) = &self.status {
            Some(d)
        } else {
            None
        }
    }
    pub fn position(&self, media: &MediaSnapshot, now: u64, offset: i32) -> u64 {
        media.position(now).saturating_add_signed(offset as i64)
    }
    pub fn current_line(&self, media: &MediaSnapshot, now: u64, offset: i32) -> Option<&str> {
        if !media.timeline_available {
            return None;
        }
        let d = self.document()?;
        let index = d.active(self.position(media, now, offset))?;
        Some(d.lines[index].text.as_str())
    }
}

/// Parse standard LRC, multiple stamps per line, millisecond precision and global offsets.
pub fn parse_lrc(text: &str) -> Vec<LyricLine> {
    let offset = text
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("[offset:")?
                .strip_suffix(']')?
                .trim()
                .parse::<i64>()
                .ok()
        })
        .unwrap_or(0);
    let mut lines = Vec::new();
    for line in text.lines().take(2500) {
        let mut rest = line.trim();
        let mut stamps = Vec::new();
        while let Some(tag) = rest.strip_prefix('[') {
            let Some(end) = tag.find(']') else {
                break;
            };
            let token = &tag[..end];
            let Some((minutes, seconds)) = token.split_once(':') else {
                break;
            };
            let Ok(minutes) = minutes.parse::<u64>() else {
                break;
            };
            let Ok(seconds) = seconds.parse::<f64>() else {
                break;
            };
            if !seconds.is_finite() || !(0.0..60.0).contains(&seconds) || minutes > 1440 {
                break;
            }
            stamps.push(
                (minutes * 60_000 + (seconds * 1000.0).round() as u64)
                    .saturating_add_signed(offset),
            );
            rest = tag[end + 1..].trim_start();
        }
        let clean: String = rest.chars().filter(|c| !c.is_control()).take(600).collect();
        for at_ms in stamps {
            lines.push(LyricLine {
                at_ms,
                text: clean.trim().to_owned(),
            });
        }
    }
    lines.sort_by_key(|l| l.at_ms);
    lines.dedup_by(|a, b| a.at_ms == b.at_ms && a.text == b.text);
    lines.truncate(2500);
    lines
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    track_name: String,
    artist_name: String,
    #[serde(default)]
    album_name: String,
    duration: f64,
    #[serde(default)]
    instrumental: bool,
    synced_lyrics: Option<String>,
    plain_lyrics: Option<String>,
}
// Remove only explicit collaboration-credit groups, retaining live/remix/version tags.
fn without_feature(s: &str) -> String {
    let mut clean = String::new();
    let mut rest = s;
    while let Some(start) = rest.find(['(', '[']) {
        clean.push_str(&rest[..start]);
        let close = if rest.as_bytes()[start] == b'(' {
            ')'
        } else {
            ']'
        };
        let Some(end) = rest[start..].find(close).map(|i| start + i) else {
            clean.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let group = rest[start + 1..end].trim().to_ascii_lowercase();
        if !["feat.", "feat ", "ft.", "ft ", "featuring ", "with "]
            .iter()
            .any(|prefix| group.starts_with(prefix))
        {
            clean.push_str(&rest[start..=end]);
        }
        rest = &rest[end + 1..];
    }
    clean.push_str(rest);
    let lower = clean.to_ascii_lowercase();
    if let Some(i) = [" feat. ", " ft. ", " featuring "]
        .iter()
        .filter_map(|marker| lower.find(marker))
        .min()
    {
        clean.truncate(i);
    }
    clean.trim().to_owned()
}
fn primary_artist(s: &str) -> String {
    let clean = without_feature(s);
    let end = clean
        .to_ascii_lowercase()
        .find(" with ")
        .unwrap_or(clean.len());
    let clean = &clean[..end];
    clean
        .split([',', ';'])
        .next()
        .unwrap_or(clean)
        .trim()
        .to_owned()
}
fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
impl Record {
    fn match_score(&self, key: &TrackKey) -> Option<i32> {
        if !self.duration.is_finite()
            || key.seconds == 0
            || (self.duration - key.seconds as f64).abs() > 3.0
        {
            return None;
        }
        if normalize(&without_feature(&self.track_name)) != normalize(&without_feature(&key.title))
            || normalize(&primary_artist(&self.artist_name))
                != normalize(&primary_artist(&key.artist))
        {
            return None;
        }
        if !key.album.is_empty()
            && normalize(&without_feature(&self.album_name))
                != normalize(&without_feature(&key.album))
        {
            return None;
        }
        Some(
            1000 + if normalize(&self.track_name) == normalize(&key.title) {
                100
            } else {
                0
            } + if normalize(&self.artist_name) == normalize(&key.artist) {
                50
            } else {
                0
            } - ((self.duration - key.seconds as f64).abs() * 10.0) as i32,
        )
    }
    fn timed_match_score(&self, key: &TrackKey) -> Option<i32> {
        let score = self.match_score(key)?;
        let lines = self
            .synced_lyrics
            .as_deref()
            .map(parse_lrc)
            .unwrap_or_default();
        // A single stamp or a constant timestamp is not useful synchronized lyrics.
        if lines.len() < 2
            || lines.last()?.at_ms <= lines.first()?.at_ms
            || lines.last()?.at_ms > key.seconds.saturating_add(5) * 1000
        {
            return None;
        }
        Some(score)
    }
    fn matches(&self, key: &TrackKey) -> bool {
        self.match_score(key).is_some()
    }
    fn document(self) -> Document {
        Document {
            lines: self
                .synced_lyrics
                .as_deref()
                .map(parse_lrc)
                .unwrap_or_default(),
            plain: self
                .plain_lyrics
                .unwrap_or_default()
                .lines()
                .take(2500)
                .map(|l| l.chars().filter(|c| !c.is_control()).take(600).collect())
                .collect(),
            instrumental: self.instrumental,
        }
        .bounded()
    }
}
pub fn encode(value: &str) -> String {
    let mut s = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            s.push(b as char);
        } else {
            use std::fmt::Write;
            let _ = write!(s, "%{b:02X}");
        }
    }
    s
}
pub fn signature_path(key: &TrackKey) -> String {
    let mut path = format!(
        "/api/get?track_name={}&artist_name={}",
        encode(&key.title),
        encode(&key.artist)
    );
    if !key.album.is_empty() {
        path.push_str(&format!("&album_name={}", encode(&key.album)));
    }
    if (1..=3600).contains(&key.seconds) {
        path.push_str(&format!("&duration={}", key.seconds));
    }
    path
}

struct Internet(*mut std::ffi::c_void);
impl Internet {
    fn checked(h: *mut std::ffi::c_void) -> Result<Self> {
        if h.is_null() {
            Err(Error::from_thread())
        } else {
            Ok(Self(h))
        }
    }
}
impl Drop for Internet {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}
struct Response {
    status: u32,
    body: Vec<u8>,
    retry_after: u64,
}
fn get(path: &str, stop: &AtomicBool) -> Result<Response> {
    unsafe {
        let session = Internet::checked(WinHttpOpen(
            w!("DynamicIslandForWindows/0.6.0 (Windows desktop lyrics; https://lrclib.net/docs)"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            None,
            None,
            0,
        ))?;
        WinHttpSetTimeouts(session.0, 1000, 1800, 1200, 1800)?;
        let connection = Internet::checked(WinHttpConnect(session.0, w!("lrclib.net"), 443, 0))?;
        let request = Internet::checked(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            &HSTRING::from(path),
            None,
            None,
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_ne_bytes()),
        )?;
        if stop.load(Ordering::Relaxed) {
            return Err(Error::empty());
        }
        WinHttpSendRequest(request.0, None, None, 0, 0, 0)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())?;
        let mut status = 0u32;
        let mut len = 4;
        let mut index = 0;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            None,
            Some((&mut status as *mut u32).cast()),
            &mut len,
            &mut index,
        )?;
        let mut retry = [0u16; 128];
        let mut len = 256;
        let mut index = 0;
        let retry_after = if WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_RETRY_AFTER,
            None,
            Some(retry.as_mut_ptr().cast()),
            &mut len,
            &mut index,
        )
        .is_ok()
        {
            String::from_utf16_lossy(
                &retry[..retry.iter().position(|c| *c == 0).unwrap_or(retry.len())],
            )
            .trim()
            .parse()
            .unwrap_or(3600)
        } else {
            60
        };
        let mut body = Vec::new();
        let mut buffer = [0u8; 8192];
        while status == 200 && !stop.load(Ordering::Relaxed) {
            let mut read = 0;
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )?;
            if read == 0 {
                break;
            }
            if body.len() + read as usize > 1_048_576 {
                return Err(Error::new(
                    HRESULT(0x80070057u32 as i32),
                    "Lyrics response too large",
                ));
            }
            body.extend_from_slice(&buffer[..read as usize]);
        }
        Ok(Response {
            status,
            body,
            retry_after,
        })
    }
}
#[derive(Serialize, Deserialize)]
struct Cached {
    key: TrackKey,
    document: Option<Document>,
    saved: u64,
    #[serde(default)]
    resolver_version: u8,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Cache {
    entries: Vec<Cached>,
    blocked_until: u64,
}
fn seconds_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
impl Cache {
    fn fresh(&self, key: &TrackKey) -> bool {
        self.entries
            .iter()
            .rev()
            .find(|e| &e.key == key)
            .is_some_and(|e| {
                let age = seconds_now().saturating_sub(e.saved);
                match &e.document {
                    Some(d) if !d.lines.is_empty() || d.instrumental => age < 7 * 86400,
                    Some(_) => e.resolver_version >= 3 && age < 6 * 3600,
                    None => e.resolver_version >= 3 && age < 3600,
                }
            })
    }
    fn load(path: &Path) -> Self {
        if std::fs::metadata(path)
            .map(|m| m.len() > 16 * 1024 * 1024)
            .unwrap_or(false)
        {
            return Self::default();
        }
        let mut cache: Self = std::fs::read(path)
            .ok()
            .filter(|b| b.len() < 16 * 1024 * 1024)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if cache.entries.len() > 40 {
            cache.entries.drain(..cache.entries.len() - 40);
        }
        for entry in &mut cache.entries {
            if let Some(document) = entry.document.take() {
                entry.document = Some(document.bounded());
            }
        }
        cache
    }
    fn lookup(&self, key: &TrackKey) -> Option<Status> {
        self.entries
            .iter()
            .rev()
            .find(|e| {
                &e.key == key
                    && (e.document.is_some() || seconds_now().saturating_sub(e.saved) < 3600)
            })
            .map(|e| {
                e.document
                    .clone()
                    .map(Status::Ready)
                    .unwrap_or(Status::Unavailable)
            })
    }
    fn store(&mut self, key: TrackKey, document: Option<Document>) {
        self.entries.retain(|e| e.key != key);
        self.entries.push(Cached {
            key,
            document,
            saved: seconds_now(),
            resolver_version: 3,
        });
        if self.entries.len() > 40 {
            self.entries.remove(0);
        }
    }
    fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec(self) {
            let temp = path.with_extension("tmp");
            if std::fs::write(&temp, bytes).is_ok() {
                unsafe {
                    use windows::Win32::Storage::FileSystem::*;
                    let _ = MoveFileExW(
                        &HSTRING::from(temp.as_os_str()),
                        &HSTRING::from(path.as_os_str()),
                        MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                    );
                }
            }
        }
    }
}
pub enum Request {
    Lookup {
        key: TrackKey,
        online: bool,
        force: bool,
    },
    Stop,
}
pub struct LyricsWorker {
    pub tx: mpsc::Sender<Request>,
    join: Option<JoinHandle<()>>,
}
impl LyricsWorker {
    pub fn start(bus: Bus, stop: Arc<AtomicBool>, directory: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let path = directory.join("lyrics-cache.json");
            let mut cache = Cache::load(&path);
            let mut last_request = std::time::Instant::now() - Duration::from_secs(1);
            while let Ok(request) = rx.recv() {
                if stop.load(Ordering::Relaxed) || matches!(request, Request::Stop) {
                    break;
                }
                let mut request = request;
                while let Ok(new) = rx.try_recv() {
                    request = new;
                }
                let Request::Lookup { key, online, force } = request else {
                    break;
                };
                let cached = cache.lookup(&key);
                if !force
                    && cache.fresh(&key)
                    && let Some(status) = cached.clone()
                {
                    bus.send(Event::Lyrics(key, status));
                    continue;
                }
                if !online {
                    bus.send(Event::Lyrics(key, cached.unwrap_or(Status::Offline)));
                    continue;
                }
                if let Some(Status::Ready(_)) = &cached {
                    bus.send(Event::Lyrics(key.clone(), cached.clone().unwrap()));
                }
                if seconds_now() < cache.blocked_until {
                    bus.send(Event::Lyrics(key, cached.unwrap_or(Status::RateLimited)));
                    continue;
                }
                let status = resolve(&key, &stop, &mut last_request, &mut cache.blocked_until);
                match &status {
                    Status::Ready(d) => cache.store(key.clone(), Some(d.clone())),
                    Status::Unavailable => cache.store(key.clone(), None),
                    _ => {}
                }
                let status = if matches!(status, Status::NetworkError | Status::RateLimited) {
                    cached.unwrap_or(status)
                } else {
                    status
                };
                cache.save(&path);
                if !stop.load(Ordering::Relaxed) {
                    bus.send(Event::Lyrics(key, status));
                }
            }
        });
        Self {
            tx,
            join: Some(join),
        }
    }
    pub fn shutdown(&mut self) {
        let _ = self.tx.send(Request::Stop);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn request(
    path: &str,
    stop: &AtomicBool,
    last: &mut std::time::Instant,
    blocked: &mut u64,
) -> Result<Response> {
    if seconds_now() < *blocked || stop.load(Ordering::Relaxed) {
        return Err(Error::empty());
    }
    while last.elapsed() < Duration::from_millis(350) && !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(25));
    }
    *last = std::time::Instant::now();
    let response = get(path, stop)?;
    if matches!(response.status, 429 | 503) {
        let delay = response.retry_after.max(1);
        *blocked = seconds_now()
            .saturating_add(delay)
            .saturating_add(delay / 10 + 1);
    }
    Ok(response)
}
#[cfg(test)]
fn choose_timed(records: Vec<Record>, key: &TrackKey) -> Option<Record> {
    records
        .into_iter()
        .take(100)
        .filter_map(|r| r.timed_match_score(key).map(|score| (score, r)))
        .max_by_key(|(score, _)| *score)
        .map(|(_, r)| r)
}
fn resolve(
    key: &TrackKey,
    stop: &AtomicBool,
    last: &mut std::time::Instant,
    blocked: &mut u64,
) -> Status {
    let mut fallback = None;
    match request(&signature_path(key), stop, last, blocked) {
        Ok(r) if r.status == 200 => {
            if let Ok(record) = serde_json::from_slice::<Record>(&r.body)
                && record.matches(key)
            {
                let d = record.document();
                if d.instrumental || !d.lines.is_empty() {
                    return Status::Ready(d);
                }
                if !d.plain.is_empty() {
                    fallback = Some(d);
                }
            }
        }
        Ok(r) if matches!(r.status, 429 | 503) => return Status::RateLimited,
        Ok(r) if r.status == 404 => {}
        _ => return Status::NetworkError,
    }
    let path = format!(
        "/api/search?track_name={}&artist_name={}",
        encode(&without_feature(&key.title)),
        encode(&primary_artist(&key.artist))
    );
    match request(&path, stop, last, blocked) {
        Ok(r) if r.status == 200 => {
            if let Ok(records) = serde_json::from_slice::<Vec<Record>>(&r.body) {
                let selected = records
                    .iter()
                    .filter_map(|r| r.timed_match_score(key).map(|score| (score, r)))
                    .max_by_key(|(score, _)| *score);
                if let Some((_, r)) = selected {
                    return Status::Ready(r.clone().document());
                }
                if fallback.is_none() {
                    fallback = records
                        .into_iter()
                        .filter_map(|r| {
                            r.match_score(key)
                                .filter(|_| {
                                    r.plain_lyrics
                                        .as_ref()
                                        .is_some_and(|text| !text.trim().is_empty())
                                })
                                .map(|score| (score, r))
                        })
                        .max_by_key(|(score, _)| *score)
                        .map(|(_, r)| r.document());
                }
            }
        }
        Ok(r) if matches!(r.status, 429 | 503) => {
            return fallback.map(Status::Ready).unwrap_or(Status::RateLimited);
        }
        _ => return fallback.map(Status::Ready).unwrap_or(Status::NetworkError),
    }
    fallback.map(Status::Ready).unwrap_or(Status::Unavailable)
}
/// Resolves only the track explicitly shown in the reported screenshot. No playback read.
pub fn probe_reported_track() -> Result<()> {
    let key = TrackKey {
        title: "Wichita Lineman (feat. Nick Cave)".into(),
        artist: "Flea".into(),
        album: "Honora".into(),
        seconds: 262,
    };
    let status = resolve(
        &key,
        &AtomicBool::new(false),
        &mut (std::time::Instant::now() - Duration::from_secs(1)),
        &mut 0,
    );
    let report = if let Status::Ready(d) = status {
        let advancing = d
            .lines
            .iter()
            .find(|l| l.at_ms > 0)
            .is_some_and(|line| d.active(line.at_ms.saturating_sub(1)) != d.active(line.at_ms));
        serde_json::json!({"fixture":"User-reported track","playback_read":false,"synced_lines":d.lines.len(),"plain_lines":d.plain.len(),"timestamp_boundaries_advance":advancing,"first_stamp_ms":d.lines.first().map(|l|l.at_ms),"last_stamp_ms":d.lines.last().map(|l|l.at_ms)})
    } else {
        serde_json::json!({"resolved":false})
    };
    std::fs::create_dir_all("artifacts").ok();
    std::fs::write(
        "artifacts/lyrics-reported-probe.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| Error::new(HRESULT(0x80004005u32 as i32), e.to_string()))?;
    Ok(())
}
pub fn probe() -> Result<()> {
    let media = super::sessions::probe_snapshot()?;
    let Some(key) = TrackKey::from_media(&media) else {
        return Err(Error::new(
            HRESULT(0x80004005u32 as i32),
            "No song metadata available",
        ));
    };
    let response = get(&signature_path(&key), &AtomicBool::new(false))?;
    let parsed = serde_json::from_slice::<Record>(&response.body).ok();
    let matched = parsed.as_ref().is_some_and(|r| r.matches(&key));
    let doc = parsed.map(Record::document).unwrap_or_default();
    let report = serde_json::json!({"http_status":response.status,"metadata_match":matched,"synced_lines":doc.lines.len(),"plain_lines":doc.plain.len(),"instrumental":doc.instrumental,"timeline_available":media.timeline_available,"playing":media.playing,"position_ms":media.position(crate::platform::now_ms())});
    let store = crate::storage::Store::new();
    let _ = std::fs::create_dir_all(&store.directory);
    std::fs::write(
        store.directory.join("lyrics-probe.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| Error::new(HRESULT(0x80004005u32 as i32), e.to_string()))?;
    Ok(())
}
/// Resolve the user's reported collaboration-credit variant without reading playback.
pub fn probe_go_easy_track() -> Result<()> {
    let key = TrackKey {
        title: "Go Easy, Kid (with James Blake)".into(),
        artist: "Monica Martin".into(),
        album: "Go Easy, Kid (with James Blake)".into(),
        seconds: 267,
    };
    let status = resolve(
        &key,
        &AtomicBool::new(false),
        &mut (std::time::Instant::now() - Duration::from_secs(1)),
        &mut 0,
    );
    let report = if let Status::Ready(d) = status {
        serde_json::json!({"playback_read":false,"synced_lines":d.lines.len(),
            "first_stamp_ms":d.lines.first().map(|l|l.at_ms),
            "last_stamp_ms":d.lines.last().map(|l|l.at_ms),
            "timestamp_boundaries_advance":d.lines.iter().filter(|l|l.at_ms>0)
                .all(|l|d.active(l.at_ms.saturating_sub(1)) != d.active(l.at_ms))})
    } else {
        serde_json::json!({"resolved":false})
    };
    std::fs::create_dir_all("artifacts").ok();
    std::fs::write(
        "artifacts/lyrics-go-easy-probe.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| Error::new(HRESULT(0x80004005u32 as i32), e.to_string()))?;
    Ok(())
}
/// Uses only the fixed, public example from LRCLIB's own documentation.
/// It does not inspect Windows media sessions or read the user's song metadata.
pub fn probe_public_example() -> Result<()> {
    let key = TrackKey {
        title: "I Want to Live".into(),
        artist: "Borislav Slavov".into(),
        album: "Baldur's Gate 3 (Original Game Soundtrack)".into(),
        seconds: 233,
    };
    let response = get(&signature_path(&key), &AtomicBool::new(false))?;
    let parsed = serde_json::from_slice::<Record>(&response.body).ok();
    let matched = parsed.as_ref().is_some_and(|r| r.matches(&key));
    let document = parsed.map(Record::document).unwrap_or_default();
    let report = serde_json::json!({"fixture":"LRCLIB documentation example","user_playback_read":false,"http_status":response.status,"metadata_match":matched,"synced_lines":document.lines.len(),"plain_lines":document.plain.len()});
    let _ = std::fs::create_dir_all("artifacts");
    std::fs::write(
        "artifacts/lyrics-public-probe.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| Error::new(HRESULT(0x80004005u32 as i32), e.to_string()))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn record(
        title: &str,
        artist: &str,
        album: &str,
        seconds: f64,
        synced: Option<&str>,
    ) -> Record {
        serde_json::from_value(serde_json::json!({"trackName":title,"artistName":artist,"albumName":album,"duration":seconds,"syncedLyrics":synced,"plainLyrics":"Original test text"})).unwrap()
    }
    fn featured_key() -> TrackKey {
        TrackKey {
            title: "Original song (feat. Friend)".into(),
            artist: "Composer".into(),
            album: "Example album".into(),
            seconds: 200,
        }
    }
    #[test]
    fn finds_timed_feature_variant_but_preserves_recording_identity() {
        let key = featured_key();
        let lrc = Some("[00:02.00]Original test text\n[00:08.00]Another test line");
        let selected = choose_timed(
            vec![
                record(&key.title, "Composer", &key.album, 200., None),
                record("Original song", "Composer", &key.album, 201., lrc),
                record("Original song (live)", "Composer", &key.album, 200., lrc),
            ],
            &key,
        )
        .unwrap();
        assert_eq!(selected.track_name, "Original song");
        assert_eq!(selected.document().lines.len(), 2);
        assert!(
            record("Original song", "Another artist", &key.album, 200., lrc)
                .timed_match_score(&key)
                .is_none()
        );
        assert!(
            record("Original song", "Composer", "Different album", 200., lrc)
                .timed_match_score(&key)
                .is_none()
        );
        assert!(
            record("Original song", "Composer", &key.album, 240., lrc)
                .timed_match_score(&key)
                .is_none()
        );
    }
    #[test]
    fn collaboration_credit_in_title_and_single_album_finds_the_matching_duration() {
        let key = TrackKey {
            title: "Go Easy, Kid (with James Blake)".into(),
            artist: "Monica Martin".into(),
            album: "Go Easy, Kid (with James Blake)".into(),
            seconds: 267,
        };
        let lrc = Some("[00:02]Fixture line one\n[00:09]Fixture line two");
        let selected = choose_timed(
            vec![
                record("Go Easy, Kid", "Monica Martin", "Go Easy, Kid", 248.0, lrc),
                record("Go Easy, Kid", "Monica Martin", "Go Easy, Kid", 267.99, lrc),
                record(
                    "Go Easy, Kid (Live)",
                    "Monica Martin",
                    "Go Easy, Kid",
                    267.0,
                    lrc,
                ),
            ],
            &key,
        )
        .unwrap();
        assert_eq!(selected.duration, 267.99);
        assert!(
            record(
                "Go Easy, Kid",
                "Monica Martin; James Blake",
                &key.album,
                267.0,
                None
            )
            .matches(&key)
        );
        assert!(!record("Go Easy, Kid", "Another artist", &key.album, 267.0, lrc).matches(&key));
        assert_eq!(without_feature("Song (with Friend) (Live)"), "Song  (Live)");
        assert_eq!(
            without_feature("Dancing with Myself"),
            "Dancing with Myself"
        );
    }
    #[test]
    fn previous_negative_cache_retries_with_new_resolver() {
        let key = featured_key();
        let mut cache = Cache::default();
        cache.entries.push(Cached {
            key: key.clone(),
            document: None,
            saved: seconds_now(),
            resolver_version: 2,
        });
        assert!(!cache.fresh(&key));
        cache.store(key.clone(), None);
        assert!(cache.fresh(&key));
    }
    #[test]
    fn rejects_unusable_timestamps_and_keeps_live_remix_versions_distinct() {
        let key = featured_key();
        assert!(
            record(
                "Original song",
                "Composer",
                &key.album,
                200.,
                Some("[00:00.00]One\n[00:00.00]Two")
            )
            .timed_match_score(&key)
            .is_none()
        );
        assert_eq!(
            without_feature("Original song (Live)"),
            "Original song (Live)"
        );
        assert_eq!(
            without_feature("Original song (Remix)"),
            "Original song (Remix)"
        );
        assert_eq!(
            without_feature("Original song (feat. Friend)"),
            "Original song"
        );
    }
    #[test]
    fn legacy_plain_cache_is_readable_offline_but_upgraded_online() {
        let key = featured_key();
        let mut c = Cache::default();
        c.entries.push(Cached {
            key: key.clone(),
            document: Some(Document {
                plain: vec!["Original text".into()],
                ..Default::default()
            }),
            saved: seconds_now(),
            resolver_version: 0,
        });
        assert!(matches!(c.lookup(&key), Some(Status::Ready(_))));
        assert!(!c.fresh(&key));
        c.store(
            key.clone(),
            Some(Document {
                plain: vec!["Original text".into()],
                ..Default::default()
            }),
        );
        assert!(c.fresh(&key));
    }
    #[test]
    fn lrc_offsets_multiple_stamps_and_empty_breaks() {
        let lines = parse_lrc(
            "[ar:Example]\n[offset:-250]\n[00:01.50][00:03.125]Original test words\n[00:05.00]\n[bogus]ignore",
        );
        assert_eq!(
            lines.iter().map(|l| l.at_ms).collect::<Vec<_>>(),
            [1250, 2875, 4750]
        );
        assert_eq!(lines[2].text, "");
        let d = Document {
            lines,
            ..Default::default()
        };
        assert_eq!(d.active(1249), None);
        assert_eq!(d.active(2875), Some(1));
        assert_eq!(d.next_delay(2876), Some(1874));
    }
    #[test]
    fn invalid_time_is_ignored() {
        assert!(parse_lrc("[00:NaN]oops\n[00:61]oops\n[999999:00]oops").is_empty());
        let d = record(
            "Original",
            "Artist",
            "Album",
            200.,
            Some("[00:00.00]One\n[00:00.00]Two"),
        )
        .document();
        assert!(d.lines.is_empty());
        assert!(!d.plain.is_empty());
    }
    #[test]
    fn query_encoding_cannot_change_host_or_parameters() {
        assert_eq!(encode("a&b #é"), "a%26b%20%23%C3%A9");
    }
    #[test]
    fn timeline_freezes_when_paused_and_caps_at_duration() {
        let mut m = MediaSnapshot {
            playing: true,
            timeline_available: true,
            playback_rate: 1.0,
            position_ms: 2000,
            sampled_at_ms: 100,
            duration_ms: 5000,
            ..Default::default()
        };
        assert_eq!(m.position(1100), 3000);
        m.playing = false;
        assert_eq!(m.position(10000), 2000);
        m.playing = true;
        assert_eq!(m.position(10000), 5000);
    }
    #[test]
    fn stale_track_keys_are_distinct() {
        let mut a = MediaSnapshot {
            available: true,
            title: "Example".into(),
            artist: "Artist".into(),
            ..Default::default()
        };
        let first = TrackKey::from_media(&a);
        a.title = "Second track".into();
        assert_ne!(first, TrackKey::from_media(&a));
    }
}
