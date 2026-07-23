//! Audio playback via rodio (feature `ext-audio`).
//!
//! Provides:
//! - [`play_sound`] / [`play_sound_bytes`] — one-shot fire-and-forget
//! - [`AudioPlayer`] — controlled playback: open → play/pause/stop, seek, speed, volume, loop
//! - [`AudioTrackMeta`] — decoded audio metadata
//! - [`PlaybackState`] — observable playback lifecycle
//! - [`LoopMode`] — repeat mode (None, One)

use rodio::decoder::DecoderBuilder;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player};
use std::cell::RefCell;
use std::io::{BufReader, Cursor};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

thread_local! {
    static ENGINE: RefCell<Option<AudioEngine>> = const { RefCell::new(None) };
}

struct AudioEngine {
    _sink: MixerDeviceSink,
}

impl AudioEngine {
    fn try_init() -> Result<(), AudioError> {
        ENGINE.with(|e| {
            if e.borrow().is_some() {
                return Ok(());
            }
            let mut sink =
                DeviceSinkBuilder::open_default_sink().map_err(|_| AudioError::NoDevice)?;
            sink.log_on_drop(false);
            *e.borrow_mut() = Some(AudioEngine { _sink: sink });
            Ok(())
        })
    }

    fn with_mixer<F, T>(f: F) -> Result<T, AudioError>
    where
        F: FnOnce(&rodio::mixer::Mixer) -> T,
    {
        Self::try_init()?;
        ENGINE.with(|e| {
            let engine = e.borrow();
            let engine = engine.as_ref().ok_or(AudioError::NoDevice)?;
            Ok(f(engine._sink.mixer()))
        })
    }
}

// ── Error types ─────────────────────────────────────────────────

/// Audio playback errors.
#[derive(Error, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum AudioError {
    /// No audio output device available.
    #[error("No audio output device available")]
    NoDevice,

    /// Failed to decode audio data.
    #[error("Failed to decode audio: {0}")]
    Decode(String),

    /// I/O error reading audio file.
    #[error("IO error: {0}")]
    #[cfg_attr(feature = "devtools", serde(skip))]
    Io(#[from] std::io::Error),

    /// Seeking is not supported for the current source.
    #[error("Seek not supported")]
    SeekNotSupported,

    /// Seek error from the decoder.
    #[error("Seek error: {0}")]
    Seek(String),
}

// ── Fire-and-forget helpers ─────────────────────────────────────

/// Play a sound file from disk (fire-and-forget).
///
/// Supported formats depend on the available decoders in the `rodio` / `symphonia`
/// dependency tree (WAV, MP3, FLAC, Ogg Vorbis, AAC, etc.).
pub fn play_sound(path: impl AsRef<Path>) -> Result<(), AudioError> {
    let file = std::fs::File::open(path.as_ref())?;
    let source =
        rodio::Decoder::new(BufReader::new(file)).map_err(|e| AudioError::Decode(e.to_string()))?;
    AudioEngine::with_mixer(|mixer| mixer.add(source))
}

/// Play raw encoded audio bytes (fire-and-forget).
///
/// The bytes must be a complete encoded audio file (WAV, MP3, etc.).
pub fn play_sound_bytes(data: &[u8]) -> Result<(), AudioError> {
    let cursor = Cursor::new(data.to_vec());
    let source = rodio::Decoder::new(cursor).map_err(|e| AudioError::Decode(e.to_string()))?;
    AudioEngine::with_mixer(|mixer| mixer.add(source))
}

// ── Metadata ────────────────────────────────────────────────────

/// Metadata extracted from a decoded audio source.
#[derive(Clone, Debug)]
pub struct AudioTrackMeta {
    /// Total duration of the track, if known.
    pub duration: Option<Duration>,
    /// Sample rate in Hz (e.g. 44100, 48000).
    pub sample_rate: u32,
    /// Number of channels (1 = mono, 2 = stereo).
    pub channels: u16,
}

impl AudioTrackMeta {
    fn from_source(source: &impl rodio::Source) -> Self {
        Self {
            duration: source.total_duration(),
            sample_rate: source.sample_rate().get(),
            channels: source.channels().get(),
        }
    }
}

