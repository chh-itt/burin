use std::rc::Rc;
use std::time::Duration;

use auralis_signal::Signal;

use crate::audio::{AudioPlayer, LoopMode};
use crate::core::clock;
use crate::core::config::EventHandler;
use crate::core::context::MountContext;
use crate::core::element::ElementId;
use crate::core::scheduler;
use crate::core::widget::Widget;
use crate::ecs::components;
use crate::event::Key;
use crate::style::{Padding, StyleRefinement, Styled};
use crate::widgets::display::Text;
use crate::widgets::input::{Button, Slider};

const PLAY_ICON: &str = "\u{25B6}";
const PAUSE_ICON: &str = "\u{23F8}";
const STOP_ICON: &str = "\u{23F9}";
const MUTE_ICON: &str = "\u{1F507}";
const VOLUME_ICON: &str = "\u{1F50A}";
const LOOP_NONE_ICON: &str = "\u{1F502}";
const LOOP_ONE_ICON: &str = "\u{1F501}";

/// An audio player widget with playback controls.
pub struct AudioPlayerWidget {
    player: Rc<AudioPlayer>,
    track_title: Option<Signal<String>>,
    track_artist: Option<Signal<String>>,
    on_ended: Option<Rc<dyn Fn()>>,
    show_track_info: bool,
    show_volume: bool,
    show_loop: bool,
    style: StyleRefinement,
}

impl AudioPlayerWidget {
    pub fn new(player: Rc<AudioPlayer>) -> Self {
        Self {
            player,
            track_title: None,
            track_artist: None,
            on_ended: None,
            show_track_info: true,
            show_volume: true,
            show_loop: true,
            style: StyleRefinement::default(),
        }
    }

    pub fn track_title(mut self, sig: Signal<String>) -> Self {
        self.track_title = Some(sig);
        self
    }

    pub fn track_artist(mut self, sig: Signal<String>) -> Self {
        self.track_artist = Some(sig);
        self
    }

    pub fn show_track_info(mut self, show: bool) -> Self {
        self.show_track_info = show;
        self
    }

    pub fn show_volume(mut self, show: bool) -> Self {
        self.show_volume = show;
        self
    }

    pub fn show_loop(mut self, show: bool) -> Self {
        self.show_loop = show;
        self
    }

    pub fn on_ended(mut self, f: impl Fn() + 'static) -> Self {
        self.on_ended = Some(Rc::new(f));
        self
    }
}

impl Styled for AudioPlayerWidget {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for AudioPlayerWidget {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let player = self.player.clone();
        let on_ended = self.on_ended.clone();
        let show_volume = self.show_volume;
        let show_track_info = self.show_track_info;
        let show_loop = self.show_loop;

        let bg = self
            .style
            .background
            .unwrap_or(ctx.theme.scheme.surface_container);

        let position_pct = Signal::new(0.0f32);
        let duration = Signal::new(0.0f64);
        let is_playing = Signal::new(false);
        let volume = Signal::new(player.raw_volume());
        let is_muted = Signal::new(player.is_muted());
        let current_time_label = Signal::new(String::from("0:00"));
        let total_time_label = Signal::new(String::from("0:00"));
        let play_icon = Signal::new(String::from(PLAY_ICON));
        let mute_icon = Signal::new(String::from(if player.is_muted() {
            MUTE_ICON
        } else {
            VOLUME_ICON
        }));
        let loop_icon = Signal::new(String::from(match player.loop_mode() {
            LoopMode::None => LOOP_NONE_ICON,
            LoopMode::One => LOOP_ONE_ICON,
        }));

        let container_id = ctx.arena.allocate();
        ctx.preallocate(container_id, self.component_mask());
        {
            let Some(el) = ctx.arena.get_mut(container_id) else {
                return container_id;
            };
            el.set_layout_direction(crate::core::LayoutDirection::Vertical);
            el.set_gap(8.0);
            el.set_padding(Padding::all(12.0));
            el.set_preferred_width(Some(400.0));
            el.set_background(bg);
            el.set_corner_radii(crate::style::CornerRadii::all(8.0));
            el.set_focusable(true);
        }

