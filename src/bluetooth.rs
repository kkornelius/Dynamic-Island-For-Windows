use crate::{
    event::{Bus, Event},
    platform::Apartment,
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};
use windows::{
    Devices::Enumeration::*,
    Foundation::{IPropertyValue, TypedEventHandler},
    core::*,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeviceKind {
    Earbuds,
    Keyboard,
    Mouse,
    Speaker,
    Phone,
    Controller,
    #[default]
    Unknown,
}
impl DeviceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Earbuds => "Headphones",
            Self::Keyboard => "Keyboard",
            Self::Mouse => "Mouse",
            Self::Speaker => "Speaker",
            Self::Phone => "Phone",
            Self::Controller => "Controller",
            Self::Unknown => "Bluetooth device",
        }
    }
}
fn device_kind(p: &windows_collections::IMapView<HSTRING, IInspectable>, name: &str) -> DeviceKind {
    let number = |key: &str| {
        p.Lookup(&HSTRING::from(key))
            .ok()
            .and_then(|v| v.cast::<IPropertyValue>().ok())
            .and_then(|v| v.GetUInt16().ok())
    };
    classify(
        number("System.Devices.Aep.Bluetooth.Cod.Major"),
        number("System.Devices.Aep.Bluetooth.Cod.Minor"),
        number("System.Devices.Aep.Bluetooth.Le.Appearance.Category"),
        number("System.Devices.Aep.Bluetooth.Le.Appearance.Subcategory"),
        name,
    )
}
fn classify(
    major: Option<u16>,
    minor: Option<u16>,
    category: Option<u16>,
    sub: Option<u16>,
    name: &str,
) -> DeviceKind {
    // Windows exposes the six-bit minor class, without the CoD's two reserved bits.
    let m = minor.unwrap_or(0);
    if major == Some(5) {
        if m & 16 != 0 {
            return DeviceKind::Keyboard;
        }
        if m & 32 != 0 {
            return DeviceKind::Mouse;
        }
        if matches!(m, 1 | 2) {
            return DeviceKind::Controller;
        }
    }
    if category == Some(15) {
        match sub {
            Some(1) => return DeviceKind::Keyboard,
            Some(2) => return DeviceKind::Mouse,
            Some(3 | 4) => return DeviceKind::Controller,
            _ => {}
        }
    }
    if major == Some(4) {
        match m {
            1..=3 | 6 => return DeviceKind::Earbuds,
            5 => return DeviceKind::Speaker,
            _ => {}
        }
    }
    if major == Some(2) || category == Some(1) {
        return DeviceKind::Phone;
    }
    if category == Some(33) && matches!(sub, Some(1..=5)) {
        return DeviceKind::Speaker;
    }
    if category == Some(37) {
        return DeviceKind::Earbuds;
    }
    // Some BLE devices omit appearance. Only recognisable device words get a fallback.
    let n = name.to_lowercase();
    if ["airpods", "earbud", "headphone", "headset", "tws"]
        .iter()
        .any(|w| n.contains(w))
    {
        DeviceKind::Earbuds
    } else if n.contains("keyboard") {
        DeviceKind::Keyboard
    } else if n.contains("mouse") {
        DeviceKind::Mouse
    } else if n.contains("speaker") {
        DeviceKind::Speaker
    } else {
        DeviceKind::Unknown
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn properties_take_priority_and_unknown_stays_generic() {
        assert_eq!(
            classify(Some(5), Some(16), None, None, "Office"),
            DeviceKind::Keyboard
        );
        assert_eq!(
            classify(Some(5), Some(32), None, None, "AirPods"),
            DeviceKind::Mouse
        );
        assert_eq!(
            classify(None, None, Some(15), Some(2), "MX"),
            DeviceKind::Mouse
        );
        assert_eq!(
            classify(Some(4), Some(6), None, None, "Audio"),
            DeviceKind::Earbuds
        );
        assert_eq!(
            classify(None, None, None, None, "AirPods Pro"),
            DeviceKind::Earbuds
        );
        assert_eq!(
            classify(None, None, Some(33), Some(2), "Living room"),
            DeviceKind::Speaker
        );
        assert_eq!(
            classify(None, None, Some(37), Some(1), "Audio"),
            DeviceKind::Earbuds
        );
        assert_eq!(
            classify(None, None, None, None, "XYZ-87"),
            DeviceKind::Unknown
        );
    }
}

pub struct BluetoothWorker {
    pub tx: mpsc::Sender<bool>,
    join: Option<JoinHandle<()>>,
}
impl BluetoothWorker {
    pub fn start(bus: Bus, stop: Arc<AtomicBool>, enabled: bool) -> Self {
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let result = (|| -> Result<()> {
                let _apartment = Apartment::new()?;
                let mut watcher: Option<Watch> = None;
                let mut on = enabled;
                if !on {
                    bus.send(Event::BluetoothStatus("Bluetooth notifications off".into()));
                }
                while !stop.load(Ordering::Relaxed) {
                    if on && watcher.is_none() {
                        match Watch::new(bus.clone()) {
                            Ok(w) => {
                                watcher = Some(w);
                                bus.send(Event::BluetoothStatus("Watching paired devices".into()));
                            }
                            Err(e) => {
                                bus.send(Event::BluetoothStatus(format!(
                                    "Bluetooth unavailable ({})",
                                    e.code()
                                )));
                                on = false;
                            }
                        }
                    }
                    if !on && watcher.is_some() {
                        watcher = None;
                        bus.send(Event::BluetoothStatus("Bluetooth notifications off".into()));
                    }
                    match rx.recv() {
                        Ok(value) => on = value,
                        Err(_) => break,
                    }
                }
                drop(watcher);
                Ok(())
            })();
            if let Err(e) = result {
                bus.send(Event::BluetoothStatus(format!(
                    "Bluetooth unavailable ({})",
                    e.code()
                )));
            }
        });
        Self {
            tx,
            join: Some(join),
        }
    }
    pub fn shutdown(&mut self) {
        let _ = self.tx.send(false);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}