// ── Playback state & loop mode ──────────────────────────────────

/// Observable playback lifecycle state.
#[derive(Clone, PartialEq, Debug)]
pub enum PlaybackState {
    /// No track loaded, or queue is empty and playback ended.
    Idle,
    /// Audio is actively playing.
    Playing,
    /// Playback is paused (track is loaded, position preserved).
    Paused,
    /// Playback stopped (position reset).
    Stopped,
}

/// Repeat mode for the player.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LoopMode {
    /// Play once and stop.
    None,
    /// Loop the current track.
    One,
}

// ── AudioPlayer ─────────────────────────────────────────────────

/// A controllable audio player for playback of audio files.
///
/// Use [`open`](AudioPlayer::open) / [`open_bytes`](AudioPlayer::open_bytes)
/// to load a track, then [`play`](AudioPlayer::play) to start.
///
/// Unlike the fire-and-forget [`play_sound`] / [`play_sound_bytes`] functions,
/// an `AudioPlayer` allows seeking, position tracking, volume/mute, speed
/// control, loop modes, and queue management.
///
/// # Example
///
/// ```no_run
/// use burin::audio::{AudioPlayer, LoopMode};
///
/// let player = AudioPlayer::new().expect("no audio device");
/// player.open("music.mp3").ok();
/// player.set_volume(0.8);
/// player.set_loop_mode(LoopMode::One);
/// player.play();
///
/// // later:
/// let pos = player.get_pos();
/// player.try_seek(std::time::Duration::from_secs(30)).ok();
/// ```
pub struct AudioPlayer {
    player: Arc<Player>,
    current_meta: RefCell<Option<AudioTrackMeta>>,
    pre_mute_volume: RefCell<f32>,
    muted: RefCell<bool>,
    playback_state: RefCell<PlaybackState>,
    loop_mode: RefCell<LoopMode>,
    source_bytes: RefCell<Option<Vec<u8>>>,
    source_path: RefCell<Option<String>>,
}

impl AudioPlayer {
    /// Create a new audio player attached to the default output device.
    ///
    /// Returns [`AudioError::NoDevice`] if no audio output is available.
    pub fn new() -> Result<Self, AudioError> {
        AudioEngine::try_init()?;
        ENGINE.with(|e| {
            let engine = e.borrow();
            let engine = engine.as_ref().ok_or(AudioError::NoDevice)?;
            let player = Arc::new(Player::connect_new(engine._sink.mixer()));
            Ok(Self {
                player,
                current_meta: RefCell::new(None),
                pre_mute_volume: RefCell::new(1.0),
                muted: RefCell::new(false),
                playback_state: RefCell::new(PlaybackState::Idle),
                loop_mode: RefCell::new(LoopMode::None),
                source_bytes: RefCell::new(None),
                source_path: RefCell::new(None),
            })
        })
    }

    // ── Load tracks ──────────────────────────────────────────

    /// Load a sound file and decode its metadata.
    ///
    /// Does not start playback — call [`play`](AudioPlayer::play) afterwards.
    /// Multiple files can be loaded; they play in sequence.
    pub fn open(&self, path: impl AsRef<Path>) -> Result<AudioTrackMeta, AudioError> {
        self.player.clear();
        let file = std::fs::File::open(path.as_ref())?;
        let source =
            rodio::Decoder::try_from(file).map_err(|e| AudioError::Decode(e.to_string()))?;
        let meta = AudioTrackMeta::from_source(&source);
        *self.source_path.borrow_mut() = Some(path.as_ref().to_string_lossy().to_string());
        *self.source_bytes.borrow_mut() = None;
        self.player.append(source);
        *self.current_meta.borrow_mut() = Some(meta.clone());
        Ok(meta)
    }

