//! Windows media transport controls (the media flyout of the volume keys),
//! on a thread of their own. Windows calls back on every change (another
//! app, another track, play / pause, position), so nothing is polled.

use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use windows::Foundation::{DateTime, TypedEventHandler};
use windows::Graphics::Imaging::{BitmapDecoder, BitmapEncoder, BitmapInterpolationMode};
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties as MediaProperties;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::{
    DataReader, IRandomAccessStream, IRandomAccessStreamWithContentType, InMemoryRandomAccessStream,
};
use windows::core::{Interface, Result, RuntimeType};

use super::{Command, Timeline, Track};

/// Covers past this size are not read.
const MAX_COVER_BYTES: u64 = 8 * 1024 * 1024;

/// Largest side of the cover sent to the notch, in pixels: shown at 76 px
/// when the notch is open, sharp up to a 200 % display scale.
const COVER_PX: u32 = 160;

/// 100 ns ticks between 1601-01-01 (Windows) and 1970-01-01 (Unix).
const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;

/// Starts the media thread; `on_change` gets each new state (`None`:
/// nothing playing nor paused).
pub(super) fn spawn(on_change: impl Fn(Option<Track>) + Send + 'static) -> Sender<Command> {
    let (commands, receiver) = mpsc::channel();
    let refresh = commands.clone();
    std::thread::spawn(move || {
        if let Err(e) = run(&receiver, refresh, &on_change) {
            log::warn!("media: {e}");
            on_change(None);
        }
    });
    commands
}

/// An event handler that asks the thread to read the state again.
fn refresher<S: RuntimeType + 'static, A: RuntimeType + 'static>(
    refresh: &Sender<Command>,
) -> TypedEventHandler<S, A> {
    let refresh = refresh.clone();
    TypedEventHandler::new(move |_, _| {
        let _ = refresh.send(Command::Refresh);
        Ok(())
    })
}

/// The current session, watched for changes.
struct Watched {
    session: Session,
    tokens: [i64; 3],
}

impl Watched {
    fn new(session: Session, refresh: &Sender<Command>) -> Result<Self> {
        let tokens = [
            session.MediaPropertiesChanged(&refresher(refresh))?,
            session.PlaybackInfoChanged(&refresher(refresh))?,
            session.TimelinePropertiesChanged(&refresher(refresh))?,
        ];
        Ok(Self { session, tokens })
    }

    fn unwatch(self) {
        let [media, playback, timeline] = self.tokens;
        let _ = self.session.RemoveMediaPropertiesChanged(media);
        let _ = self.session.RemovePlaybackInfoChanged(playback);
        let _ = self.session.RemoveTimelinePropertiesChanged(timeline);
    }
}

fn run(
    receiver: &Receiver<Command>,
    refresh: Sender<Command>,
    on_change: &impl Fn(Option<Track>),
) -> Result<()> {
    let manager = Manager::RequestAsync()?.join()?;
    let current_token = manager.CurrentSessionChanged(&refresher(&refresh))?;
    let sessions_token = manager.SessionsChanged(&refresher(&refresh))?;

    // Every open media app is watched: the one shown may change without
    // Windows changing its current one (a browser starting a real video).
    let mut watched: Vec<Watched> = Vec::new();
    // The session shown, which the buttons control.
    let mut shown: Option<Session> = None;
    let mut memory = Memory::default();
    let _ = refresh.send(Command::Refresh);
    while let Ok(command) = receiver.recv() {
        let session = shown.clone();
        match command {
            Command::Refresh => {
                // Several events often come at once: one read is enough.
                while let Ok(Command::Refresh) = receiver.try_recv() {}
                let sessions = sessions(&manager);
                if sessions.len() != watched.len()
                    || sessions.iter().zip(&watched).any(|(s, w)| s != &w.session)
                {
                    for old in watched.drain(..) {
                        old.unwatch();
                    }
                    watched = sessions
                        .iter()
                        .filter_map(|s| Watched::new(s.clone(), &refresh).ok())
                        .collect();
                }
                shown = choose(manager.GetCurrentSession().ok(), &sessions);
                let track = shown
                    .as_ref()
                    .and_then(|s| read(s, &mut memory, &refresh).ok());
                on_change(track);
            }
            Command::Previous => control(session, |s| s.TrySkipPreviousAsync()?.join()),
            Command::TogglePlayPause => control(session, |s| s.TryTogglePlayPauseAsync()?.join()),
            Command::Next => control(session, |s| s.TrySkipNextAsync()?.join()),
            Command::Seek(position) => control(session, |s| {
                let start = s.GetTimelineProperties()?.StartTime()?.Duration;
                let ticks = start + (position.as_nanos() / 100) as i64;
                s.TryChangePlaybackPositionAsync(ticks)?.join()
            }),
            Command::Stop => break,
        }
    }
    for old in watched {
        old.unwatch();
    }
    let _ = manager.RemoveCurrentSessionChanged(current_token);
    let _ = manager.RemoveSessionsChanged(sessions_token);
    Ok(())
}

