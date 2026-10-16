//! Actual render timing boundary and post-present transition reporting.
use crate::*;

impl GpuState {
    /// Unchanged instance packing, extracted solely to keep frame orchestration bounded.
    pub(crate) fn pack_frame_instances(
        instances: &mut Vec<ActorInstance>,
        actor_ranges: &mut Vec<Range<u32>>,
        object_ranges: &mut Vec<Range<u32>>,
        mover_ranges: &mut Vec<Range<u32>>,
        actors: &[Vec<ActorInstance>],
        objects: &[Vec<ActorInstance>],
        movers: &[Vec<ActorInstance>],
    ) {
        instances.clear();
        actor_ranges.clear();
        object_ranges.clear();
        mover_ranges.clear();
        for group in actors.iter() {
            actor_ranges.push(crate::actor_instance::append_group(instances, group));
        }
        for group in objects.iter() {
            object_ranges.push(crate::actor_instance::append_group(instances, group));
        }
        movers::append_groups(instances, movers, mover_ranges);
    }

    /// Unchanged frame-entry operations, extracted only to keep the render module bounded.
    pub(crate) fn prepare_timed_frame(&mut self, game_audio: &mut Option<GameAudio>) {
        self.load_event_gap.observe(Instant::now());
        self.poll_console_screenshot();
        self.poll_bug_report();
        self.poll_player_report();
        self.poll_world_note();
        self.run_console_command_buffer(game_audio);
        self.sync_runtime_cvars();
        self.update_input_motion();
        self.poll_client_shell();
        self.finish_resident_attach(game_audio);
        self.drive_portal();
        if let Err(error) = self.update_player_animation() {
            eprintln!("player preview animation stopped: {error}");
            self.player_animation = None;
        }
    }

    /// Measure all production render attempts; skipped attempts are not completed samples.
    pub(crate) fn render(&mut self, audio: &mut Option<GameAudio>) -> FrameStatus {
        if std::mem::take(&mut self.frame_pacer.report_context) {
            eprintln!(
                "frame-budget context adapter={} backend={} viewport={}x{} \
                headless={} budget=3.00ms population=2048",
                self.adapter_name,
                self.adapter_backend,
                self.configuration.width,
                self.configuration.height,
                self.window.is_none()
            );
        }
        let now = Instant::now();
        self.frame_pacer.begin_render(now);
        let mut timer = super::budget::Timer::new(now);
        let status = self.render_inner(audio, &mut timer);
        if status == FrameStatus::Rendered {
            self.frame_pacer.record(timer.finish());
            self.observe_hitch(now);
            self.finish_frame();
        } else {
            self.frame_pacer.schedule(self.maximum_fps());
        }
        status
    }

    /// Preserve the existing transition/quit reporting outside the large render method.
    pub(crate) fn complete_render_transition(&mut self, game_audio: &mut Option<GameAudio>) {
        if self.live_map_installed
            && let Some(mut timeline) = self.connect_timeline.take()
        {
            timeline.mark(log::TimelinePhase::FirstFrame);
            timeline.set_event_gap(self.load_event_gap.stop());
            log::progress(format_args!("{}", timeline.line()));
            log::progress(format_args!(
                "audio underruns after first frame: {}",
                game_audio.as_ref().map_or(0, GameAudio::underrun_count)
            ));
            if let Some(audio) = game_audio.as_ref() {
                let (count, maximum_ms) = audio.deferred_start_stats();
                log::progress(format_args!(
                    "deferred audio after first frame: count={count} max={maximum_ms:.1}ms"
                ));
            }
        } else if self.transition_report_pending {
            let gap = self.load_event_gap.stop();
            let elapsed = self
                .world_load_started
                .take()
                .map_or(0.0, |started| started.elapsed().as_secs_f64() * 1_000.0);
            log::progress(format_args!(
                "map transition first frame: map={} total={elapsed:.1}ms \
                 max-event-gap={gap}",
                self.world_load_map
            ));
            self.transition_report_pending = false;
        }
    }
}