#[derive(Default)]
struct Records {
    ready: bool,
    devices: HashMap<String, (String, Option<bool>, DeviceKind)>,
}
struct Watch {
    watcher: DeviceWatcher,
    tokens: [i64; 4],
}
fn property(properties: &windows_collections::IMapView<HSTRING, IInspectable>) -> Option<bool> {
    properties
        .Lookup(&HSTRING::from("System.Devices.Aep.IsConnected"))
        .ok()?
        .cast::<IPropertyValue>()
        .ok()?
        .GetBoolean()
        .ok()
}
impl Watch {
    fn new(bus: Bus) -> Result<Self> {
        // Paired classic/LE association endpoints only; no active advertisement scanning.
        let aqs = HSTRING::from(
            "((System.Devices.Aep.ProtocolId:=\"{e0cbf06c-cd8b-4647-bb8a-263b43f0f974}\") OR (System.Devices.Aep.ProtocolId:=\"{bb7bb05e-5972-42b5-94fc-76eaa7084d49}\")) AND (System.Devices.Aep.IsPaired:=System.StructuredQueryType.Boolean#True)",
        );
        let props = windows_collections::IIterable::<HSTRING>::from(
            [
                "System.Devices.Aep.IsConnected",
                "System.Devices.Aep.Bluetooth.Cod.Major",
                "System.Devices.Aep.Bluetooth.Cod.Minor",
                "System.Devices.Aep.Bluetooth.Le.Appearance.Category",
                "System.Devices.Aep.Bluetooth.Le.Appearance.Subcategory",
            ]
            .into_iter()
            .map(HSTRING::from)
            .collect::<Vec<_>>(),
        );
        let watcher = DeviceInformation::CreateWatcherWithKindAqsFilterAndAdditionalProperties(
            &aqs,
            &props,
            DeviceInformationKind::AssociationEndpoint,
        )?;
        let mut watch = Self {
            watcher,
            tokens: [0; 4],
        };
        let records = Arc::new(Mutex::new(Records::default()));
        let r = records.clone();
        let b = bus.clone();
        watch.tokens[0] = watch.watcher.Added(&TypedEventHandler::<
            DeviceWatcher,
            DeviceInformation,
        >::new(move |_, item| {
            if let Some(item) = item.as_ref() {
                let id = item.Id()?.to_string();
                let name = item.Name()?.to_string();
                let properties = item.Properties()?;
                let connected = property(&properties);
                let kind = device_kind(&properties, &name);
                let mut r = r.lock().unwrap_or_else(|e| e.into_inner());
                let previous = r.devices.insert(id, (name.clone(), connected, kind));
                if r.ready
                    && connected == Some(true)
                    && previous.as_ref().and_then(|v| v.1) != Some(true)
                {
                    b.send(Event::Bluetooth {
                        name,
                        kind,
                        connected: true,
                    });
                }
            }
            Ok(())
        }))?;
        let r = records.clone();
        let b = bus.clone();
        watch.tokens[1] = watch.watcher.Updated(&TypedEventHandler::<
            DeviceWatcher,
            DeviceInformationUpdate,
        >::new(move |_, update| {
            if let Some(update) = update.as_ref() {
                let id = update.Id()?.to_string();
                let properties = update.Properties()?;
                let connected = property(&properties);
                let mut r = r.lock().unwrap_or_else(|e| e.into_inner());
                let ready = r.ready;
                if let Some(record) = r.devices.get_mut(&id) {
                    if [
                        "System.Devices.Aep.Bluetooth.Cod.Major",
                        "System.Devices.Aep.Bluetooth.Le.Appearance.Category",
                    ]
                    .iter()
                    .any(|k| properties.Lookup(&HSTRING::from(*k)).is_ok())
                    {
                        let kind = device_kind(&properties, &record.0);
                        if kind != DeviceKind::Unknown {
                            record.2 = kind;
                        }
                    }
                    if let Some(connected) = connected {
                        if ready && record.1.is_some() && record.1 != Some(connected) {
                            b.send(Event::Bluetooth {
                                name: record.0.clone(),
                                kind: record.2,
                                connected,
                            });
                        }
                        record.1 = Some(connected);
                    }
                }
            }
            Ok(())
        }))?;
        let r = records.clone();
        let b = bus;
        watch.tokens[2] = watch.watcher.Removed(&TypedEventHandler::<
            DeviceWatcher,
            DeviceInformationUpdate,
        >::new(move |_, update| {
            if let Some(update) = update.as_ref() {
                let mut r = r.lock().unwrap_or_else(|e| e.into_inner());
                if let Some((name, Some(true), kind)) = r.devices.remove(&update.Id()?.to_string())
                    && r.ready
                {
                    b.send(Event::Bluetooth {
                        name,
                        kind,
                        connected: false,
                    });
                }
            }
            Ok(())
        }))?;
        watch.tokens[3] =
            watch
                .watcher
                .EnumerationCompleted(&TypedEventHandler::new(move |_, _| {
                    records.lock().unwrap_or_else(|e| e.into_inner()).ready = true;
                    Ok(())
                }))?;
        watch.watcher.Start()?;
        Ok(watch)
    }
}
impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.watcher.Stop();
        let _ = self.watcher.RemoveAdded(self.tokens[0]);
        let _ = self.watcher.RemoveUpdated(self.tokens[1]);
        let _ = self.watcher.RemoveRemoved(self.tokens[2]);
        let _ = self.watcher.RemoveEnumerationCompleted(self.tokens[3]);
    }
}