/// The open media apps.
fn sessions(manager: &Manager) -> Vec<Session> {
    let Ok(list) = manager.GetSessions() else {
        return Vec::new();
    };
    (0..list.Size().unwrap_or(0))
        .filter_map(|i| list.GetAt(i).ok())
        .collect()
}

/// The session to show: Windows' current one if it has a title, else
/// another one with a title, playing first. A session without title is not
/// a media (a browser left as current once an album has ended): none of
/// them means nothing to show.
fn choose(current: Option<Session>, sessions: &[Session]) -> Option<Session> {
    let titled = |s: &Session| {
        s.TryGetMediaPropertiesAsync()
            .and_then(|p| p.join())
            .and_then(|p| p.Title())
            .is_ok_and(|t| !t.is_empty())
    };
    let playing = |s: &Session| {
        s.GetPlaybackInfo()
            .and_then(|p| p.PlaybackStatus())
            .is_ok_and(|status| status == Status::Playing)
    };
    if let Some(current) = current.filter(titled) {
        return Some(current);
    }
    let mut others: Vec<&Session> = sessions.iter().filter(|s| titled(s)).collect();
    others.sort_by_key(|s| !playing(s));
    others.first().map(|s| (*s).clone())
}

/// Runs a control on the shown session; Windows tells the result through
/// the change events.
fn control(session: Option<Session>, act: impl FnOnce(&Session) -> Result<bool>) {
    if let Some(session) = session
        && let Err(e) = act(&session)
    {
        log::warn!("media control: {e}");
    }
}

/// What the thread remembers between two reads.
#[derive(Default)]
struct Memory {
    /// The current track (its key) and when it was first seen.
    track: Option<(String, Instant)>,
    /// The current track's cover, as a data URL.
    cover: Option<String>,
    /// Apps that gave a progress once: while a new track loads, they keep
    /// an empty bar rather than none.
    with_timeline: HashSet<String>,
}

/// Some apps (Apple Music, a new album) tell the track before its cover:
/// none yet, or still the previous track's one. The cover is read again at
/// each change during this time after a new track, then kept.
const COVER_SETTLING: Duration = Duration::from_secs(5);

/// Reads asked in that time, for apps that tell nothing when the cover
/// arrives.
const COVER_RETRIES: [Duration; 4] = [
    Duration::from_millis(500),
    Duration::from_millis(1500),
    Duration::from_secs(3),
    COVER_SETTLING,
];

fn read(session: &Session, memory: &mut Memory, refresh: &Sender<Command>) -> Result<Track> {
    let properties = session.TryGetMediaPropertiesAsync()?.join()?;
    let playback = session.GetPlaybackInfo()?;
    let controls = playback.Controls()?;
    let title = properties.Title()?.to_string();
    let artist = properties.Artist()?.to_string();
    let playing = playback.PlaybackStatus()? == Status::Playing;

    let app = session.SourceAppUserModelId()?.to_string();
    let key = format!("{app}\u{0}{title}\u{0}{artist}");
    let new_track = memory.track.as_ref().is_none_or(|(k, _)| k != &key);
    if new_track {
        memory.track = Some((key, Instant::now()));
        memory.cover = None;
        for wait in COVER_RETRIES {
            let refresh = refresh.clone();
            std::thread::spawn(move || {
                std::thread::sleep(wait);
                let _ = refresh.send(Command::Refresh);
            });
        }
    }
    let settling = memory
        .track
        .as_ref()
        .is_some_and(|(_, since)| since.elapsed() <= COVER_SETTLING);
    if new_track || settling {
        // A cover that goes missing while settling keeps the last one.
        if let Some(url) = read_cover(&properties).ok().flatten() {
            memory.cover = Some(url);
        }
    }
    let timeline = timeline(session, playing, controls.IsPlaybackPositionEnabled()?)
        .ok()
        .flatten();
    if timeline.is_some() {
        memory.with_timeline.insert(app.clone());
    }

    Ok(Track {
        title,
        artist,
        cover: memory.cover.clone(),
        playing,
        can_previous: controls.IsPreviousEnabled()?,
        can_next: controls.IsNextEnabled()?,
        can_toggle: controls.IsPlayPauseToggleEnabled()?
            || controls.IsPlayEnabled()?
            || controls.IsPauseEnabled()?,
        expects_timeline: memory.with_timeline.contains(&app),
        timeline,
    })
}

fn timeline(session: &Session, playing: bool, can_seek: bool) -> Result<Option<Timeline>> {
    let timeline = session.GetTimelineProperties()?;
    let start = timeline.StartTime()?.Duration;
    let end = timeline.EndTime()?.Duration;
    if end <= start {
        return Ok(None);
    }
    let mut position = timeline.Position()?.Duration - start;
    // Some apps (Spotify) only update the position now and then: add the
    // time played since.
    if playing {
        position += elapsed_since(timeline.LastUpdatedTime()?);
    }
    Ok(Some(Timeline {
        position: ticks(position.clamp(0, end - start)),
        duration: ticks(end - start),
        at: Instant::now(),
        can_seek,
    }))
}