        // ── Track info row ─────────────────────────────────────
        if show_track_info {
            let info_id = ctx.arena.allocate();
            {
                let Some(el) = ctx.arena.get_mut(info_id) else {
                    return container_id;
                };
                el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
                el.set_gap(8.0);
                el.set_alignment(crate::style::Alignment::Center);
            }

            if let Some(ref title_sig) = self.track_title {
                let title_widget = Box::new(
                    Text::new(title_sig.read())
                        .bind(title_sig.clone())
                        .font_size(14.0)
                        .font_weight(600),
                );
                let title_id = title_widget.mount_box(&mut ctx.child_with_events(info_id));
                ctx.arena.add_child(info_id, title_id);
            }

            if let Some(ref artist_sig) = self.track_artist {
                let artist_widget = Box::new(
                    Text::new(artist_sig.read())
                        .bind(artist_sig.clone())
                        .font_size(12.0)
                        .color(ctx.theme.scheme.on_surface_variant),
                );
                let artist_id = artist_widget.mount_box(&mut ctx.child_with_events(info_id));
                ctx.arena.add_child(info_id, artist_id);
            }

            ctx.arena.add_child(container_id, info_id);
        }

        // ── Seek bar row ───────────────────────────────────────
        let is_seeking: std::rc::Rc<std::cell::Cell<bool>> =
            std::rc::Rc::new(std::cell::Cell::new(false));
        {
            let seek_row_id = ctx.arena.allocate();
            {
                let Some(el) = ctx.arena.get_mut(seek_row_id) else {
                    return container_id;
                };
                el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
                el.set_gap(8.0);
                el.set_alignment(crate::style::Alignment::Center);
                el.set_flex_grow(1.0);
            }

            let cur_text = Box::new(
                Text::new(current_time_label.read())
                    .bind(current_time_label.clone())
                    .font_size(12.0)
                    .width(36.0),
            );
            let cur_text_id = cur_text.mount_box(&mut ctx.child_with_events(seek_row_id));
            ctx.arena.add_child(seek_row_id, cur_text_id);

            let seek_slider = Box::new(
                Slider::new(position_pct.clone())
                    .range(0.0, 100.0)
                    .step(0.1),
            );
            let seek_id = seek_slider.mount_box(&mut ctx.child_with_events(seek_row_id));
            {
                let Some(el) = ctx.arena.get_mut(seek_id) else {
                    return container_id;
                };
                el.set_flex_grow(1.0);
            }
            ctx.arena.add_child(seek_row_id, seek_id);

            let dur_text = Box::new(
                Text::new(total_time_label.read())
                    .bind(total_time_label.clone())
                    .font_size(12.0)
                    .width(36.0),
            );
            let dur_text_id = dur_text.mount_box(&mut ctx.child_with_events(seek_row_id));
            ctx.arena.add_child(seek_row_id, dur_text_id);

            if let Some(reg) = ctx.event_registry.as_mut() {
                let is_seeking_end = is_seeking.clone();
                let player_seek = player.clone();
                let dur_seek = duration.clone();
                let pp_seek = position_pct.clone();
                reg.register_drag_end(
                    seek_id,
                    Box::new(move |_local: crate::style::Point, _abs| {
                        is_seeking_end.set(false);
                        let pct = pp_seek.read() as f64;
                        let dur = dur_seek.read();
                        if dur > 0.0 {
                            let secs = pct / 100.0 * dur;
                            player_seek.try_seek(Duration::from_secs_f64(secs)).ok();
                        }
                    }),
                );

                let is_seeking_start = is_seeking.clone();
                reg.register_drag_start(
                    seek_id,
                    Box::new(move |_local: crate::style::Point, _abs| {
                        is_seeking_start.set(true);
                    }),
                );
            }

            ctx.arena.add_child(container_id, seek_row_id);
        }

