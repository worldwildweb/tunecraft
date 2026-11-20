//! Audio output and the sequencer that decides what plays next
//! (rules from Songbird's sbMediacoreSequencer, see MECHANICS.md).

use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Stopped,
    Playing,
    Paused,
}

pub struct Audio {
    device: Option<rodio::MixerDeviceSink>,
    player: Option<rodio::Player>,
    pub error: Option<String>,
}

impl Audio {
    pub fn new() -> Audio {
        Audio { device: None, player: None, error: None }
    }

    fn device(&mut self) -> Option<&rodio::MixerDeviceSink> {
        if self.device.is_none() {
            match open_output() {
                Ok(mut d) => {
                    d.log_on_drop(false);
                    self.device = Some(d);
                }
                Err(e) => self.error = Some(format!("No sound output: {e}")),
            }
        }
        self.device.as_ref()
    }

    /// Starts a file from the beginning. Returns the decoder's duration when it knows one.
    pub fn play_file(&mut self, path: &Path, volume: f32) -> Result<Option<Duration>, String> {
        self.stop();
        let file = std::fs::File::open(path).map_err(|e| format!("Can't open {}: {e}", path.display()))?;
        let decoder = rodio::Decoder::try_from(file).map_err(|e| format!("Can't play {}: {e}", path.display()))?;
        let total = rodio::Source::total_duration(&decoder);
        let Some(device) = self.device() else {
            return Err(self.error.clone().unwrap_or_default());
        };
        let player = rodio::Player::connect_new(device.mixer());
        player.set_volume(volume as _);
        player.append(decoder);
        player.play();
        self.player = Some(player);
        self.error = None;
        Ok(total)
    }

    pub fn pause(&self) {
        if let Some(p) = &self.player {
            p.pause();
        }
    }

    pub fn resume(&self) {
        if let Some(p) = &self.player {
            p.play();
        }
    }

    pub fn stop(&mut self) {
        if let Some(p) = self.player.take() {
            p.stop();
        }
    }

    pub fn set_volume(&self, v: f32) {
        if let Some(p) = &self.player {
            p.set_volume(v as _);
        }
    }

    pub fn position(&self) -> Duration {
        self.player.as_ref().map(|p| p.get_pos()).unwrap_or_default()
    }

    pub fn seek(&self, to: Duration) {
        if let Some(p) = &self.player {
            let _ = p.try_seek(to);
        }
    }

    /// True once the current file has played to its end.
    pub fn finished(&self) -> bool {
        self.player.as_ref().map(|p| p.empty()).unwrap_or(false)
    }
}

/// Opens the Mac's current sound output. rodio's own default asks for a 50 ms buffer
/// (2048 frames at 44.1 kHz), which some outputs refuse (a soundbar here allows 14..=1024),
/// so first ask for a buffer inside the range the device reports, then the device's own
/// default buffer, then rodio's search over other devices and settings.
fn open_output() -> Result<rodio::MixerDeviceSink, String> {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    let mut first_error: Option<String> = None;
    if let Some(dev) = rodio::cpal::default_host().default_output_device() {
        if let Ok(cfg) = dev.default_output_config() {
            if let rodio::cpal::SupportedBufferSize::Range { min, max } = cfg.buffer_size() {
                let frames = 2048u32.clamp(*min, (*max).max(*min));
                match rodio::DeviceSinkBuilder::from_device(dev.clone())
                    .map(|b| b.with_buffer_size(rodio::cpal::BufferSize::Fixed(frames)))
                    .and_then(|b| b.open_stream())
                {
                    Ok(s) => return Ok(s),
                    Err(e) => first_error = Some(e.to_string()),
                }
            }
        }
        match rodio::DeviceSinkBuilder::from_device(dev)
            .map(|b| b.with_buffer_size(rodio::cpal::BufferSize::Default))
            .and_then(|b| b.open_stream())
        {
            Ok(s) => return Ok(s),
            Err(e) => {
                first_error.get_or_insert(e.to_string());
            }
        }
    }
    rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| first_error.unwrap_or_else(|| e.to_string()))
}

/// Small random generator seeded from the clock, standing in for Songbird's srand(time)/rand().
pub struct Rng(u64);

impl Rng {
    pub fn from_time() -> Rng {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x2545F4914F6CDD1D);
        Rng(seed | 1)
    }
    pub fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}

/// Fisher–Yates over 0..n, as sbMediacoreShuffleSequenceGenerator::Init (0x99d0):
/// for i in 1..n swap(i, rand() % (i + 1)).
pub fn shuffle_order(n: usize, rng: &mut Rng) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    for i in 1..n {
        let j = rng.below(i + 1);
        v.swap(i, j);
    }
    v
}

