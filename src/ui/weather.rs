use super::*;
pub(super) fn draw(v: &mut View<'_>, now: u64) {
    let w = &v.s.weather;
    if w.choosing {
        v.label("CHOOSE YOUR CITY", 28, 57, 390, 25, 10, true, MUTED);
        let editing = v.s.editing == Some(EditField::City);
        v.c.round(28, 91, 292, 37, 11, CARD);
        v.label(
            &format!(
                "{}{}",
                if v.s.edit_text.is_empty() {
                    "Type a city name"
                } else {
                    &v.s.edit_text
                },
                if editing { "│" } else { "" }
            ),
            40,
            95,
            267,
            28,
            13,
            false,
            if editing { WHITE } else { MUTED },
        );
        v.hits.push(Hit {
            rect: [28, 91, 292, 37],
            action: Action::WeatherSearch,
        });
        v.button(
            if editing { "Search" } else { "Edit" },
            [330, 91, 98, 37],
            if editing {
                Action::WeatherSubmit
            } else {
                Action::WeatherSearch
            },
            true,
            !w.searching,
        );
        for (i, city) in w.cities.iter().enumerate() {
            v.button(
                &city.label(),
                [28, 141 + i as i32 * 29, 400, 26],
                Action::WeatherChoose(i),
                false,
                true,
            );
        }
        if w.searching {
            v.label("Finding cities…", 28, 143, 400, 28, 13, false, MUTED);
        } else if !w.search_error.is_empty() {
            v.c.text_block(&w.search_error, 28, 143, 400, 58, 13, false, MUTED);
        }
        v.c.text("Cancel", 338, 278, 90, 24, 11, false, MUTED, true);
        v.hits.push(Hit {
            rect: [338, 276, 90, 26],
            action: Action::WeatherCancel,
        });
        return;
    }
    let city = v.s.config.weather_city.as_ref();
    v.label(
        &city
            .map(|c| c.label())
            .unwrap_or_else(|| "Choose a city".into()),
        28,
        54,
        294,
        26,
        12,
        true,
        MUTED,
    );
    v.button(
        "Change city",
        [327, 53, 101, 28],
        Action::WeatherSearch,
        false,
        true,
    );
    if let Some(f) = &w.forecast {
        icons::weather(v.c, f.code, f.is_day, 29, 94, 77);
        v.label(
            &format!("{:.0}°", f.temperature),
            126,
            87,
            145,
            59,
            43,
            true,
            WHITE,
        );
        v.label(
            crate::weather::condition(f.code),
            128,
            146,
            280,
            24,
            14,
            true,
            WHITE,
        );
        v.label(
            &format!(
                "↑ {:.0}°  ↓ {:.0}°   Feels {:.0}°",
                f.high, f.low, f.feels_like
            ),
            128,
            172,
            286,
            22,
            11,
            false,
            MUTED,
        );
        for (i, h) in f.hours.iter().enumerate() {
            let x = 28 + i as i32 * 81;
            v.c.round(x, 210, 76, 99, 13, CARD);
            v.c.text(&f.clock(h.time), x, 213, 76, 19, 10, false, MUTED, true);
            icons::weather(v.c, h.code, h.is_day, x + 24, 236, 28);
            v.c.text(
                &format!("{:.0}°", h.temperature),
                x,
                268,
                76,
                22,
                13,
                true,
                WHITE,
                true,
            );
            if let Some(rain) = h.rain {
                v.c.text(
                    &format!("{rain:.0}% rain"),
                    x,
                    290,
                    76,
                    17,
                    9,
                    false,
                    MUTED,
                    true,
                );
            }
        }
        let subtitle = if !w.error.is_empty() {
            String::new()
        } else if f.stale() {
            "Saved forecast · updating…".into()
        } else {
            format!("Updated {} · Wind {:.0} km/h", f.clock(f.time), f.wind)
        };
        if !subtitle.is_empty() {
            v.label(&subtitle, 28, 317, 296, 20, 10, false, MUTED);
        }
        v.button(
            if w.refreshing {
                "Updating…"
            } else {
                "Refresh"
            },
            [336, 314, 92, 29],
            Action::WeatherRefresh,
            false,
            !w.refreshing && now >= w.refresh_after_ms,
        );
    } else {
        icons::weather(v.c, 2, true, 188, 107, 80);
        v.center(
            if !w.error.is_empty() {
                "Forecast unavailable."
            } else if city.is_some() {
                "Getting the forecast…"
            } else {
                "How's the day outside?"
            },
            198,
            34,
            21,
            true,
            WHITE,
        );
        v.c.text_block(
            if w.error.is_empty() {
                "Weather for your selected city. No location tracking."
            } else {
                &w.error
            },
            44,
            239,
            368,
            48,
            12,
            false,
            MUTED,
        );
        v.button(
            if w.refreshing {
                "Updating…"
            } else if city.is_some() {
                "Try again"
            } else {
                "Choose a city"
            },
            [143, 290, 170, 32],
            if city.is_some() {
                Action::WeatherRefresh
            } else {
                Action::WeatherSearch
            },
            false,
            city.is_none() || (!w.refreshing && now >= w.refresh_after_ms),
        );
    }
}
