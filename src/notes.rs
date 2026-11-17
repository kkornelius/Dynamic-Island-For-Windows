//! A bounded local scratchpad. File I/O runs on a sleeping, lazily started writer.
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};

pub const LIMIT: usize = 20_000;
pub const FILE: &str = "quick-note.txt";
pub const BODY: [i32; 4] = [28, 118, 400, 262];
pub const TEXT: [i32; 4] = [44, 134, 364, 230];

#[derive(Clone, Default)]
struct Snapshot {
    text: String,
    cursor: usize,
    anchor: usize,
}
#[derive(Default)]
pub struct Editor {
    pub text: String,
    pub cursor: usize,
    pub anchor: usize,
    pub scroll: f32,
    pub revision: u64,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}
impl Editor {
    pub fn selection(&self) -> std::ops::Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
        }
    }
    pub fn select(&mut self, position: usize, extend: bool) {
        self.cursor = position.min(self.text.len());
        while !self.text.is_char_boundary(self.cursor) {
            self.cursor -= 1;
        }
        if !extend {
            self.anchor = self.cursor;
        }
    }
    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
    }
    pub fn insert(&mut self, text: &str) {
        let r = self.selection();
        let room =
            LIMIT.saturating_sub(self.text.chars().count() - self.text[r.clone()].chars().count());
        let clean: String = text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .chars()
            .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
            .take(room)
            .collect();
        if self.text[r.clone()] == clean {
            return;
        }
        self.undo.push(self.snapshot());
        if self.undo.len() > 32 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.text.replace_range(r.clone(), &clean);
        self.cursor = r.start + clean.len();
        self.anchor = self.cursor;
        self.revision += 1;
    }
    pub fn delete(&mut self, backwards: bool) {
        let original = (self.cursor, self.anchor, self.revision);
        if self.selection().is_empty() {
            let p = if backwards {
                self.text[..self.cursor]
                    .char_indices()
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            } else {
                self.text[self.cursor..]
                    .chars()
                    .next()
                    .map(|c| self.cursor + c.len_utf8())
                    .unwrap_or(self.cursor)
            };
            self.anchor = p;
        }
        self.insert("");
        if self.revision != original.2
            && let Some(previous) = self.undo.last_mut()
        {
            previous.cursor = original.0;
            previous.anchor = original.1;
        }
    }
    pub fn horizontal(&mut self, right: bool, extend: bool, word: bool) {
        if !extend && !word && !self.selection().is_empty() {
            let r = self.selection();
            self.select(if right { r.end } else { r.start }, false);
            return;
        }
        let mut p = self.cursor;
        if word {
            if right {
                while let Some(c) = self.text[p..].chars().next() {
                    if c.is_whitespace() {
                        break;
                    }
                    p += c.len_utf8();
                }
                while let Some(c) = self.text[p..].chars().next() {
                    if !c.is_whitespace() {
                        break;
                    }
                    p += c.len_utf8();
                }
            } else {
                while let Some(c) = self.text[..p].chars().next_back() {
                    if !c.is_whitespace() {
                        break;
                    }
                    p -= c.len_utf8();
                }
                while let Some(c) = self.text[..p].chars().next_back() {
                    if c.is_whitespace() {
                        break;
                    }
                    p -= c.len_utf8();
                }
            }
            self.select(p, extend);
            return;
        }
        loop {
            let next = if right {
                self.text[p..].chars().next().map(|c| p + c.len_utf8())
            } else {
                self.text[..p].char_indices().next_back().map(|(i, _)| i)
            };
            let Some(next) = next else { break };
            p = next;
            if !word
                || p == 0
                || p == self.text.len()
                || self.text[p..].starts_with(char::is_whitespace)
            {
                break;
            }
        }
        self.select(p, extend);
    }
    pub fn undo(&mut self, redo: bool) {
        let entry = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        if let Some(s) = entry {
            let old = self.snapshot();
            if redo {
                self.undo.push(old)
            } else {
                self.redo.push(old)
            }
            self.text = s.text;
            self.cursor = s.cursor;
            self.anchor = s.anchor;
            self.revision += 1;
        }
    }
    pub fn utf16(&self, byte: usize) -> u32 {
        self.text[..byte].encode_utf16().count() as u32
    }
    pub fn byte(&self, utf16: u32) -> usize {
        let mut units = 0;
        for (i, c) in self.text.char_indices() {
            if units >= utf16 {
                return i;
            }
            units += c.len_utf16() as u32;
        }
        self.text.len()
    }
}