    /// Load raw encoded audio bytes and decode their metadata.
    ///
    /// Does not start playback — call [`play`](AudioPlayer::play) afterwards.
    pub fn open_bytes(&self, data: &[u8]) -> Result<AudioTrackMeta, AudioError> {
        self.player.clear();
        let data_vec = data.to_vec();
        let byte_len = data_vec.len() as u64;
        let cursor = Cursor::new(data_vec.clone());
        let source = DecoderBuilder::new()
            .with_data(cursor)
            .with_byte_len(byte_len)
            .build()
            .map_err(|e| AudioError::Decode(e.to_string()))?;
        let meta = AudioTrackMeta::from_source(&source);
        *self.source_bytes.borrow_mut() = Some(data_vec);
        *self.source_path.borrow_mut() = None;
        self.player.append(source);
        *self.current_meta.borrow_mut() = Some(meta.clone());
        Ok(meta)
    }

    /// Re-load the last opened source into the player (for loop / stop-then-play).
    ///
    /// Returns an error if no source was previously loaded.
    pub fn reload(&self) -> Result<(), AudioError> {
        if let Some(ref path) = *self.source_path.borrow() {
            let file = std::fs::File::open(path).map_err(|e| AudioError::Io(e))?;
            let source =
                rodio::Decoder::try_from(file).map_err(|e| AudioError::Decode(e.to_string()))?;
            self.player.append(source);
            Ok(())
        } else if let Some(ref data) = *self.source_bytes.borrow() {
            let byte_len = data.len() as u64;
            let cursor = Cursor::new(data.clone());
            let source = DecoderBuilder::new()
                .with_data(cursor)
                .with_byte_len(byte_len)
                .build()
                .map_err(|e| AudioError::Decode(e.to_string()))?;
            self.player.append(source);
            Ok(())
        } else {
            Err(AudioError::Decode("No source to reload".into()))
        }
    }

    /// Returns the metadata for the currently loaded track, if any.
    pub fn current_meta(&self) -> Option<AudioTrackMeta> {
        self.current_meta.borrow().clone()
    }

    // ── Position & seeking ───────────────────────────────────

    /// Returns the current playback position.
    ///
    /// Accounts for playback speed. If speed is 2× and 5 wall-clock seconds
    /// have elapsed, this returns 10 seconds.
    pub fn get_pos(&self) -> Duration {
        self.player.get_pos()
    }

    /// Seek to a position in the current track.
    ///
    /// Works during playback and while paused/stopped. If the position is
    /// beyond the track duration, it saturates at the end.
    pub fn try_seek(&self, pos: Duration) -> Result<(), AudioError> {
        self.player
            .try_seek(pos)
            .map_err(|e| AudioError::Seek(e.to_string()))
    }

    /// Returns the total duration of the current track, if known.
    pub fn total_duration(&self) -> Option<Duration> {
        self.current_meta.borrow().as_ref().and_then(|m| m.duration)
    }

    // ── Speed ────────────────────────────────────────────────

    /// Set playback speed. 1.0 = normal, 2.0 = double speed.
    ///
    /// Common values: 0.5, 0.75, 1.0, 1.25, 1.5, 2.0.
    /// Values ≤ 0 are clamped to a small positive.
    pub fn set_speed(&self, speed: f32) {
        self.player.set_speed(speed.max(0.1));
    }

    /// Get current playback speed.
    pub fn speed(&self) -> f32 {
        self.player.speed()
    }

    // ── Volume ───────────────────────────────────────────────

    /// Set volume (0.0 = silent, 1.0 = normal).
    /// If muted, this un-mutes and sets the new volume.
    pub fn set_volume(&self, volume: f32) {
        let v = volume.clamp(0.0, 1.0);
        self.player.set_volume(v);
        if *self.muted.borrow() && v > 0.0 {
            *self.muted.borrow_mut() = false;
        }
        *self.pre_mute_volume.borrow_mut() = v;
    }

    /// Get current volume (post-mute). Returns 0.0 if muted.
    pub fn volume(&self) -> f32 {
        if *self.muted.borrow() {
            0.0
        } else {
            self.player.volume()
        }
    }

    /// Get the actual volume level ignoring mute state.
    pub fn raw_volume(&self) -> f32 {
        self.player.volume()
    }