/// Which list the sequencer is playing from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Library,
    Playlist(u64),
    /// The web playlist of a browser tab.
    Web(u64),
}

pub struct Sequencer {
    /// The view being played (track ids, in the order shown when playback started).
    pub view: Vec<u64>,
    pub source: Source,
    /// Play order: indexes into `view`.
    order: Vec<usize>,
    /// Position in `order`.
    pos: usize,
    pub shuffle: bool,
    /// 0 none, 1 one, 2 all.
    pub repeat: u8,
    pub queue: VecDeque<u64>,
    /// A queued track is playing (outside the view).
    pub playing_from_queue: Option<u64>,
    rng: Rng,
}

impl Sequencer {
    pub fn new(shuffle: bool, repeat: u8) -> Sequencer {
        Sequencer {
            view: vec![],
            source: Source::Library,
            order: vec![],
            pos: 0,
            shuffle,
            repeat,
            queue: VecDeque::new(),
            playing_from_queue: None,
            rng: Rng::from_time(),
        }
    }

    pub fn current(&self) -> Option<u64> {
        if let Some(id) = self.playing_from_queue {
            return Some(id);
        }
        self.order.get(self.pos).and_then(|&i| self.view.get(i)).copied()
    }

    /// Starts playing `view` at `start` (an index into the view).
    pub fn play_view(&mut self, view: Vec<u64>, source: Source, start: usize) -> Option<u64> {
        self.view = view;
        self.source = source;
        self.playing_from_queue = None;
        self.rebuild_order(Some(start));
        self.current()
    }

    /// Recomputes the play order keeping `keep` (an index into the view) as the current item.
    /// With shuffle on, the kept item is moved to the front of the new order (estimated).
    fn rebuild_order(&mut self, keep: Option<usize>) {
        let n = self.view.len();
        if self.shuffle {
            self.order = shuffle_order(n, &mut self.rng);
            if let Some(k) = keep {
                if let Some(p) = self.order.iter().position(|&i| i == k) {
                    self.order.swap(0, p);
                }
            }
            self.pos = 0;
        } else {
            self.order = (0..n).collect();
            self.pos = keep.unwrap_or(0).min(n.saturating_sub(1));
        }
    }

    pub fn set_shuffle(&mut self, on: bool) {
        if self.shuffle == on {
            return;
        }
        self.shuffle = on;
        let keep = self.order.get(self.pos).copied();
        self.rebuild_order(keep);
    }

    /// Repeat button: none(0) → all(2) → one(1) → none(0) (playerControls.xml onRepeat).
    pub fn cycle_repeat(&mut self) {
        self.repeat = match self.repeat {
            0 => 2,
            2 => 1,
            _ => 0,
        };
    }

    /// Moves on. `user` is true for the Next button, false when a song ended.
    /// Returns the track to play, or None to stop.
    pub fn next(&mut self, user: bool) -> Option<u64> {
        if !user && self.repeat == 1 {
            if let Some(id) = self.current() {
                return Some(id);
            }
        }
        if let Some(id) = self.queue.pop_front() {
            self.playing_from_queue = Some(id);
            return Some(id);
        }
        if self.playing_from_queue.take().is_some() {
            // Back to the view: carry on after the item that was current before the queue.
            return self.advance();
        }
        self.advance()
    }

    fn advance(&mut self) -> Option<u64> {
        let n = self.order.len();
        if n == 0 {
            return None;
        }
        if self.pos + 1 < n {
            self.pos += 1;
            self.current()
        } else if self.repeat == 2 {
            self.pos = 0;
            self.current()
        } else {
            None
        }
    }

    /// Steps back one item (no restart threshold was found in the sequencer).
    pub fn previous(&mut self) -> Option<u64> {
        self.playing_from_queue = None;
        let n = self.order.len();
        if n == 0 {
            return None;
        }
        if self.pos > 0 {
            self.pos -= 1;
        } else if self.repeat == 2 {
            self.pos = n - 1;
        }
        self.current()
    }

    pub fn has_previous(&self) -> bool {
        !self.order.is_empty() && (self.pos > 0 || self.repeat == 2)
    }

    pub fn has_next(&self) -> bool {
        !self.queue.is_empty() || (!self.order.is_empty() && (self.pos + 1 < self.order.len() || self.repeat == 2))
    }

