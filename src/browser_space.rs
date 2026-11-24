//! Browser-chrome geometry only: no tab names, URLs, page contents or browser control.
use crate::event::{Bus, Event};
use std::{
    collections::VecDeque,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Dwm::*,
        System::{Com::*, Threading::*},
        UI::{Accessibility::*, HiDpi::*, WindowsAndMessaging::*},
    },
    core::*,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Rect(pub i32, pub i32, pub i32, pub i32);
impl Rect {
    fn from(r: RECT) -> Self {
        Self(r.left, r.top, r.right, r.bottom)
    }
    fn valid(self) -> bool {
        self.2 > self.0 && self.3 > self.1
    }
    fn overlaps(self, other: Self, padding: i32) -> bool {
        self.valid()
            && other.valid()
            && self.0 < other.2 + padding
            && self.2 > other.0 - padding
            && self.1 < other.3 + padding
            && self.3 > other.1 - padding
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Query {
    pub generation: u64,
    pub window: isize,
    pub area: Rect,
}
pub struct Layout {
    tabs: Vec<Rect>,
    buttons: Vec<Rect>,
    visited: usize,
    complete: bool,
}
impl Layout {
    fn canonicalize(&mut self) {
        for rects in [&mut self.tabs, &mut self.buttons] {
            rects.retain(|r| r.valid());
            rects.sort_by_key(|r| (r.1, r.0, r.3, r.2));
            rects.dedup();
        }
    }
    fn blocked(&self, area: Rect, padding: i32) -> Option<bool> {
        if self.tabs.is_empty() {
            return None;
        }
        let top = self.tabs.iter().map(|r| r.1).min()?;
        let bottom = self.tabs.iter().map(|r| r.3).max()?;
        // Only buttons in the tab row count, including New tab and caption controls.
        let hit = self
            .tabs
            .iter()
            .chain(self.buttons.iter().filter(|r| {
                r.1 >= top - padding
                    && r.3 <= bottom + padding
                    && r.3 - r.1 <= bottom - top + padding
            }))
            .any(|r| area.overlaps(*r, padding));
        if hit || self.complete {
            Some(hit)
        } else {
            None
        }
    }
}
fn browser(window: HWND) -> bool {
    unsafe {
        let mut class = [0u16; 128];
        let n = GetClassNameW(window, &mut class);
        let class = String::from_utf16_lossy(&class[..n.max(0) as usize]);
        if !class.starts_with("Chrome_WidgetWin_") && class != "MozillaWindowClass" {
            return false;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(window, Some(&mut pid));
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut path = [0u16; 1024];
        let mut length = path.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )
        .is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return false;
        }
        let path = String::from_utf16_lossy(&path[..length as usize]);
        matches!(
            path.rsplit(['\\', '/'])
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
            "chrome.exe"
                | "brave.exe"
                | "msedge.exe"
                | "firefox.exe"
                | "opera.exe"
                | "vivaldi.exe"
                | "arc.exe"
                | "zen.exe"
                | "thorium.exe"
                | "floorp.exe"
                | "librewolf.exe"
                | "waterfox.exe"
        )
    }
}
fn usable(window: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(window).as_bool() || IsIconic(window).as_bool() {
            return false;
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(window, DWMWA_CLOAKED, (&mut cloaked as *mut u32).cast(), 4);
        cloaked == 0 && GetWindowLongW(window, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 == 0
    }
}
/// First visible top-level surface at this area; never inspect a browser behind another app.
pub fn target(own: HWND, area: Rect) -> Option<isize> {
    unsafe {
        let mut next = GetTopWindow(None).ok();
        for _ in 0..160 {
            let window = next?;
            next = GetWindow(window, GW_HWNDNEXT).ok();
            if window == own || !usable(window) {
                continue;
            }
            let mut r = RECT::default();
            if GetWindowRect(window, &mut r).is_err() || !area.overlaps(Rect::from(r), 0) {
                continue;
            }
            return browser(window).then_some(window.0 as isize);
        }
        None
    }
}
struct Reader {
    automation: IUIAutomation,
    cache: IUIAutomationCacheRequest,
    condition: IUIAutomationCondition,
    row_cache: Option<(isize, Instant, Vec<IUIAutomationElement>)>,
}
impl Reader {
    fn new() -> Result<Self> {
        unsafe {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
            let options: IUIAutomation2 = automation.cast()?;
            options.SetConnectionTimeout(150)?;
            options.SetTransactionTimeout(200)?;
            options.SetAutoSetFocus(false)?;
            let cache = automation.CreateCacheRequest()?;
            cache.SetTreeScope(TreeScope_Element)?;
            for property in [
                UIA_ControlTypePropertyId,
                UIA_BoundingRectanglePropertyId,
                UIA_IsOffscreenPropertyId,
                UIA_ProcessIdPropertyId,
            ] {
                cache.AddProperty(property)?;
            }
            let condition = automation.CreateTrueCondition()?;
            Ok(Self {
                automation,
                cache,
                condition,
                row_cache: None,
            })
        }
    }
    /// Only chrome geometry/focus is read. Never retrieve the URL, edit value,
    /// search text, document text, or browser history.
    fn address_bar(&self, window: HWND) -> Result<Option<Rect>> {
        unsafe {
            let element = self.automation.GetFocusedElementBuildCache(&self.cache)?;
            let kind = element.CachedControlType()?;
            if kind != UIA_EditControlTypeId && kind != UIA_ComboBoxControlTypeId {
                return Ok(None);
            }
            let mut pid = 0;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if element.CachedProcessId()? as u32 != pid || element.CachedIsOffscreen()?.as_bool() {
                return Ok(None);
            }
            let bounds = Rect::from(element.CachedBoundingRectangle()?);
            let mut frame = RECT::default();
            GetWindowRect(window, &mut frame)?;
            let scale = (GetDpiForWindow(window) as f32 / 96.0).max(1.0);
            let header = Rect(
                frame.left,
                frame.top,
                frame.right,
                frame.top + (150.0 * scale) as i32,
            );
            if !bounds.overlaps(header, 0) {
                return Ok(None);
            }
            // Website search fields can be near the screen top too. Reject every
            // edit nested in a web Document, even when its rectangle overlaps us.
            let walker = self.automation.ControlViewWalker()?;
            let mut parent = element;
            for _ in 0..16 {
                parent = match walker.GetParentElementBuildCache(&parent, &self.cache) {
                    Ok(parent) => parent,
                    Err(_) => return Ok(None),
                };
                let kind = parent.CachedControlType()?;
                if kind == UIA_DocumentControlTypeId {
                    return Ok(None);
                }
                if kind == UIA_WindowControlTypeId {
                    return Ok(Some(bounds));
                }
            }
            Ok(None)
        }
    }
    fn cached_row(&self, parents: &[IUIAutomationElement], window: HWND) -> Result<Layout> {
        unsafe {
            let start = Instant::now();
            let mut bounds = RECT::default();
            GetWindowRect(window, &mut bounds)?;
            let scale = (GetDpiForWindow(window) as f32 / 96.0).max(1.0);
            let header = Rect(
                bounds.left,
                bounds.top,
                bounds.right,
                bounds.top + (150.0 * scale) as i32,
            );
            let mut layout = Layout {
                tabs: Vec::new(),
                buttons: Vec::new(),
                visited: 0,
                complete: true,
            };
            let mut queue: VecDeque<_> = parents.iter().cloned().map(|p| (p, 0)).collect();
            while let Some((parent, depth)) = queue.pop_front() {
                if start.elapsed() > Duration::from_millis(180) || layout.visited >= 240 {
                    layout.complete = false;
                    break;
                }
                let children =
                    parent.FindAllBuildCache(TreeScope_Children, &self.condition, &self.cache)?;
                let count = children.Length()?;
                if count == 0 && depth == 0 {
                    return Err(Error::empty());
                }
                let mut visible = false;
                for i in 0..count.min(300) {
                    if start.elapsed() > Duration::from_millis(180) || layout.visited >= 240 {
                        layout.complete = false;
                        break;
                    }
                    let element = children.GetElement(i)?;
                    layout.visited += 1;
                    let kind = element.CachedControlType()?;
                    if kind == UIA_DocumentControlTypeId || element.CachedIsOffscreen()?.as_bool() {
                        continue;
                    }
                    let r = Rect::from(element.CachedBoundingRectangle()?);
                    if !r.overlaps(header, 0) {
                        continue;
                    }
                    visible = true;
                    if kind == UIA_TabItemControlTypeId {
                        layout.tabs.push(r);
                    } else if kind == UIA_ButtonControlTypeId || kind == UIA_MenuItemControlTypeId {
                        layout.buttons.push(r);
                    } else if depth < 9 {
                        queue.push_back((element, depth + 1));
                    } else {
                        layout.complete = false;
                    }
                }
                if !visible && depth == 0 {
                    return Err(Error::empty());
                }
                if !layout.complete {
                    break;
                }
            }
            layout.canonicalize();
            Ok(layout)
        }
    }
    fn layout(&mut self, window: HWND) -> Result<Layout> {
        unsafe {
            if let Some((handle, saved, parents)) = &self.row_cache
                && *handle == window.0 as isize
                && saved.elapsed() < Duration::from_secs(2)
                && let Ok(layout) = self.cached_row(parents, window)
                && layout.complete
                && !layout.tabs.is_empty()
            {
                return Ok(layout);
            }
            self.row_cache = None;
            let start = Instant::now();
            let root = self
                .automation
                .ElementFromHandleBuildCache(window, &self.cache)?;
            let bounds = Rect::from(root.CachedBoundingRectangle()?);
            let scale = (GetDpiForWindow(window) as f32 / 96.0).max(1.0);
            let header = Rect(
                bounds.0,
                bounds.1,
                bounds.2,
                bounds.1 + (150.0 * scale) as i32,
            );
            let mut queue = VecDeque::from([(root, 0)]);
            let mut parents = Vec::new();
            let mut layout = Layout {
                tabs: Vec::new(),
                buttons: Vec::new(),
                visited: 0,
                complete: true,
            };
            while let Some((parent, depth)) = queue.pop_front() {
                if start.elapsed() > Duration::from_millis(180) || layout.visited >= 240 {
                    layout.complete = false;
                    break;
                }
                let children = match parent.FindAllBuildCache(
                    TreeScope_Children,
                    &self.condition,
                    &self.cache,
                ) {
                    Ok(children) => children,
                    Err(_) => {
                        layout.complete = false;
                        continue;
                    }
                };
                for i in 0..children.Length()?.min(300) {
                    if start.elapsed() > Duration::from_millis(180) || layout.visited >= 240 {
                        layout.complete = false;
                        break;
                    }
                    let element = children.GetElement(i)?;
                    layout.visited += 1;
                    let kind = element.CachedControlType()?;
                    // Do not descend into web documents, even when they contain tabs/buttons.
                    if kind == UIA_DocumentControlTypeId || element.CachedIsOffscreen()?.as_bool() {
                        continue;
                    }
                    let r = Rect::from(element.CachedBoundingRectangle()?);
                    if !r.overlaps(header, 0) {
                        continue;
                    }
                    if kind == UIA_TabItemControlTypeId {
                        layout.tabs.push(r);
                        parents.push((parent.clone(), r, true));
                    } else if kind == UIA_ButtonControlTypeId || kind == UIA_MenuItemControlTypeId {
                        layout.buttons.push(r);
                        parents.push((parent.clone(), r, false));
                    } else if depth < 9 {
                        queue.push_back((element, depth + 1));
                    } else {
                        layout.complete = false;
                    }
                }
            }
            layout.canonicalize();
            if layout.complete && !layout.tabs.is_empty() {
                let top = layout.tabs.iter().map(|r| r.1).min().unwrap();
                let bottom = layout.tabs.iter().map(|r| r.3).max().unwrap();
                let mut row: Vec<IUIAutomationElement> = Vec::new();
                for (parent, r, tab) in parents {
                    if (tab || (r.1 >= top - 4 && r.3 <= bottom + 4))
                        && !row.iter().any(|p| p.as_raw() == parent.as_raw())
                    {
                        row.push(parent);
                    }
                }
                if row.len() <= 8 {
                    self.row_cache = Some((window.0 as isize, Instant::now(), row));
                }
            }
            Ok(layout)
        }
    }
}
#[derive(Default)]
struct Stability {
    hidden: bool,
    clear_since: Option<Instant>,
    unknown_since: Option<Instant>,
}
impl Stability {
    fn update(&mut self, value: Option<bool>, now: Instant) -> bool {
        match value {
            Some(true) => {
                self.hidden = true;
                self.clear_since = None;
                self.unknown_since = None;
            }
            Some(false) => {
                self.unknown_since = None;
                let since = *self.clear_since.get_or_insert(now);
                if now.duration_since(since) >= Duration::from_millis(600) {
                    self.hidden = false;
                }
            }
            None => {
                self.clear_since = None;
                let since = *self.unknown_since.get_or_insert(now);
                if now.duration_since(since) >= Duration::from_millis(2000) {
                    self.hidden = false;
                }
            }
        }
        self.hidden
    }
}
enum Request {
    Watch(Option<Query>),
    Stop,
}
pub struct BrowserWorker {
    tx: mpsc::Sender<Request>,
    join: Option<JoinHandle<()>>,
}
impl BrowserWorker {
    pub fn start(bus: Bus) -> Self {
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let Ok(_apartment) = crate::platform::Apartment::new() else {
                return;
            };
            let mut reader = None;
            let mut query = None;
            let mut stable = Stability::default();
            let mut sent = None;
            loop {
                match rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(Request::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Ok(Request::Watch(next)) => {
                        if query != next {
                            stable = Stability {
                                hidden: stable.hidden && next.is_some(),
                                ..Default::default()
                            };
                            sent = None;
                        }
                        query = next;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                let mut stopping = false;
                while let Ok(request) = rx.try_recv() {
                    match request {
                        Request::Stop => {
                            stopping = true;
                            break;
                        }
                        Request::Watch(next) => {
                            if query != next {
                                stable = Stability {
                                    hidden: stable.hidden && next.is_some(),
                                    ..Default::default()
                                };
                                sent = None;
                            }
                            query = next;
                        }
                    }
                }
                if stopping {
                    break;
                }
                let Some(q) = query else {
                    continue;
                };
                if reader.is_none() {
                    reader = Reader::new().ok();
                }
                let value = reader.as_mut().and_then(|r| {
                    let window = HWND(q.window as _);
                    if r.address_bar(window)
                        .ok()
                        .flatten()
                        .is_some_and(|rect| q.area.overlaps(rect, 4))
                    {
                        Some(true)
                    } else {
                        r.layout(window)
                            .ok()
                            .and_then(|layout| layout.blocked(q.area, 4))
                    }
                });
                let hidden = stable.update(value, Instant::now());
                if sent != Some(hidden) {
                    bus.send(Event::BrowserSpace(q.generation, hidden));
                    sent = Some(hidden);
                }
            }
        });
        Self {
            tx,
            join: Some(join),
        }
    }
    pub fn watch(&self, query: Option<Query>) {
        let _ = self.tx.send(Request::Watch(query));
    }
    pub fn shutdown(&mut self) {
        let _ = self.tx.send(Request::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn probe_worker(window: HWND, occupied: Rect, gap: Option<Rect>) -> serde_json::Value {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicIsize},
    };
    let (tx, rx) = mpsc::channel();
    let bus = Bus {
        tx,
        hwnd: Arc::new(AtomicIsize::new(0)),
        audio: Arc::new(Mutex::new(crate::event::AudioFrame::default())),
        audio_pending: Arc::new(AtomicBool::new(false)),
    };
    let mut worker = BrowserWorker::start(bus);
    let wait = |generation, desired| {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(4) {
            let remaining = Duration::from_secs(4).saturating_sub(start.elapsed());
            match rx.recv_timeout(remaining) {
                Ok(Event::BrowserSpace(g, hidden)) if g == generation && hidden == desired => {
                    return true;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        false
    };
    worker.watch(Some(Query {
        generation: 1,
        window: window.0 as isize,
        area: occupied,
    }));
    let hides = wait(1, true);
    let returns = gap.map(|area| {
        worker.watch(Some(Query {
            generation: 2,
            window: window.0 as isize,
            area,
        }));
        wait(2, false)
    });
    worker.shutdown();
    serde_json::json!({"occupied_tab_hides":hides,"clear_gap_restores":returns,"actual_browser_input_sent":false})
}
pub fn probe() -> Result<()> {
    let _apartment = crate::platform::Apartment::new()?;
    let mut reader = Reader::new()?;
    let mut reports = Vec::new();
    unsafe {
        let mut next = GetTopWindow(None).ok();
        for _ in 0..160 {
            let Some(window) = next else {
                break;
            };
            next = GetWindow(window, GW_HWNDNEXT).ok();
            if !usable(window) || !browser(window) {
                continue;
            }
            let started = Instant::now();
            let report = match reader.layout(window) {
                Ok(layout) => {
                    let initial_ms = started.elapsed().as_secs_f64() * 1000.0;
                    let warm = Instant::now();
                    let refreshed = reader.layout(window)?;
                    let warm_ms = warm.elapsed().as_secs_f64() * 1000.0;
                    let mut bounds = RECT::default();
                    let _ = GetWindowRect(window, &mut bounds);
                    let top = layout.tabs.iter().map(|r| r.1).min().unwrap_or(bounds.top);
                    let gap = (bounds.left..bounds.right.saturating_sub(336))
                        .step_by(16)
                        .map(|x| Rect(x, top, x + 336, top + 58))
                        .find(|area| layout.blocked(*area, 4) == Some(false));
                    let worker = layout.tabs.first().map(|r| probe_worker(window, *r, gap));
                    serde_json::json!({"tabs":layout.tabs,"buttons":layout.buttons,
                        "visited":layout.visited,"complete":layout.complete,"duration_ms":initial_ms,
                        "warm_duration_ms":warm_ms,"worker":worker,
                        "warm_same_tab_geometry":layout.tabs == refreshed.tabs,"warm_visited":refreshed.visited})
                }
                Err(e) => {
                    serde_json::json!({"error":format!("{:?}",e.code()),"duration_ms":started.elapsed().as_millis()})
                }
            };
            reports.push(report);
            if reports.len() >= 4 {
                break;
            }
        }
    }
    let report =
        serde_json::json!({"geometry_only":true,"browser_input_sent":false,"windows":reports});
    std::fs::create_dir_all("artifacts").ok();
    std::fs::write(
        "artifacts/browser-space-probe.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| Error::new(HRESULT(0x80004005u32 as i32), e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_actual_tabs_and_same_row_buttons_occupy_the_capsule_gap() {
        let area = Rect(790, 0, 1130, 58);
        let mut layout = Layout {
            tabs: vec![Rect(40, 0, 280, 36), Rect(280, 0, 520, 36)],
            buttons: vec![
                Rect(530, 4, 560, 32),
                Rect(1800, 0, 1900, 32),
                Rect(200, 44, 1750, 80),
            ],
            visited: 5,
            complete: true,
        };
        assert_eq!(layout.blocked(area, 4), Some(false));
        layout.tabs.push(Rect(720, 0, 960, 36));
        assert_eq!(layout.blocked(area, 4), Some(true));
        layout.tabs.pop();
        layout.buttons.push(Rect(800, 4, 830, 32));
        assert_eq!(layout.blocked(area, 4), Some(true));
    }
    #[test]
    fn focused_address_geometry_includes_the_capsule_but_lower_chrome_does_not() {
        // Matches an anchored capsule overlapping the top of Brave's address bar.
        let capsule = Rect(584, 0, 920, 58);
        assert!(capsule.overlaps(Rect(208, 40, 1298, 80), 4));
        assert!(!capsule.overlaps(Rect(208, 84, 1298, 120), 4));
        assert!(!capsule.overlaps(Rect(0, 40, 400, 80), 4));
    }
    #[test]
    fn negative_coordinates_vertical_tabs_and_unknown_layouts_are_safe() {
        let layout = Layout {
            tabs: vec![Rect(-1920, 100, -1700, 140)],
            buttons: vec![],
            visited: 1,
            complete: true,
        };
        assert_eq!(layout.blocked(Rect(-1130, 0, -790, 58), 4), Some(false));
        let empty = Layout {
            tabs: vec![],
            buttons: vec![],
            visited: 1,
            complete: true,
        };
        assert_eq!(empty.blocked(Rect(0, 0, 100, 50), 4), None);
    }
    #[test]
    fn tab_creation_blips_cannot_reveal_a_blocked_capsule() {
        let start = Instant::now();
        let mut s = Stability::default();
        assert!(s.update(Some(true), start));
        assert!(s.update(Some(false), start + Duration::from_millis(200)));
        assert!(s.update(Some(false), start + Duration::from_millis(500)));
        assert!(s.update(Some(true), start + Duration::from_millis(550)));
        assert!(s.update(Some(false), start + Duration::from_millis(800)));
        assert!(s.update(None, start + Duration::from_millis(1000)));
        assert!(s.update(Some(false), start + Duration::from_millis(1200)));
        assert!(s.update(Some(false), start + Duration::from_millis(1799)));
        assert!(!s.update(Some(false), start + Duration::from_millis(1800)));
    }
    #[test]
    fn hides_immediately_but_waits_for_stable_empty_space_before_returning() {
        let start = Instant::now();
        let mut s = Stability::default();
        assert!(s.update(Some(true), start));
        assert!(s.update(Some(false), start));
        assert!(s.update(Some(false), start + Duration::from_millis(599)));
        assert!(!s.update(Some(false), start + Duration::from_millis(600)));
        assert!(s.update(Some(true), start + Duration::from_millis(610)));
        assert!(s.update(None, start + Duration::from_millis(700)));
        assert!(!s.update(None, start + Duration::from_millis(2700)));
    }
}