        // ── Controls row ───────────────────────────────────────
        let ctrl_row_id = ctx.arena.allocate();
        {
            let Some(el) = ctx.arena.get_mut(ctrl_row_id) else {
                return container_id;
            };
            el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
            el.set_gap(4.0);
            el.set_alignment(crate::style::Alignment::Center);
        }

        let play_btn = Box::new(Button::new(play_icon.read()).bind(play_icon.clone()));
        let play_btn_id = play_btn.mount_box(&mut ctx.child_with_events(ctrl_row_id));
        ctx.arena.add_child(ctrl_row_id, play_btn_id);

        if let Some(reg) = ctx.event_registry.as_mut() {
            let player_pp = player.clone();
            let ip_pp = is_playing.clone();
            let dur_pp = duration.clone();
            let pi_pp = play_icon.clone();
            let ttl_pp = total_time_label.clone();
            reg.on_click(play_btn_id, move || {
                let was_playing = ip_pp.read();
                if was_playing {
                    player_pp.pause();
                    ip_pp.set(false);
                    pi_pp.set(String::from(PLAY_ICON));
                } else {
                    player_pp.play();
                    ip_pp.set(true);
                    pi_pp.set(String::from(PAUSE_ICON));
                    if dur_pp.read() == 0.0 {
                        if let Some(meta) = player_pp.current_meta() {
                            if let Some(d) = meta.duration {
                                let secs = d.as_secs_f64();
                                dur_pp.set(secs);
                                ttl_pp.set(format_time(secs));
                            }
                        }
                    }
                }
            });
        }

        let stop_btn = Box::new(Button::new(STOP_ICON));
        let stop_btn_id = stop_btn.mount_box(&mut ctx.child_with_events(ctrl_row_id));
        ctx.arena.add_child(ctrl_row_id, stop_btn_id);

        if let Some(reg) = ctx.event_registry.as_mut() {
            let player_stop = player.clone();
            let ip_stop = is_playing.clone();
            let pi_stop = play_icon.clone();
            let pp_stop = position_pct.clone();
            let ctl_stop = current_time_label.clone();
            reg.on_click(stop_btn_id, move || {
                player_stop.stop();
                ip_stop.set(false);
                pi_stop.set(String::from(PLAY_ICON));
                pp_stop.set(0.0);
                ctl_stop.set(String::from("0:00"));
            });
        }

        if show_loop {
            let loop_btn = Box::new(Button::new(loop_icon.read()).bind(loop_icon.clone()));
            let loop_btn_id = loop_btn.mount_box(&mut ctx.child_with_events(ctrl_row_id));
            ctx.arena.add_child(ctrl_row_id, loop_btn_id);

            if let Some(reg) = ctx.event_registry.as_mut() {
                let player_lp = player.clone();
                let li_lp = loop_icon.clone();
                reg.on_click(loop_btn_id, move || {
                    let next = match player_lp.loop_mode() {
                        LoopMode::None => LoopMode::One,
                        LoopMode::One => LoopMode::None,
                    };
                    player_lp.set_loop_mode(next);
                    li_lp.set(String::from(match next {
                        LoopMode::None => LOOP_NONE_ICON,
                        LoopMode::One => LOOP_ONE_ICON,
                    }));
                });
            }
        }

        if show_volume {
            let mute_btn = Box::new(Button::new(mute_icon.read()).bind(mute_icon.clone()));
            let mute_btn_id = mute_btn.mount_box(&mut ctx.child_with_events(ctrl_row_id));
            ctx.arena.add_child(ctrl_row_id, mute_btn_id);

            if let Some(reg) = ctx.event_registry.as_mut() {
                let player_mute = player.clone();
                let im_mute = is_muted.clone();
                let mi_mute = mute_icon.clone();
                reg.on_click(mute_btn_id, move || {
                    player_mute.toggle_mute();
                    let muted = player_mute.is_muted();
                    im_mute.set(muted);
                    mi_mute.set(String::from(if muted { MUTE_ICON } else { VOLUME_ICON }));
                });
            }

            let vol_slider = Box::new(
                Slider::new(volume.clone())
                    .range(0.0, 1.0)
                    .step(0.05)
                    .width(80.0),
            );
            let vol_id = vol_slider.mount_box(&mut ctx.child_with_events(ctrl_row_id));
            ctx.arena.add_child(ctrl_row_id, vol_id);

            if let Some(reg) = ctx.event_registry.as_mut() {
                let player_vol = player.clone();
                let im_vol = is_muted.clone();
                let mi_vol = mute_icon.clone();
                let v_sig = volume.clone();
                reg.register_drag_end(
                    vol_id,
                    Box::new(move |_local: crate::style::Point, _abs| {
                        let v = v_sig.read();
                        player_vol.set_volume(v);
                        if player_vol.is_muted() && v > 0.0 {
                            im_vol.set(false);
                            mi_vol.set(String::from(VOLUME_ICON));
                        }
                    }),
                );
            }
        }

