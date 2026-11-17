//! Open-Meteo city forecasts. No IP lookup, GPS, account or media metadata.
use crate::event::{Bus, Event};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct City {
    pub name: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub admin1: String,
    pub latitude: f64,
    pub longitude: f64,
}
impl City {
    pub fn jakarta() -> Self {
        Self {
            name: "Jakarta".into(),
            country: "Indonesia".into(),
            admin1: "Jakarta".into(),
            latitude: -6.21462,
            longitude: 106.84513,
        }
    }
    pub fn valid(&self) -> bool {
        !self.name.is_empty()
            && self.name.len() <= 160
            && self.latitude.is_finite()
            && self.longitude.is_finite()
            && (-90.0..=90.0).contains(&self.latitude)
            && (-180.0..=180.0).contains(&self.longitude)
    }
    pub fn label(&self) -> String {
        format!("{}, {}", self.name, self.country)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hour {
    pub time: i64,
    pub temperature: f64,
    pub code: u16,
    pub rain: Option<f64>,
    pub is_day: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Forecast {
    pub temperature: f64,
    pub feels_like: f64,
    pub code: u16,
    pub is_day: bool,
    pub wind: f64,
    pub high: f64,
    pub low: f64,
    pub time: i64,
    pub fetched: i64,
    pub utc_offset: i64,
    pub hours: Vec<Hour>,
}
impl Forecast {
    pub fn stale(&self) -> bool {
        epoch().saturating_sub(self.fetched) > 3600
    }
    pub fn clock(&self, time: i64) -> String {
        let minutes = (time + self.utc_offset).rem_euclid(86400) / 60;
        format!("{:02}:{:02}", minutes / 60, minutes % 60)
    }
}
#[derive(Default)]
pub struct WeatherState {
    pub forecast: Option<Forecast>,
    pub error: String,
    pub searching: bool,
    pub search_error: String,
    pub cities: Vec<City>,
    pub generation: u64,
    pub choosing: bool,
    pub refreshing: bool,
    pub refresh_after_ms: u64,
}
pub enum Request {
    Select(Option<City>),
    Refresh,
    Search(String, u64),
    Stop,
}
pub struct WeatherWorker {
    pub tx: mpsc::Sender<Request>,
    join: Option<JoinHandle<()>>,
}
#[derive(Serialize, Deserialize)]
struct Cache {
    city: City,
    forecast: Forecast,
}
impl WeatherWorker {
    pub fn start(
        bus: Bus,
        stop: Arc<AtomicBool>,
        directory: PathBuf,
        initial: Option<City>,
        offline: bool,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let mut city = initial;
            let mut due = Instant::now();
            let path = directory.join("weather-cache.json");
            if let Ok(bytes) = std::fs::read(&path)
                && bytes.len() <= 64000
                && let Ok(cache) = serde_json::from_slice::<Cache>(&bytes)
                && Some(&cache.city) == city.as_ref()
                && valid_forecast(&cache.forecast)
            {
                bus.send(Event::Weather(cache.city, Ok(cache.forecast)));
            }
            if offline && let Some(c) = city.as_ref() {
                bus.send(Event::Weather(
                    c.clone(),
                    Err("Weather is offline in this session.".into()),
                ));
            }
            while !stop.load(Ordering::Relaxed) {
                if !offline
                    && Instant::now() >= due
                    && let Some(c) = city.as_ref()
                {
                    let result = fetch(c, &stop);
                    due = Instant::now()
                        + Duration::from_secs(if result.is_ok() {
                            1800
                        } else {
                            backoff_remaining().max(300)
                        });
                    if let Ok(f) = &result {
                        let cache = Cache {
                            city: c.clone(),
                            forecast: f.clone(),
                        };
                        if let Ok(bytes) = serde_json::to_vec(&cache) {
                            let _ = std::fs::create_dir_all(&directory);
                            let _ = std::fs::write(&path, bytes);
                        }
                    }
                    bus.send(Event::Weather(c.clone(), result));
                }
                let wait = if offline || city.is_none() {
                    Duration::from_secs(86400)
                } else {
                    due.saturating_duration_since(Instant::now())
                        .max(Duration::from_millis(50))
                };
                match rx.recv_timeout(wait) {
                    Ok(Request::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Ok(Request::Select(c)) => {
                        city = c.filter(City::valid);
                        due = Instant::now();
                        if offline && let Some(c) = city.as_ref() {
                            bus.send(Event::Weather(
                                c.clone(),
                                Err("Weather is offline in this session.".into()),
                            ));
                        }
                    }
                    Ok(Request::Refresh) => {
                        due = Instant::now();
                        if offline && let Some(c) = city.as_ref() {
                            bus.send(Event::Weather(
                                c.clone(),
                                Err("Weather is offline in this session.".into()),
                            ));
                        }
                    }
                    Ok(Request::Search(query, generation)) => {
                        let result = if offline {
                            Err("Weather is offline in this session.".into())
                        } else {
                            search(&query, &stop)
                        };
                        bus.send(Event::Cities(generation, result));
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
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
fn epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
thread_local! { static BACKOFF: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) }; }
fn backoff_remaining() -> u64 {
    BACKOFF.with(|b| {
        b.get()
            .map(|t| {
                t.saturating_duration_since(Instant::now())
                    .as_secs()
                    .saturating_add(1)
            })
            .unwrap_or(0)
    })
}
fn json(host: &str, path: &str, stop: &AtomicBool) -> Result<serde_json::Value, String> {
    if backoff_remaining() > 1 {
        return Err("Weather is busy. Please try again later.".into());
    }
    let response = crate::network::get(host, path, stop)
        .map_err(|_| "Couldn't connect. Check your internet connection.".to_string())?;
    if response.status != 200 {
        return Err(if response.status == 429 {
            BACKOFF.with(|b| {
                b.set(Some(
                    Instant::now() + Duration::from_secs(response.retry_after.clamp(60, 86400)),
                ))
            });
            format!(
                "Weather is busy. Try again in {} minutes.",
                response.retry_after.div_ceil(60).max(1)
            )
        } else {
            "Weather is unavailable. Try again later.".into()
        });
    }
    serde_json::from_slice(&response.body)
        .map_err(|_| "The weather response could not be read.".into())
}
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn search(query: &str, stop: &AtomicBool) -> Result<Vec<City>, String> {
    let query = query.trim();
    if query.chars().count() < 2 || query.len() > 160 {
        return Err("Enter a city name, at least two letters.".into());
    }
    let value = json(
        "geocoding-api.open-meteo.com",
        &format!(
            "/v1/search?name={}&count=5&language=en&format=json",
            encode(query)
        ),
        stop,
    )?;
    let cities: Vec<City> = serde_json::from_value(
        value
            .get("results")
            .cloned()
            .unwrap_or(serde_json::json!([])),
    )
    .map_err(|_| "Could not read cities.".to_string())?;
    Ok(cities.into_iter().filter(City::valid).take(5).collect())
}
fn fetch(city: &City, stop: &AtomicBool) -> Result<Forecast, String> {
    if !city.valid() {
        return Err("Choose a valid city first.".into());
    }
    let path = format!(
        "/v1/forecast?latitude={:.5}&longitude={:.5}&current=temperature_2m,apparent_temperature,is_day,weather_code,wind_speed_10m&hourly=temperature_2m,weather_code,precipitation_probability,is_day&daily=temperature_2m_max,temperature_2m_min&timezone=auto&timeformat=unixtime&forecast_days=2",
        city.latitude, city.longitude
    );
    parse(&json("api.open-meteo.com", &path, stop)?)
}
fn number(v: &serde_json::Value) -> Result<f64, String> {
    v.as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| "Weather readings are incomplete.".into())
}
fn parse(v: &serde_json::Value) -> Result<Forecast, String> {
    let c = &v["current"];
    let time = c["time"].as_i64().ok_or("Missing observation time")?;
    let h = &v["hourly"];
    let times = h["time"].as_array().ok_or("Missing hourly forecast")?;
    let mut hours = Vec::new();
    for (i, t) in times.iter().enumerate() {
        let Some(t) = t.as_i64() else {
            continue;
        };
        if t < time {
            continue;
        }
        if let (Ok(temperature), Some(code)) = (
            number(&h["temperature_2m"][i]),
            h["weather_code"][i].as_u64(),
        ) {
            hours.push(Hour {
                time: t,
                temperature,
                code: code as u16,
                rain: h["precipitation_probability"][i]
                    .as_f64()
                    .filter(|v| (0.0..=100.0).contains(v)),
                is_day: h["is_day"][i].as_u64().unwrap_or(1) == 1,
            });
        }
        if hours.len() == 5 {
            break;
        }
    }
    let f = Forecast {
        temperature: number(&c["temperature_2m"])?,
        feels_like: number(&c["apparent_temperature"])?,
        code: c["weather_code"]
            .as_u64()
            .ok_or("Missing weather condition")? as u16,
        is_day: c["is_day"].as_u64() == Some(1),
        wind: number(&c["wind_speed_10m"])?,
        high: number(&v["daily"]["temperature_2m_max"][0])?,
        low: number(&v["daily"]["temperature_2m_min"][0])?,
        time,
        fetched: epoch(),
        utc_offset: v["utc_offset_seconds"].as_i64().unwrap_or(0),
        hours,
    };
    if !valid_forecast(&f) {
        return Err("Weather readings are incomplete.".into());
    }
    Ok(f)
}
fn valid_forecast(f: &Forecast) -> bool {
    [f.temperature, f.feels_like, f.wind, f.high, f.low]
        .iter()
        .all(|v| v.is_finite())
        && (-100.0..=70.0).contains(&f.temperature)
        && f.hours.len() <= 5
        && f.hours.iter().all(|h| h.temperature.is_finite())
}
pub fn condition(code: u16) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Foggy",
        51..=57 => "Drizzle",
        61..=67 => "Rainy",
        71..=77 => "Snowy",
        80..=82 => "Rain showers",
        85 | 86 => "Snow showers",
        95..=99 => "Thunderstorm",
        _ => "Weather unavailable",
    }
}
pub fn probe() -> windows::core::Result<()> {
    let stop = AtomicBool::new(false);
    let cities = search("Jakarta, Indonesia", &stop);
    let result = cities
        .and_then(|cities| {
            cities
                .into_iter()
                .find(|c| c.name == "Jakarta")
                .ok_or_else(|| "Jakarta not found".into())
        })
        .and_then(|city| {
            fetch(&city, &stop).map(|f| serde_json::json!({"city":city,"forecast":f}))
        });
    std::fs::create_dir_all("artifacts").ok();
    std::fs::write(
        "artifacts/weather-probe.json",
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .map_err(|e| {
        windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), e.to_string())
    })?;
    if result.is_err() {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "Weather probe failed; see artifacts/weather-probe.json",
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_partial_data_and_invalid_location() {
        assert!(parse(&serde_json::json!({"current":{"temperature_2m":29}})).is_err());
        let mut c = City::jakarta();
        c.latitude = f64::NAN;
        assert!(!c.valid());
        assert_eq!(encode("New York, US"), "New%20York%2C%20US");
    }
    #[test]
    fn hourly_selection_uses_observation_time_and_preserves_missing_rain() {
        let v = serde_json::json!({"current":{"time":5000,"temperature_2m":29,"apparent_temperature":32,"weather_code":2,"is_day":1,"wind_speed_10m":8},"daily":{"temperature_2m_max":[31],"temperature_2m_min":[24]},"utc_offset_seconds":25200,"hourly":{"time":[3600,7200,10800],"temperature_2m":[28,29,30],"weather_code":[1,2,3],"precipitation_probability":[10,null,60]}});
        let f = parse(&v).unwrap();
        assert_eq!(f.hours.len(), 2);
        assert_eq!(f.hours[0].time, 7200);
        assert_eq!(f.hours[0].rain, None);
        assert_eq!(f.clock(7200), "09:00");
        assert_eq!(condition(95), "Thunderstorm");
    }
}
