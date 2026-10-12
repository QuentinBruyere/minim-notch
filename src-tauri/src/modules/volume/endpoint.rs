//! Windows Core Audio: the default output's volume, on a thread of its own
//! that owns the COM objects. Windows calls back on every change (volume,
//! mute, default output), so nothing is polled.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, EDataFlow, ERole, IMMDeviceEnumerator,
    IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator, eConsole, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::core::{GUID, PCWSTR, Result, implement};

use super::{Command, Level};
use crate::platform::Key;

/// Marks the changes made from the notch: they are not shown briefly.
const FROM_NOTCH: GUID = GUID::from_u128(0x5f3a_9c2e_7b41_4d8a_9e06_1c2b_3d4e_5f60);

type OnChange = Arc<dyn Fn(Option<Level>, bool) + Send + Sync>;

/// Starts the audio thread; `on_change` gets each new level (`None` without
/// an output) and whether it came from outside the notch.
pub(super) fn spawn(
    on_change: impl Fn(Option<Level>, bool) + Send + Sync + 'static,
) -> Sender<Command> {
    let (commands, receiver) = mpsc::channel();
    let on_change: OnChange = Arc::new(on_change);
    let devices = commands.clone();
    std::thread::spawn(move || {
        unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                log::warn!("volume: cannot initialize COM");
                return;
            }
        }
        if let Err(e) = run(&receiver, devices, &on_change) {
            log::warn!("volume: {e}");
            on_change(None, false);
        }
        unsafe { CoUninitialize() };
    });
    commands
}

fn run(receiver: &Receiver<Command>, devices: Sender<Command>, on_change: &OnChange) -> Result<()> {
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let client: IMMNotificationClient = DeviceWatch {
        devices: Mutex::new(devices),
    }
    .into();
    unsafe { enumerator.RegisterEndpointNotificationCallback(&client)? };
    let mut output = Output::bind(&enumerator, on_change);
    for command in receiver {
        match command {
            Command::Stop => break,
            Command::DeviceChanged => {
                drop(output.take());
                output = Output::bind(&enumerator, on_change);
            }
            Command::Set(percent) => {
                if let Some(o) = &output {
                    o.set(percent);
                }
            }
            Command::ToggleMute => {
                if let Some(o) = &output {
                    o.toggle_mute();
                }
            }
            Command::Key(key) => {
                if let Some(o) = &output {
                    o.key(key);
                    // Shown briefly even when nothing moved (already at the
                    // top): the key was heard.
                    on_change(o.level(), true);
                }
            }
        }
    }
    drop(output);
    unsafe { enumerator.UnregisterEndpointNotificationCallback(&client)? };
    Ok(())
}

/// The default output's volume control, watched while bound.
struct Output {
    volume: IAudioEndpointVolume,
    watch: IAudioEndpointVolumeCallback,
}

impl Output {
    /// Binds to the current default output and reports its level; `None`
    /// (reported too) when there is no output.
    fn bind(enumerator: &IMMDeviceEnumerator, on_change: &OnChange) -> Option<Self> {
        let bound = (|| -> Result<Self> {
            let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? };
            let volume: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None)? };
            let watch: IAudioEndpointVolumeCallback = VolumeWatch {
                on_change: Arc::clone(on_change),
            }
            .into();
            unsafe { volume.RegisterControlChangeNotify(&watch)? };
            Ok(Self { volume, watch })
        })();
        match bound {
            Ok(output) => {
                on_change(output.level(), false);
                Some(output)
            }
            Err(e) => {
                log::info!("volume: no default output ({e})");
                on_change(None, false);
                None
            }
        }
    }

    fn level(&self) -> Option<Level> {
        let scalar = unsafe { self.volume.GetMasterVolumeLevelScalar() }.ok()?;
        let muted = unsafe { self.volume.GetMute() }.ok()?.as_bool();
        Some(level(scalar, muted))
    }

    /// Moving the slider brings the sound back if it was muted.
    fn set(&self, percent: u8) {
        unsafe {
            let _ = self
                .volume
                .SetMasterVolumeLevelScalar(f32::from(percent) / 100.0, &FROM_NOTCH);
            if percent > 0 && self.volume.GetMute().is_ok_and(|m| m.as_bool()) {
                let _ = self.volume.SetMute(false, &FROM_NOTCH);
            }
        }
    }

    /// A volume key taken over from Windows (ADR-0020): Windows' own step,
    /// and a key up or down brings the sound back, like Windows. Not marked
    /// as from the notch, so it shows briefly.
    fn key(&self, key: Key) {
        let context = std::ptr::null();
        unsafe {
            let muted = self.volume.GetMute().is_ok_and(|m| m.as_bool());
            match key {
                Key::VolumeMute => {
                    let _ = self.volume.SetMute(!muted, context);
                }
                Key::VolumeUp | Key::VolumeDown => {
                    if muted {
                        let _ = self.volume.SetMute(false, context);
                    }
                    let _ = if key == Key::VolumeUp {
                        self.volume.VolumeStepUp(context)
                    } else {
                        self.volume.VolumeStepDown(context)
                    };
                }
                _ => {}
            }
        }
    }

    fn toggle_mute(&self) {
        unsafe {
            if let Ok(muted) = self.volume.GetMute() {
                let _ = self.volume.SetMute(!muted.as_bool(), &FROM_NOTCH);
            }
        }
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        let _ = unsafe { self.volume.UnregisterControlChangeNotify(&self.watch) };
    }
}

fn level(scalar: f32, muted: bool) -> Level {
    Level {
        percent: (scalar * 100.0).round().clamp(0.0, 100.0) as u8,
        muted,
    }
}

/// Called by Windows on every volume or mute change of the bound output.
#[implement(IAudioEndpointVolumeCallback)]
struct VolumeWatch {
    on_change: OnChange,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeWatch_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        // Windows hands valid data for the length of the call.
        if let Some(data) = unsafe { data.as_ref() } {
            let external = data.guidEventContext != FROM_NOTCH;
            (self.on_change)(
                Some(level(data.fMasterVolume, data.bMuted.as_bool())),
                external,
            );
        }
        Ok(())
    }
}

/// Called by Windows when outputs come and go: follows the default one.
#[implement(IMMNotificationClient)]
struct DeviceWatch {
    devices: Mutex<Sender<Command>>,
}

impl IMMNotificationClient_Impl for DeviceWatch_Impl {
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _id: &PCWSTR) -> Result<()> {
        if flow == eRender && role == eConsole {
            let _ = self.devices.lock().unwrap().send(Command::DeviceChanged);
        }
        Ok(())
    }

    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> Result<()> {
        Ok(())
    }

    fn OnDeviceAdded(&self, _id: &PCWSTR) -> Result<()> {
        Ok(())
    }

    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> Result<()> {
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_rounded_percentages() {
        assert_eq!(level(0.454, false).percent, 45);
        assert_eq!(
            level(0.456, true),
            Level {
                percent: 46,
                muted: true
            }
        );
        assert_eq!(level(1.2, false).percent, 100);
    }

    /// Reads the real default output: run by hand on a machine with sound
    /// (`cargo test -- --ignored`).
    #[test]
    #[ignore]
    fn reads_the_default_output() {
        let (tx, rx) = mpsc::channel();
        let commands = spawn(move |level, _| {
            let _ = tx.send(level);
        });
        let level = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("a level is reported");
        assert!(level.is_some(), "no default output");
        let _ = commands.send(Command::Stop);
    }
}
