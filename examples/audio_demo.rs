//! Audio playback demo using sine-wave tones.
//!
//! Run with: cargo run --example audio_demo --features ext-audio

use burin::audio::{play_sound_bytes, AudioPlayer};
use burin::core::{Compositor, Widget};
use burin::platform::{App, WindowConfig};
use burin::style::{Color, Padding, Styled};
use burin::widgets::display::Text;
use burin::widgets::input::Button;
use burin::widgets::layout::{HStack, VStack};
use std::rc::Rc;

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

fn main() {
    let config = WindowConfig {
        title: "Audio Demo".into(),
        width: 400.0,
        height: 300.0,
        ..WindowConfig::auto_theme()
    };

    App::new()
        .window(config, Compositor::new(move |_scope| demo()))
        .run()
        .unwrap();
}

fn demo() -> impl Widget {
    let player = Rc::new(AudioPlayer::new().expect("no audio device"));

    let c4 = sine_wav_bytes(261.63, 0.5, 44100);
    let e4 = sine_wav_bytes(329.63, 0.5, 44100);
    let g4 = sine_wav_bytes(392.00, 0.5, 44100);

    let bytes_c = c4.clone();
    let btn_c = Button::new("C4").on_click(move || {
        play_sound_bytes(&bytes_c).ok();
    });

    let player_e = player.clone();
    let bytes_e = e4.clone();
    let btn_e = Button::new("E4").on_click(move || {
        player_e.stop();
        player_e.open_bytes(&bytes_e).ok();
        player_e.play();
    });

    let player_g = player.clone();
    let bytes_g = g4.clone();
    let btn_g = Button::new("G4").on_click(move || {
        player_g.stop();
        player_g.open_bytes(&bytes_g).ok();
        player_g.play();
    });

    let player2 = player.clone();
    let btn_stop = Button::new("Stop").on_click(move || player2.stop());

    let player3 = player.clone();
    let btn_pause = Button::new("Pause").on_click(move || player3.pause());

    let player4 = player.clone();
    let btn_resume = Button::new("Resume").on_click(move || player4.play());

    let player5 = player.clone();
    let btn_vol_up = Button::new("Vol+").on_click(move || {
        let v = (player5.volume() + 0.1).min(1.0);
        player5.set_volume(v);
    });

    let player6 = player;
    let btn_vol_down = Button::new("Vol-").on_click(move || {
        let v = (player6.volume() - 0.1).max(0.0);
        player6.set_volume(v);
    });

    VStack::new()
        .gap(12.0)
        .padding(Padding::all(20.0))
        .push(Text::new("Audio Demo").font_size(24.0).color(Color::WHITE))
        .push(HStack::new().gap(8.0).push(btn_c).push(btn_e).push(btn_g))
        .push(Text::new("C4=fire-and-forget | E4,G4=AudioPlayer"))
        .push(
            HStack::new()
                .gap(8.0)
                .push(btn_stop)
                .push(btn_pause)
                .push(btn_resume),
        )
        .push(HStack::new().gap(8.0).push(btn_vol_down).push(btn_vol_up))
}