    /// Drops removed tracks from the view and queue.
    pub fn forget(&mut self, ids: &std::collections::HashSet<u64>) {
        self.queue.retain(|id| !ids.contains(id));
        if self.view.iter().any(|id| ids.contains(id)) {
            let cur = self.current();
            self.view.retain(|id| !ids.contains(id));
            let keep = cur.and_then(|c| self.view.iter().position(|&v| v == c));
            self.rebuild_order(keep);
        }
    }
}

/// "m:ss" or "h:mm:ss", like the faceplate and the Time column.
pub fn fmt_time(ms: u64) -> String {
    let s = ms / 1000;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// sbTimeFormatter.formatSingle: one decimal (floored) of hours or minutes, else seconds.
pub fn fmt_single(secs: u64) -> String {
    let t = secs as f64;
    if t >= 3600.0 {
        let v = (t * 10.0 / 3600.0).floor() / 10.0;
        if v == 1.0 { "1 hour".into() } else { format!("{v} hours") }
    } else if t >= 60.0 {
        let v = (t * 10.0 / 60.0).floor() / 10.0;
        if v == 1.0 { "1 minute".into() } else { format!("{v} minutes") }
    } else if secs == 1 {
        "1 second".into()
    } else {
        format!("{secs} seconds")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shuffle_is_permutation() {
        let mut r = Rng::from_time();
        let mut v = shuffle_order(50, &mut r);
        v.sort();
        assert_eq!(v, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn repeat_cycle() {
        let mut s = Sequencer::new(false, 0);
        s.cycle_repeat();
        assert_eq!(s.repeat, 2);
        s.cycle_repeat();
        assert_eq!(s.repeat, 1);
        s.cycle_repeat();
        assert_eq!(s.repeat, 0);
    }

    #[test]
    fn next_and_wrap() {
        let mut s = Sequencer::new(false, 0);
        s.play_view(vec![10, 11, 12], Source::Library, 1);
        assert_eq!(s.next(true), Some(12));
        assert_eq!(s.next(false), None);
        s.repeat = 2;
        assert_eq!(s.next(false), Some(10));
        s.repeat = 1;
        assert_eq!(s.next(false), Some(10));
        assert_eq!(s.next(true), Some(11));
        s.queue.push_back(99);
        assert_eq!(s.next(true), Some(99));
        assert_eq!(s.next(true), Some(12));
    }

    #[test]
    fn formats() {
        assert_eq!(fmt_time(66_000), "1:06");
        assert_eq!(fmt_time(3_725_000), "1:02:05");
        assert_eq!(fmt_single(66), "1.1 minutes");
        assert_eq!(fmt_single(32), "32 seconds");
    }
}

#[cfg(test)]
mod file_tests {
    #[test]
    fn reads_and_decodes_a_wav() {
        let path = std::env::temp_dir().join("songbird_player_test.wav");
        let rate = 8000u32;
        let samples: Vec<i16> = (0..rate * 2).map(|i| ((i as f32 * 0.3).sin() * 8000.0) as i16).collect();
        let mut b = Vec::new();
        let data_len = (samples.len() * 2) as u32;
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 2).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for s in samples {
            b.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write(&path, b).unwrap();
        let t = crate::library::Track::from_file(&path, 1).expect("track");
        assert_eq!(t.kind, "WAV");
        assert!((t.duration_ms as i64 - 2000).abs() < 50, "duration {}", t.duration_ms);
        let dec = rodio::Decoder::try_from(std::fs::File::open(&path).unwrap()).expect("decode");
        assert!(rodio::Source::total_duration(&dec).is_some());
    }
}

#[cfg(test)]
mod device_tests {
    /// Tries the ways of opening the sound output and prints what each says.
    #[test]
    #[ignore]
    fn probe_sound_output() {
        use rodio::cpal::traits::{DeviceTrait, HostTrait};
        let host = rodio::cpal::default_host();
        let dev = host.default_output_device().expect("no default device");
        eprintln!("default device: {:?}", dev.description().map(|d| d.name().to_string()));
        eprintln!("default config: {:?}", dev.default_output_config());
        let a = rodio::DeviceSinkBuilder::open_default_sink();
        eprintln!("open_default_sink: {:?}", a.as_ref().err());
        let b = rodio::DeviceSinkBuilder::from_default_device()
            .map(|x| x.with_buffer_size(rodio::cpal::BufferSize::Default))
            .and_then(|x| x.open_stream());
        eprintln!("default buffer: {:?}", b.as_ref().err());
        let c = super::open_output();
        eprintln!("open_output: {:?}", c.as_ref().err());
        assert!(c.is_ok());
    }
}