pub struct Note {
    pub editor: Editor,
    pub readable: bool,
    pub saved: u64,
    pub queued: u64,
    pub due: Option<u64>,
    pub error: bool,
    pub copied_until: u64,
    pub caret_at: u64,
}
impl Note {
    pub fn load(directory: &Path) -> Self {
        let text = (|| -> io::Result<String> {
            let path = directory.join(FILE);
            if fs::metadata(&path)?.len() > (LIMIT * 4) as u64 {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let text = fs::read_to_string(path)?;
            if text.chars().count() > LIMIT {
                return Err(io::ErrorKind::InvalidData.into());
            }
            Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
        })();
        let readable = text.as_ref().is_ok_and(|_| true)
            || text
                .as_ref()
                .is_err_and(|e| e.kind() == io::ErrorKind::NotFound);
        let mut editor = Editor {
            text: text.unwrap_or_default(),
            ..Default::default()
        };
        editor.cursor = editor.text.len();
        editor.anchor = editor.cursor;
        Self {
            editor,
            readable,
            saved: 0,
            queued: 0,
            due: None,
            error: !readable,
            copied_until: 0,
            caret_at: 0,
        }
    }
    pub fn changed(&mut self, previous: u64, now: u64) {
        self.caret_at = now;
        if self.editor.revision != previous {
            self.due = Some(now + 600);
            self.error = false;
        }
    }
    pub fn completed(&mut self, revision: u64, success: bool, now: u64) {
        if success {
            self.saved = self.saved.max(revision);
        }
        if revision == self.editor.revision {
            self.error = !success;
            self.due = if success { None } else { Some(now + 5_000) };
            if !success {
                self.queued = self.saved;
            }
        }
    }
}

type Pending = (Mutex<(Option<(u64, String)>, bool)>, Condvar);
pub struct Writer {
    pending: Arc<Pending>,
    join: Option<JoinHandle<()>>,
}
impl Writer {
    pub fn new(directory: PathBuf, bus: crate::event::Bus) -> io::Result<Self> {
        let pending: Arc<Pending> = Arc::new((Mutex::new((None, false)), Condvar::new()));
        let shared = Arc::clone(&pending);
        let join = std::thread::Builder::new()
            .name("island-note-save".into())
            .spawn(move || {
                loop {
                    let (lock, wake) = &*shared;
                    let mut data = lock.lock().unwrap_or_else(|e| e.into_inner());
                    while data.0.is_none() && !data.1 {
                        data = wake.wait(data).unwrap_or_else(|e| e.into_inner());
                    }
                    let Some((revision, text)) = data.0.take() else {
                        break;
                    };
                    drop(data);
                    bus.send(crate::event::Event::NoteSaved(
                        revision,
                        save(&directory, &text).is_ok(),
                    ));
                }
            })?;
        Ok(Self {
            pending,
            join: Some(join),
        })
    }
    pub fn submit(&self, revision: u64, text: String) {
        let (lock, wake) = &*self.pending;
        lock.lock().unwrap_or_else(|e| e.into_inner()).0 = Some((revision, text));
        wake.notify_one();
    }
    pub fn shutdown(&mut self) {
        let (lock, wake) = &*self.pending;
        lock.lock().unwrap_or_else(|e| e.into_inner()).1 = true;
        wake.notify_one();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn save(directory: &Path, text: &str) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let temp = directory.join("quick-note.tmp");
    let mut f = fs::File::create(&temp)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    drop(f);
    unsafe {
        use windows::{Win32::Storage::FileSystem::*, core::HSTRING};
        MoveFileExW(
            &HSTRING::from(temp.as_os_str()),
            &HSTRING::from(directory.join(FILE).as_os_str()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(io::Error::other)?;
    }
    Ok(())
}

pub mod clipboard {
    use windows::{
        Win32::{
            Foundation::*,
            System::{DataExchange::*, Memory::*},
        },
        core::*,
    };
    struct Open;
    impl Drop for Open {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }
    pub fn copy(hwnd: HWND, text: &str) -> Result<()> {
        let mut utf: Vec<u16> = text.replace('\n', "\r\n").encode_utf16().collect();
        utf.push(0);
        unsafe {
            OpenClipboard(Some(hwnd))?;
            let _open = Open;
            let memory = GlobalAlloc(GMEM_MOVEABLE, utf.len() * 2)?;
            let ptr = GlobalLock(memory).cast::<u16>();
            if ptr.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err(Error::from_thread());
            }
            std::ptr::copy_nonoverlapping(utf.as_ptr(), ptr, utf.len());
            let _ = GlobalUnlock(memory);
            if let Err(e) = EmptyClipboard()
                .and_then(|_| SetClipboardData(13, Some(HANDLE(memory.0))).map(|_| ()))
            {
                let _ = GlobalFree(Some(memory));
                return Err(e);
            }
            Ok(())
        }
    }
    pub fn paste(hwnd: HWND) -> Result<String> {
        unsafe {
            OpenClipboard(Some(hwnd))?;
            let _open = Open;
            let memory = HGLOBAL(GetClipboardData(13)?.0);
            let len = (GlobalSize(memory) / 2).min(super::LIMIT * 2 + 1);
            let ptr = GlobalLock(memory).cast::<u16>();
            if ptr.is_null() {
                return Err(Error::from_thread());
            }
            let utf = std::slice::from_raw_parts(ptr, len);
            let n = utf.iter().position(|c| *c == 0).unwrap_or(len);
            let text = String::from_utf16_lossy(&utf[..n]);
            let _ = GlobalUnlock(memory);
            Ok(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_selection_newlines_and_undo() {
        let mut e = Editor::default();
        e.insert("ide\r\n🥤 besok");
        assert_eq!(e.text, "ide\n🥤 besok");
        e.select(8, false);
        e.delete(true);
        assert_eq!(e.text, "ide\n besok");
        e.undo(false);
        assert_eq!(e.text, "ide\n🥤 besok");
        assert_eq!(e.cursor, e.anchor);
        e.undo(true);
        assert_eq!(e.text, "ide\n besok");
        e.select_all();
        e.insert("baru");
        assert_eq!(e.text, "baru");
    }
    #[test]
    fn paste_limit_and_utf16_round_trip() {
        let mut e = Editor::default();
        e.insert(&"🥤".repeat(LIMIT + 1));
        assert_eq!(e.text.chars().count(), LIMIT);
        for i in [0, 4, 80, e.text.len()] {
            assert_eq!(e.byte(e.utf16(i)), i);
        }
        e.insert("x");
        assert_eq!(e.text.chars().count(), LIMIT);
    }
    #[test]
    fn local_atomic_save_restore_and_unreadable_preservation() {
        let dir = std::env::temp_dir().join(format!("island-notes-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        save(&dir, "ingat besok\n🥤").unwrap();
        let n = Note::load(&dir);
        assert_eq!(n.editor.text, "ingat besok\n🥤");
        assert!(n.readable);
        save(&dir, "").unwrap();
        assert!(Note::load(&dir).editor.text.is_empty());
        fs::write(dir.join(FILE), [0xff, 0xfe]).unwrap();
        assert!(!Note::load(&dir).readable);
        assert_eq!(fs::read(dir.join(FILE)).unwrap(), [0xff, 0xfe]);
        fs::remove_file(dir.join(FILE)).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn stale_save_ack_does_not_clear_newer_edits() {
        let mut n = Note::load(&std::env::temp_dir().join("island-no-note-fixture"));
        n.editor.insert("first");
        n.changed(0, 100);
        assert_eq!(n.due, Some(700));
        let first = n.editor.revision;
        n.queued = first;
        n.editor.insert("second");
        n.changed(first, 200);
        n.completed(first, true, 300);
        assert_eq!(n.due, Some(800));
        assert_ne!(n.saved, n.editor.revision);
        n.completed(n.editor.revision, false, 1000);
        assert!(n.error);
        assert_eq!(n.due, Some(6000));
    }
    #[test]
    fn writer_flushes_latest_snapshot_on_shutdown() {
        use std::sync::atomic::{AtomicBool, AtomicIsize};
        let dir = std::env::temp_dir().join(format!("island-note-writer-{}", std::process::id()));
        let (tx, _rx) = std::sync::mpsc::channel();
        let bus = crate::event::Bus {
            tx,
            hwnd: Arc::new(AtomicIsize::new(0)),
            audio: Arc::new(Mutex::new(Default::default())),
            audio_pending: Arc::new(AtomicBool::new(false)),
        };
        let mut writer = Writer::new(dir.clone(), bus).unwrap();
        writer.submit(1, "old".into());
        writer.submit(2, "new 🥤".into());
        writer.shutdown();
        assert_eq!(fs::read_to_string(dir.join(FILE)).unwrap(), "new 🥤");
        fs::remove_file(dir.join(FILE)).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