        ctx.arena.add_child(container_id, ctrl_row_id);

        // ── Keyboard events on container ───────────────────────
        {
            let player_kb = player.clone();
            let ip_kb = is_playing.clone();
            let pi_kb = play_icon.clone();
            let pp_kb = position_pct.clone();
            let dur_kb = duration.clone();
            let vol_kb = volume.clone();
            let im_kb = is_muted.clone();
            let mi_kb = mute_icon.clone();
            let li_kb = loop_icon.clone();
            let ctl_kb = current_time_label.clone();
            let ttl_kb = total_time_label.clone();

            let mut events = EventHandler::new();

            {
                let pk = player_kb.clone();
                let ik = ip_kb.clone();
                let pik = pi_kb.clone();
                let dk = dur_kb.clone();
                let tk = ttl_kb.clone();
                events = events.on_key_down(move |key: Key, _mods| -> bool {
                    match key {
                        Key::Space => {
                            if ik.read() {
                                pk.pause();
                                ik.set(false);
                                pik.set(String::from(PLAY_ICON));
                            } else {
                                pk.play();
                                ik.set(true);
                                pik.set(String::from(PAUSE_ICON));
                                if dk.read() == 0.0 {
                                    if let Some(meta) = pk.current_meta() {
                                        if let Some(d) = meta.duration {
                                            let secs = d.as_secs_f64();
                                            dk.set(secs);
                                            tk.set(format_time(secs));
                                        }
                                    }
                                }
                            }
                            true
                        }
                        Key::ArrowLeft => {
                            let dur = dur_kb.read();
                            if dur > 0.0 {
                                let current = pk.get_pos().as_secs_f64();
                                let new_pos = (current - 5.0).max(0.0);
                                pk.try_seek(Duration::from_secs_f64(new_pos)).ok();
                                pp_kb.set((new_pos / dur * 100.0) as f32);
                                ctl_kb.set(format_time(new_pos));
                            }
                            true
                        }
                        Key::ArrowRight => {
                            let dur = dur_kb.read();
                            if dur > 0.0 {
                                let current = pk.get_pos().as_secs_f64();
                                let new_pos = (current + 5.0).min(dur);
                                pk.try_seek(Duration::from_secs_f64(new_pos)).ok();
                                pp_kb.set((new_pos / dur * 100.0) as f32);
                                ctl_kb.set(format_time(new_pos));
                            }
                            true
                        }
                        Key::Home => {
                            let dur = dur_kb.read();
                            if dur > 0.0 {
                                pk.try_seek(Duration::ZERO).ok();
                                pp_kb.set(0.0);
                                ctl_kb.set(String::from("0:00"));
                            }
                            true
                        }
                        Key::End => {
                            let dur = dur_kb.read();
                            if dur > 0.0 {
                                pk.try_seek(Duration::from_secs_f64(dur)).ok();
                                pp_kb.set(100.0);
                                ctl_kb.set(format_time(dur));
                            }
                            true
                        }
                        Key::ArrowUp => {
                            let v = (vol_kb.read() + 0.05).min(1.0);
                            player_kb.set_volume(v);
                            vol_kb.set(v);
                            if player_kb.is_muted() {
                                im_kb.set(false);
                                mi_kb.set(String::from(VOLUME_ICON));
                            }
                            true
                        }
                        Key::ArrowDown => {
                            let v = (vol_kb.read() - 0.05).max(0.0);
                            player_kb.set_volume(v);
                            vol_kb.set(v);
                            true
                        }
                        Key::Character(ref c) if c == "m" => {
                            player_kb.toggle_mute();
                            let muted = player_kb.is_muted();
                            im_kb.set(muted);
                            mi_kb.set(String::from(if muted { MUTE_ICON } else { VOLUME_ICON }));
                            true
                        }
                        Key::Character(ref c) if c == "l" => {
                            let next = match player_kb.loop_mode() {
                                LoopMode::None => LoopMode::One,
                                LoopMode::One => LoopMode::None,
                            };
                            player_kb.set_loop_mode(next);
                            li_kb.set(String::from(match next {
                                LoopMode::None => LOOP_NONE_ICON,
                                LoopMode::One => LOOP_ONE_ICON,
                            }));
                            true
                        }
                        _ => false,
                    }
                });
            }

            if let Some(reg) = ctx.event_registry.as_mut() {
                events.register_all(reg, container_id);
            }
        }

