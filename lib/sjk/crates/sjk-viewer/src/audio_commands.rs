//! Sound console services share the map music sequencer and asset registration.
use super::*;

impl GameAudio {
    /// Execute one explicit sound command; no separate music player is created.
    pub(crate) fn sound_command(
        &mut self,
        name: &str,
        args: &[String],
        vfs: Option<&VirtualFileSystem>,
    ) -> Result<Vec<String>, String> {
        match name {
            "play" => {
                if args.is_empty() {
                    return Err("usage: play <file...>".into());
                }
                let vfs = vfs.ok_or("No asset search path attached")?;
                let mut lines = Vec::new();
                for path in args {
                    if let Some(handle) = self.register_vfs_async(vfs, path) {
                        self.output.send(AudioCommand::Play(
                            handle,
                            PlayRequest {
                                origin: None,
                                source: SourceId(u32::MAX - 1),
                                // codemp/qcommon/q_shared.h:863, CHAN_LOCAL_SOUND.
                                channel: ChannelId(8),
                                volume: 1.0,
                                attenuation: sjk_audio::Attenuation::None,
                            },
                        ));
                    } else {
                        lines.push(format!("WARNING: could not find {path}"));
                    }
                }
                Ok(lines)
            }
            "music" => {
                let music = match args {
                    [intro] => assets::MusicSpec {
                        intro: intro.clone(),
                        repeating: intro.clone(),
                    },
                    [intro, repeating] => assets::MusicSpec {
                        intro: intro.clone(),
                        repeating: repeating.clone(),
                    },
                    _ => return Err("music <musicfile> [loopfile]".into()),
                };
                self.output.stop_music();
                self.map_music = None;
                self.start_music(vfs.ok_or("No asset search path attached")?, &music);
                if self.map_music.is_none() {
                    return Err("Music file/level could not be loaded".into());
                }
                Ok(vec![])
            }
            "stopmusic" | "soundstop" => {
                self.output.stop_music();
                self.map_music = None;
                self.dynamic = None;
                if name == "soundstop" {
                    self.output.send(AudioCommand::StopAll);
                    self.deferred_snapshots.clear();
                }
                Ok(vec![])
            }
            "s_dynamic" => {
                let [state] = args else {
                    return Err("s_dynamic <explore|action|silence|boss|death>".into());
                };
                if state.eq_ignore_ascii_case("boss") || state.eq_ignore_ascii_case("death") {
                    return Err("Boss/death terminal-state timing is not implemented".into());
                }
                if !self.allow_dynamic {
                    return Err("Dynamic music inhibited by s_allowDynamicMusic".into());
                }
                let level = self.dynamic.as_mut().ok_or("No active dynamic music set")?;
                if ["explore", "action", "silence", "boss", "death"][level.state]
                    .eq_ignore_ascii_case(state)
                {
                    return Ok(vec![]);
                }
                let path = level.request(state)?.map(str::to_owned);
                if let Some(path) = path {
                    self.start_file_music(
                        vfs.ok_or("No asset search path attached")?,
                        &assets::MusicSpec {
                            intro: path.clone(),
                            repeating: path,
                        },
                    );
                } else {
                    self.output.stop_music();
                }
                Ok(vec![
                    "Dynamic selection applied; marker timing/crossfades are not implemented"
                        .into(),
                ])
            }
            "soundlist" => Ok(self
                .output
                .cache
                .borrow()
                .iter()
                .map(|(handle, size, format)| {
                    let path = self
                        .handles
                        .iter()
                        .find(|(_, h)| *h == handle)
                        .map_or("<loading>", |(path, _)| path.as_str());
                    format!("{size:8} encoded bytes {format:4} -> 44100 Hz mono PCM: {path}")
                })
                .collect()),
            "soundinfo" => Ok(vec![format!(
                "rodio/cpal {}; mixer: 44100 Hz, 2 channels",
                self.output.description
            )]),
            _ => Err("Unsupported sound command".into()),
        }
    }

    /// Drain the bounded diagnostic handles; disabled s_show produces no strings.
    pub(crate) fn print_sound_starts(&mut self, console: &mut ViewerConsole) {
        while let Some(handle) = self.output.started.pop_front() {
            let path = self
                .handles
                .iter()
                .find(|(_, h)| **h == handle)
                .map_or("<loading>", |(path, _)| path.as_str());
            console.push_log(format!("sound: {path}"));
        }
    }
}
