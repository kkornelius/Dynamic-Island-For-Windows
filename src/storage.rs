use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockMode {
    #[default]
    Floating,
    Anchored,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WaterConfig {
    pub enabled: bool,
    pub interval_minutes: u16,
    pub active_start: u16,
    pub active_end: u16,
    pub quiet_enabled: bool,
    pub quiet_start: u16,
    pub quiet_end: u16,
}
impl Default for WaterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_minutes: 45,
            active_start: 8,
            active_end: 22,
            quiet_enabled: true,
            quiet_start: 22,
            quiet_end: 8,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub theme: String,
    pub opacity: u8,
    pub position: Option<[i32; 2]>,
    pub dock_mode: DockMode,
    pub anchored_position: Option<[i32; 2]>,
    pub online_lyrics: bool,
    pub lyrics_offset_ms: i32,
    pub stroke_enabled: bool,
    pub stroke_color: u32,
    pub stroke_width: u8,
    pub stroke_glow: u8,
    pub weather_city: Option<crate::weather::City>,
    pub always_on_top: bool,
    pub clock_24h: bool,
    pub show_date: bool,
    pub visualizer: bool,
    pub sensitivity: f32,
    pub smoothing: f32,
    pub bluetooth: bool,
    pub alarm_sound: bool,
    pub timer_seconds: u64,
    pub notification_seconds: u16,
    pub water: WaterConfig,
    pub water_day: u32,
    pub water_count: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            version: 4,
            theme: "Black".into(),
            opacity: 96,
            position: None,
            dock_mode: DockMode::Floating,
            anchored_position: None,
            online_lyrics: false,
            lyrics_offset_ms: 0,
            stroke_enabled: true,
            stroke_color: 0xe6e6e6,
            stroke_width: 1,
            stroke_glow: 70,
            weather_city: Some(crate::weather::City::jakarta()),
            always_on_top: true,
            clock_24h: true,
            show_date: true,
            visualizer: true,
            sensitivity: 2.5,
            smoothing: 0.72,
            bluetooth: true,
            alarm_sound: true,
            timer_seconds: 1500,
            notification_seconds: 7,
            water: WaterConfig::default(),
            water_day: 0,
            water_count: 0,
        }
    }
}
impl Config {
    pub fn validate(&mut self) {
        self.version = 4;
        self.stroke_color &= 0xffffff;
        self.stroke_glow = self.stroke_glow.min(100);
        self.stroke_width = self.stroke_width.clamp(1, 3);
        if self.weather_city.as_ref().is_some_and(|c| !c.valid()) {
            self.weather_city = None;
        }
        self.theme = "Black".into();
        self.opacity = self.opacity.clamp(75, 100);
        self.sensitivity = if self.sensitivity.is_finite() {
            self.sensitivity.clamp(0.5, 8.0)
        } else {
            2.5
        };
        self.smoothing = if self.smoothing.is_finite() {
            self.smoothing.clamp(0.1, 0.95)
        } else {
            0.72
        };
        self.timer_seconds = self.timer_seconds.clamp(1, 86400);
        self.notification_seconds = self.notification_seconds.clamp(3, 30);
        self.water.interval_minutes = self.water.interval_minutes.clamp(1, 240);
        self.water.active_start %= 24;
        self.water.active_end %= 24;
        self.water.quiet_start %= 24;
        self.water.quiet_end %= 24;
        self.water_count = self.water_count.min(10000);
        self.lyrics_offset_ms = self.lyrics_offset_ms.clamp(-10000, 10000);
    }
}
pub struct Store {
    pub directory: PathBuf,
}
impl Store {
    pub fn new() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let explicit = args
            .iter()
            .position(|a| a == "--data-dir")
            .and_then(|i| args.get(i + 1));
        if let Some(path) = explicit {
            return Self {
                directory: PathBuf::from(path),
            };
        }
        let root = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let directory = root.join("DynamicIslandForWindows");
        // Compatibility identifier only: keep existing preferences and caches during rebranding.
        const LEGACY_DIRECTORY: &str = "MidnightIsland";
        let legacy = root.join(LEGACY_DIRECTORY);
        if migrate_legacy_data(&legacy, &directory).is_err() {
            // Keep the user's existing data usable if the new directory is unavailable.
            return Self { directory: legacy };
        }
        Self { directory }
    }
    pub fn load(&self) -> (Config, Option<String>) {
        let path = self.directory.join("settings.json");
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Config>(&bytes) {
                Ok(mut config) => {
                    config.validate();
                    (config, None)
                }
                Err(_) => {
                    let stamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let backup = self
                        .directory
                        .join(format!("settings.corrupt-{stamp}.json"));
                    let saved = fs::copy(&path, backup).is_ok();
                    (Config::default(),Some(if saved{"Settings could not be read. Defaults loaded; original backed up."}else{"Settings could not be read. Defaults loaded; backup could not be written."}.into()))
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::default(), None),
            Err(_) => (
                Config::default(),
                Some("Settings are unavailable. Using defaults.".into()),
            ),
        }
    }
    pub fn save(&self, config: &Config) -> std::io::Result<()> {
        fs::create_dir_all(&self.directory)?;
        let temp = self.directory.join("settings.tmp");
        let mut file = fs::File::create(&temp)?;
        use std::io::Write;
        file.write_all(&serde_json::to_vec_pretty(config)?)?;
        file.sync_all()?;
        drop(file);
        #[cfg(windows)]
        unsafe {
            use windows::{Win32::Storage::FileSystem::*, core::HSTRING};
            MoveFileExW(
                &HSTRING::from(temp.as_os_str()),
                &HSTRING::from(self.directory.join("settings.json").as_os_str()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
            .map_err(std::io::Error::other)?;
        }
        Ok(())
    }
    pub fn log(&self, message: &str) {
        use std::io::Write;
        let _ = fs::create_dir_all(&self.directory);
        let path = self.directory.join("diagnostics.log");
        if fs::metadata(&path)
            .map(|m| m.len() > 128_000)
            .unwrap_or(false)
            && fs::copy(&path, self.directory.join("diagnostics.previous.log")).is_ok()
        {
            let _ = fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&path);
        }
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{message}");
        }
    }
}
fn migrate_legacy_data(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<bool> {
    let settings = source.join("settings.json");
    if destination.join("settings.json").exists() || !settings.exists() {
        return Ok(false);
    }
    fs::create_dir_all(destination)?;
    for name in ["lyrics-cache.json", "weather-cache.json"] {
        if source.join(name).is_file() && !destination.join(name).exists() {
            fs::copy(source.join(name), destination.join(name))?;
        }
    }
    let bytes = fs::read(&settings)?;
    if let Ok(mut config) = serde_json::from_slice::<Config>(&bytes) {
        config.validate();
        Store {
            directory: destination.to_path_buf(),
        }
        .save(&config)?;
    } else {
        fs::write(destination.join("settings.json"), bytes)?;
    }
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_settings_migrate_without_changing_position_or_enabling_lyrics() {
        let mut c: Config =
            serde_json::from_str(r#"{"version":1,"position":[780,32],"opacity":89}"#).unwrap();
        c.validate();
        assert_eq!(c.version, 4);
        assert_eq!(c.position, Some([780, 32]));
        assert_eq!(c.opacity, 89);
        assert_eq!(c.dock_mode, DockMode::Floating);
        assert!(!c.online_lyrics);
        c.dock_mode = DockMode::Anchored;
        c.anchored_position = Some([960, 0]);
        let saved = serde_json::to_string(&c).unwrap();
        let restored: Config = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.position, c.position);
        assert_eq!(restored.anchored_position, c.anchored_position);
        assert_eq!(restored.dock_mode, DockMode::Anchored);
    }
    #[test]
    fn version_two_preserves_opt_in_and_validates_new_style_fields() {
        let mut c:Config=serde_json::from_str(r#"{"version":2,"online_lyrics":true,"stroke_width":255,"stroke_color":4294967295,"weather_city":{"name":"Bad","latitude":200,"longitude":0}}"#).unwrap();
        c.validate();
        assert_eq!(c.version, 4);
        assert!(c.online_lyrics);
        assert_eq!(c.stroke_width, 3);
        assert_eq!(c.stroke_color, 0xffffff);
        assert!(c.weather_city.is_none());
    }
    #[test]
    fn rebrand_migration_preserves_settings_caches_and_existing_destination() {
        let root = std::env::temp_dir().join(format!("island-migration-{}", std::process::id()));
        let source = root.join("legacy");
        let destination = root.join("current");
        fs::create_dir_all(&source).unwrap();
        let store = Store {
            directory: source.clone(),
        };
        let config = Config {
            position: Some([432, 72]),
            anchored_position: Some([432, 0]),
            opacity: 100,
            online_lyrics: true,
            stroke_color: 0x89abcd,
            sensitivity: 3.5,
            ..Default::default()
        };
        store.save(&config).unwrap();
        let original = fs::read(source.join("settings.json")).unwrap();
        fs::write(source.join("lyrics-cache.json"), b"lyric fixture").unwrap();
        fs::write(source.join("weather-cache.json"), b"weather fixture").unwrap();
        assert!(migrate_legacy_data(&source, &destination).unwrap());
        let current = Store {
            directory: destination.clone(),
        };
        let loaded = current.load().0;
        assert_eq!(loaded.position, config.position);
        assert_eq!(loaded.anchored_position, config.anchored_position);
        assert_eq!(loaded.stroke_color, config.stroke_color);
        assert_eq!(loaded.sensitivity, 3.5);
        assert!(loaded.online_lyrics);
        assert_eq!(loaded.theme, "Black");
        assert_eq!(fs::read(source.join("settings.json")).unwrap(), original);
        assert_eq!(
            fs::read(destination.join("weather-cache.json")).unwrap(),
            b"weather fixture"
        );
        let changed = Config {
            opacity: 87,
            ..loaded
        };
        current.save(&changed).unwrap();
        assert!(!migrate_legacy_data(&source, &destination).unwrap());
        assert_eq!(current.load().0.opacity, 87);
        for dir in [&source, &destination] {
            for name in ["settings.json", "lyrics-cache.json", "weather-cache.json"] {
                fs::remove_file(dir.join(name)).unwrap();
            }
            fs::remove_dir(dir).unwrap();
        }
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn defaults_and_validation() {
        let mut c: Config = serde_json::from_str(
            r#"{"opacity":1,"timer_seconds":0,"water":{"interval_minutes":0,"quiet_start":49}}"#,
        )
        .unwrap();
        c.validate();
        assert_eq!(c.opacity, 75);
        assert_eq!(c.timer_seconds, 1);
        assert_eq!(c.water.interval_minutes, 1);
        assert_eq!(c.water.quiet_start, 1);
        assert!(c.always_on_top);
    }
    #[test]
    fn persistence_and_corrupt_recovery() {
        let directory =
            std::env::temp_dir().join(format!("dynamic-island-test-{}", std::process::id()));
        let store = Store {
            directory: directory.clone(),
        };
        let c = Config {
            opacity: 87,
            ..Default::default()
        };
        store.save(&c).unwrap();
        assert_eq!(store.load().0.opacity, 87);
        fs::write(directory.join("settings.json"), b"invalid").unwrap();
        assert!(store.load().1.is_some());
        assert!(
            fs::read_dir(&directory).unwrap().any(|x| x
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("corrupt"))
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