/// 100 ns ticks since a Windows date, 0 if it is unknown or in the future.
fn elapsed_since(date: DateTime) -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_nanos() / 100) as i64 + UNIX_EPOCH_TICKS)
        .unwrap_or(0);
    if date.UniversalTime <= 0 {
        return 0;
    }
    (now - date.UniversalTime).max(0)
}

fn ticks(ticks: i64) -> Duration {
    Duration::from_nanos(ticks.max(0) as u64 * 100)
}

/// The cover as a small `data:` URL, if the app gives one. Shrunk to
/// `COVER_PX`: the notch shows it at 20 px, and it goes to the front with
/// every refresh (each second while playing). Apps describe their pictures
/// loosely (Apple Music: "image/jpeg,image/jpe,image/jpg"), so the type is
/// read from the bytes.
fn read_cover(properties: &MediaProperties) -> Result<Option<String>> {
    let Ok(reference) = properties.Thumbnail() else {
        return Ok(None);
    };
    let stream = reference.OpenReadAsync()?.join()?;
    let size = stream.Size()?;
    if size == 0 || size > MAX_COVER_BYTES {
        return Ok(None);
    }
    let bytes = match shrink(&stream) {
        Ok(bytes) => bytes,
        Err(e) => {
            log::warn!("media: cannot shrink the cover: {e}");
            read_all(&stream)?
        }
    };
    let Some(kind) = image_type(&bytes) else {
        return Ok(None);
    };
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(Some(format!("data:{kind};base64,{data}")))
}

/// The picture at most `COVER_PX` wide and high, in its own format.
fn shrink(stream: &IRandomAccessStreamWithContentType) -> Result<Vec<u8>> {
    let decoder = BitmapDecoder::CreateAsync(stream)?.join()?;
    let (width, height) = (decoder.PixelWidth()?, decoder.PixelHeight()?);
    let scale = (f64::from(COVER_PX) / f64::from(width.max(height).max(1))).min(1.0);
    let out = InMemoryRandomAccessStream::new()?;
    let encoder = BitmapEncoder::CreateForTranscodingAsync(&out, &decoder)?.join()?;
    let transform = encoder.BitmapTransform()?;
    transform.SetScaledWidth(((f64::from(width) * scale).round() as u32).max(1))?;
    transform.SetScaledHeight(((f64::from(height) * scale).round() as u32).max(1))?;
    transform.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
    encoder.FlushAsync()?.join()?;
    read_all(&out)
}

/// The whole stream, from its start: it may come in several loads.
fn read_all(stream: &impl Interface) -> Result<Vec<u8>> {
    let stream: IRandomAccessStream = stream.cast()?;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    let mut bytes = Vec::new();
    loop {
        let loaded = reader.LoadAsync(64 * 1024)?.join()?;
        if loaded == 0 {
            break;
        }
        let start = bytes.len();
        bytes.resize(start + loaded as usize, 0);
        reader.ReadBytes(&mut bytes[start..])?;
        if bytes.len() as u64 > MAX_COVER_BYTES {
            break;
        }
    }
    Ok(bytes)
}

/// The picture's type from its first bytes; `None` if not a picture the
/// notch can show.
fn image_type(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0xff, 0xd8, 0xff, ..] => Some("image/jpeg"),
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [b'B', b'M', ..] => Some("image/bmp"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("image/webp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diagnostic on the real machine: `cargo test -p minim-notch-media -- --ignored --nocapture`
    /// with something playing. Prints what the current app gives as cover.
    #[test]
    #[ignore]
    fn print_the_current_cover() {
        let manager = Manager::RequestAsync().unwrap().join().unwrap();
        let session = manager.GetCurrentSession().expect("nothing playing");
        let properties = session
            .TryGetMediaPropertiesAsync()
            .unwrap()
            .join()
            .unwrap();
        println!("app: {}", session.SourceAppUserModelId().unwrap());
        println!("title: {}", properties.Title().unwrap());
        let reference = properties.Thumbnail().expect("no thumbnail");
        let stream = reference.OpenReadAsync().unwrap().join().unwrap();
        println!(
            "type: {:?}, size: {}",
            stream.ContentType(),
            stream.Size().unwrap()
        );
        let reader = DataReader::CreateDataReader(&stream).unwrap();
        let mut total = 0u64;
        let mut first = Vec::new();
        loop {
            let n = reader.LoadAsync(64 * 1024).unwrap().join().unwrap();
            if n == 0 {
                break;
            }
            let mut chunk = vec![0; n as usize];
            reader.ReadBytes(&mut chunk).unwrap();
            if first.is_empty() {
                first = chunk[..chunk.len().min(16)].to_vec();
            }
            total += u64::from(n);
            println!("loaded {n} (total {total})");
        }
        println!("first bytes: {first:02x?}");
        let cover = read_cover(&properties).unwrap().expect("no cover");
        println!("data URL: {} bytes, {}", cover.len(), &cover[..40]);
    }
}
