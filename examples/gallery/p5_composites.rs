use auralis_signal::Signal;
use burin::core::{Compositor, Widget};
use burin::style::Styled;
use burin::widgets::composite::{TabBar, TabPanel};
use burin::widgets::display::Text;
use burin::widgets::layout::*;

#[cfg(feature = "ext-audio")]
use std::rc::Rc;

use super::section_title;

#[cfg(feature = "ext-audio")]
use burin::audio::AudioPlayer;
#[cfg(feature = "ext-audio")]
use burin::widgets::composite::AudioPlayerWidget;
#[cfg(feature = "ext-audio")]
use burin::widgets::input::Button;
#[cfg(all(feature = "ext-audio", feature = "file-dialog"))]
use burin::widgets::input::{FilePickerButton, FilePickerMode};

#[cfg(feature = "ext-audio")]
fn sine_wav_bytes(freq: f32, duration_secs: f32, sample_rate: u32) -> Vec<u8> {
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let data_size = num_samples * 2;
    let mut wav = Vec::with_capacity(44 + data_size);

    wav.extend(b"RIFF");
    wav.extend(&(36 + data_size as u32).to_le_bytes());
    wav.extend(b"WAVE");
    wav.extend(b"fmt ");
    wav.extend(&16u32.to_le_bytes());
    wav.extend(&1u16.to_le_bytes());
    wav.extend(&1u16.to_le_bytes());
    wav.extend(&sample_rate.to_le_bytes());
    wav.extend(&(sample_rate * 2).to_le_bytes());
    wav.extend(&2u16.to_le_bytes());
    wav.extend(&16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(&(data_size as u32).to_le_bytes());

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = (t * freq * 2.0 * std::f32::consts::PI).sin();
        let sample_i16 = (sample * 0.3 * 32767.0) as i16;
        wav.extend(&sample_i16.to_le_bytes());
    }
    wav
}

#[cfg(feature = "ext-audio")]
pub fn audio_player_section() -> impl Widget {
    Compositor::new(|_scope| {
        let player = Rc::new(AudioPlayer::new().expect("No audio device"));
        let track_title = Signal::new(String::from("No track loaded"));
        let track_artist = Signal::new(String::from("Synthesized"));

        let c4 = sine_wav_bytes(261.63, 3.0, 44100);
        let e4 = sine_wav_bytes(329.63, 3.0, 44100);
        let g4 = sine_wav_bytes(392.00, 3.0, 44100);
        let c5 = sine_wav_bytes(523.25, 3.0, 44100);

        let player_c4 = player.clone();
        let title_c4 = track_title.clone();
        let btn_c4 = Button::new("C4").on_click(move || {
            player_c4.stop();
            player_c4.open_bytes(&c4).ok();
            player_c4.play();
            title_c4.set(String::from("C4 (261 Hz) — Piano Note"));
        });

        let player_e4 = player.clone();
        let title_e4 = track_title.clone();
        let btn_e4 = Button::new("E4").on_click(move || {
            player_e4.stop();
            player_e4.open_bytes(&e4).ok();
            player_e4.play();
            title_e4.set(String::from("E4 (329 Hz) — Piano Note"));
        });

        let player_g4 = player.clone();
        let title_g4 = track_title.clone();
        let btn_g4 = Button::new("G4").on_click(move || {
            player_g4.stop();
            player_g4.open_bytes(&g4).ok();
            player_g4.play();
            title_g4.set(String::from("G4 (392 Hz) — Piano Note"));
        });

        let player_c5 = player.clone();
        let title_c5 = track_title.clone();
        let btn_c5 = Button::new("C5").on_click(move || {
            player_c5.stop();
            player_c5.open_bytes(&c5).ok();
            player_c5.play();
            title_c5.set(String::from("C5 (523 Hz) — Piano Note"));
        });

        let widget_player = player.clone();
        let widget_title = track_title.clone();
        let widget_artist = track_artist.clone();

        VStack::new().gap(8.0)
            .push(section_title("AudioPlayer  G6"))
            .push(Text::new("Full-featured audio player with seek, volume, keyboard shortcuts. Pick a file or load synthetic tones.").font_size(12.0))
            .push(
                HStack::new().gap(8.0)
                    .push({
                        let player_open = player.clone();
                        let title_open = track_title.clone();
                        let artist_open = track_artist.clone();
                        #[cfg(feature = "file-dialog")]
                        let picker = FilePickerButton::new("Open File...")
                            .mode(FilePickerMode::Open)
                            .filter("Audio Files", &["mp3", "wav", "flac", "ogg", "aac", "m4a"])
                            .on_file_selected(move |file| {
                                player_open.stop();
                                if let Ok(meta) = player_open.open(&file.path) {
                                    player_open.play();
                                    let name = file.name().unwrap_or("Unknown");
                                    title_open.set(name.to_string());
                                    if let Some(d) = meta.duration {
                                        let secs = d.as_secs();
                                        artist_open.set(format!(
                                            "{} kHz, {}ch, {:02}:{:02}",
                                            meta.sample_rate,
                                            meta.channels,
                                            secs / 60,
                                            secs % 60
                                        ));
                                    }
                                }
                            });
                        #[cfg(not(feature = "file-dialog"))]
                        let picker = Button::new("Open File (unavailable)")
                            .disabled();
                        picker
                    })
                    .push(btn_c4)
                    .push(btn_e4)
                    .push(btn_g4)
                    .push(btn_c5)
            )
            .push(
                AudioPlayerWidget::new(widget_player)
                    .track_title(widget_title)
                    .track_artist(widget_artist)
            )
            .push(Text::new("Keyboard: Space (play/pause), ← → (seek ±5s), ↑ ↓ (volume), Home/End, M (mute)").font_size(11.0))
    })
}

pub fn tab_bar_section() -> impl Widget {
    Compositor::new(|_scope| {
        let active = Signal::new(0usize);

        VStack::new()
            .gap(8.0)
            .push(section_title("TabBar & TabPanel  G6"))
            .push(
                Text::new("Pill-style tab bar. Click or use ← → Home End to navigate.")
                    .font_size(12.0),
            )
            .push(
                TabBar::new(active.clone())
                    .tab("General")
                    .tab("Advanced")
                    .tab("About"),
            )
            .push(TabPanel::new(
                0,
                active.clone(),
                Text::new("General tab content: Welcome to Auralis UI!").font_size(12.0),
            ))
            .push(TabPanel::new(
                1,
                active.clone(),
                Text::new(
                    "Advanced tab content: CPU/GPU rendering, theming, and performance tuning.",
                )
                .font_size(12.0),
            ))
            .push(TabPanel::new(
                2,
                active.clone(),
                Text::new("About tab content: Auralis UI v0.1 — MIT License").font_size(12.0),
            ))
    })
}