        // ── Per-frame position update (frame_tick) ─────────────
        {
            let frame_player = player.clone();
            let ft_pp = position_pct.clone();
            let ft_dur = duration.clone();
            let ft_ip = is_playing.clone();
            let ft_pi = play_icon.clone();
            let ft_ctl = current_time_label.clone();
            let ft_ttl = total_time_label.clone();
            let ft_oe = on_ended.clone();

            let sched_key = container_id.to_u64();
            let ft_is_seeking = is_seeking.clone();

            let Some(el) = ctx.arena.get_mut(container_id) else {
                return container_id;
            };
            el.set_frame_tick(Box::new(move || {
                let pos = frame_player.get_pos().as_secs_f64();
                let dur = ft_dur.read();
                let was_playing = ft_ip.read();

                let track_ended = frame_player.is_empty() && was_playing;

                if track_ended {
                    match frame_player.loop_mode() {
                        LoopMode::One => {
                            let _ = frame_player.reload();
                            frame_player.play();
                            ft_pp.set(0.0);
                            ft_ctl.set(String::from("0:00"));
                            return;
                        }
                        LoopMode::None => {
                            ft_ip.set(false);
                            ft_pi.set(String::from(PLAY_ICON));
                            ft_pp.set(0.0);
                            ft_ctl.set(String::from("0:00"));
                            scheduler::cancel(sched_key);
                            if let Some(ref cb) = ft_oe {
                                cb();
                            }
                            return;
                        }
                    }
                }

                if !frame_player.is_empty() {
                    let actively_playing = frame_player.is_playing();

                    if actively_playing {
                        scheduler::schedule_at(clock::now() + Duration::from_millis(16), sched_key);
                    } else {
                        scheduler::cancel(sched_key);
                    }

                    if let Some(d) = frame_player.total_duration() {
                        let secs = d.as_secs_f64();
                        if secs > 0.0 && (secs - dur).abs() > 0.01 {
                            ft_dur.set(secs);
                            ft_ttl.set(format_time(secs));
                        }
                    }

                    ft_ctl.set(format_time(pos));
                    let dur = ft_dur.read();
                    if dur > 0.0 && pos > 0.001 && !ft_is_seeking.get() {
                        let pct = (pos / dur * 100.0).clamp(0.0, 100.0) as f32;
                        ft_pp.set(pct);
                    }

                    if !was_playing && actively_playing {
                        ft_ip.set(true);
                        ft_pi.set(String::from(PAUSE_ICON));
                    }
                }
            }));
        }

        container_id
    }
}

impl std::fmt::Debug for AudioPlayerWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioPlayerWidget").finish_non_exhaustive()
    }
}

fn format_time(seconds: f64) -> String {
    let total_secs = (seconds + 0.5).max(0.0) as u64;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, mins, secs)
    } else {
        format!("{}:{:02}", mins, secs)
    }
}