    /// Mute or unmute.
    ///
    /// When muting, the current volume is remembered and restored on unmute.
    pub fn set_muted(&self, muted: bool) {
        if muted == *self.muted.borrow() {
            return;
        }
        if muted {
            *self.pre_mute_volume.borrow_mut() = self.player.volume();
            self.player.set_volume(0.0);
        } else {
            self.player.set_volume(*self.pre_mute_volume.borrow());
        }
        *self.muted.borrow_mut() = muted;
    }

    /// Toggle mute state.
    pub fn toggle_mute(&self) {
        let currently_muted = *self.muted.borrow();
        self.set_muted(!currently_muted);
    }

    /// Returns true if currently muted.
    pub fn is_muted(&self) -> bool {
        *self.muted.borrow()
    }

    // ── Playback control ─────────────────────────────────────

    /// Start or resume playback.
    ///
    /// If a track was loaded via [`open`](AudioPlayer::open) it begins
    /// playing. If playback was paused, it resumes from the paused position.
    pub fn play(&self) {
        self.player.play();
        *self.playback_state.borrow_mut() = PlaybackState::Playing;
    }

    /// Pause playback. Call [`play`](AudioPlayer::play) to resume.
    pub fn pause(&self) {
        self.player.pause();
        *self.playback_state.borrow_mut() = PlaybackState::Paused;
    }

    /// Stop playback and seek back to the beginning.
    ///
    /// The track remains loaded and can be restarted with [`play`](AudioPlayer::play).
    pub fn stop(&self) {
        self.player.pause();
        let _ = self.player.try_seek(Duration::ZERO);
        *self.playback_state.borrow_mut() = PlaybackState::Stopped;
    }

    /// Returns true if playback is currently active (playing).
    pub fn is_playing(&self) -> bool {
        *self.playback_state.borrow() == PlaybackState::Playing
    }

    /// Returns true if playback is currently paused.
    pub fn is_paused(&self) -> bool {
        *self.playback_state.borrow() == PlaybackState::Paused
    }

    // ── Loop mode ────────────────────────────────────────────

    /// Returns the current loop mode.
    pub fn loop_mode(&self) -> LoopMode {
        *self.loop_mode.borrow()
    }

    /// Set the loop mode.
    ///
    /// - [`LoopMode::None`] — play once and stop.
    /// - [`LoopMode::One`] — restart the current track when it ends.
    pub fn set_loop_mode(&self, mode: LoopMode) {
        *self.loop_mode.borrow_mut() = mode;
    }

    // ── Queue ────────────────────────────────────────────────

    /// Returns true if the queue is empty (all sounds have finished).
    pub fn is_empty(&self) -> bool {
        self.player.empty()
    }

    /// Returns the number of sounds still queued or currently playing.
    pub fn len(&self) -> usize {
        self.player.len()
    }

    /// Skip the current sound and advance to the next in queue.
    pub fn skip_one(&self) {
        self.player.skip_one();
    }

    /// Clears the playback queue without stopping the current sound.
    /// Currently playing sound continues; no subsequent sounds will play.
    pub fn clear_queue(&self) {
        self.player.clear();
    }

    /// Returns a reference-counted handle to the underlying rodio `Player`,
    /// suitable for position polling from a background thread.
    pub fn inner_player(&self) -> Arc<Player> {
        self.player.clone()
    }

    /// Returns the current playback state.
    pub fn state(&self) -> PlaybackState {
        self.playback_state.borrow().clone()
    }

    /// Check whether the current track has ended (queue empty after playing).
    /// Call this periodically to detect end-of-track.
    /// Updates state to Idle if ended, to Paused if paused while not empty.
    pub fn poll_state(&self) -> PlaybackState {
        let was_playing = *self.playback_state.borrow() == PlaybackState::Playing;
        if was_playing && self.player.empty() {
            *self.playback_state.borrow_mut() = PlaybackState::Idle;
            return PlaybackState::Idle;
        }
        if *self.playback_state.borrow() != PlaybackState::Paused
            && self.player.empty()
            && !was_playing
        {
            *self.playback_state.borrow_mut() = PlaybackState::Idle;
            return PlaybackState::Idle;
        }
        self.playback_state.borrow().clone()
    }
}
