//! Streaming protocol-26 demo recording.

use sjk_protocol::{
    GameState, GameStateWriteError, MAX_LEGACY_MESSAGE_BYTES, write_initial_gamestate,
};
use std::error::Error;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Classification of a received server message while waiting to record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DemoMessageKind {
    /// The message contains no snapshot command.
    Other,
    /// The message contains a snapshot encoded against an older snapshot.
    DeltaSnapshot,
    /// The message contains a snapshot with `deltaNum == 0`.
    FullSnapshot,
}

/// Counts from the current or most recently stopped recording.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DemoRecordingStats {
    /// Demo records written, including the synthesized gamestate.
    pub records_written: u64,
    /// Delta snapshot messages skipped before the first full snapshot.
    pub skipped_delta_snapshots: u64,
    /// Payload bytes written, excluding framing and the terminator.
    pub payload_bytes: u64,
}

struct Recording {
    output: BufWriter<File>,
    frame: Vec<u8>,
    path: PathBuf,
    waiting_for_full_snapshot: bool,
}

/// Stateful `.dm_26` writer owned by one live client session.
///
/// `CL_Record_f` writes a synthetic gamestate before setting `demowaiting`
/// (`codemp/client/cl_main.cpp:338-394`). `CL_ParseSnapshot` clears that wait
/// only for `deltaNum == 0` (`codemp/client/cl_parse.cpp:220-260`), after which
/// `CL_WriteDemoMessage` stores the decoded message payload with its server
/// sequence (`cl_main.cpp:201-219`).
#[derive(Default)]
pub struct DemoRecorder {
    recording: Option<Recording>,
    stats: DemoRecordingStats,
}

impl DemoRecorder {
    /// Begin recording in `<config>/demos`, refusing to overwrite a file.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &mut self,
        active: bool,
        config_directory: &Path,
        requested_name: Option<&str>,
        now: SystemTime,
        first_sequence: i32,
        reliable_sequence: i32,
        game_state: &GameState,
    ) -> Result<&Path, DemoRecorderError> {
        if !active {
            return Err(DemoRecorderError::NotActive);
        }
        if self.recording.is_some() {
            return Err(DemoRecorderError::AlreadyRecording);
        }
        let name = normalized_demo_name(requested_name, now)?;
        let directory = config_directory.join("demos");
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(name);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| DemoRecorderError::Create {
                path: path.clone(),
                error,
            })?;
        let payload = write_initial_gamestate(reliable_sequence, game_state)?;
        let mut output = BufWriter::new(file);
        let mut frame = Vec::with_capacity(MAX_LEGACY_MESSAGE_BYTES + 8);
        write_record(&mut output, &mut frame, first_sequence, &payload)?;
        self.stats = DemoRecordingStats {
            records_written: 1,
            skipped_delta_snapshots: 0,
            payload_bytes: payload.len() as u64,
        };
        self.recording = Some(Recording {
            output,
            frame,
            path,
            waiting_for_full_snapshot: true,
        });
        Ok(&self.recording.as_ref().expect("installed recording").path)
    }

    /// Keep move packets requesting a full snapshot until the recording gate opens.
    /// Pending delta packets may arrive between `record` and the next usercmd.
    pub(crate) fn waiting_for_full_snapshot(&self) -> bool {
        self.recording
            .as_ref()
            .is_some_and(|recording| recording.waiting_for_full_snapshot)
    }

    /// Append one decoded netchan payload when the demowaiting gate permits it.
    pub fn observe_message(
        &mut self,
        sequence: i32,
        payload: &[u8],
        kind: DemoMessageKind,
    ) -> Result<(), DemoRecorderError> {
        let Some(recording) = self.recording.as_mut() else {
            return Ok(());
        };
        if recording.waiting_for_full_snapshot {
            match kind {
                DemoMessageKind::FullSnapshot => {
                    recording.waiting_for_full_snapshot = false;
                }
                DemoMessageKind::DeltaSnapshot => {
                    self.stats.skipped_delta_snapshots += 1;
                    return Ok(());
                }
                DemoMessageKind::Other => return Ok(()),
            }
        }
        write_record(
            &mut recording.output,
            &mut recording.frame,
            sequence,
            payload,
        )?;
        self.stats.records_written += 1;
        self.stats.payload_bytes += payload.len() as u64;
        Ok(())
    }

    /// Finish the stream with OpenJK's `-1, -1` terminator and close it.
    ///
    /// This matches `CL_StopRecord_f` at `codemp/client/cl_main.cpp:229-245`.
    pub fn stop(&mut self) -> Result<Option<PathBuf>, DemoRecorderError> {
        let Some(mut recording) = self.recording.take() else {
            return Ok(None);
        };
        let mut terminator = [0_u8; 8];
        terminator[..4].copy_from_slice(&(-1_i32).to_le_bytes());
        terminator[4..].copy_from_slice(&(-1_i32).to_le_bytes());
        recording.output.write_all(&terminator)?;
        recording.output.flush()?;
        Ok(Some(recording.path))
    }

    /// Whether a demo file is currently open.
    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Path of the current recording.
    pub fn path(&self) -> Option<&Path> {
        self.recording
            .as_ref()
            .map(|recording| recording.path.as_path())
    }

    /// Current counters without allocating.
    pub fn stats(&self) -> &DemoRecordingStats {
        &self.stats
    }
}

impl Drop for DemoRecorder {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn write_record(
    output: &mut impl Write,
    frame: &mut Vec<u8>,
    sequence: i32,
    payload: &[u8],
) -> io::Result<()> {
    if payload.len() > MAX_LEGACY_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "demo payload exceeds legacy message limit",
        ));
    }
    let length = i32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "demo payload exceeds i32"))?;
    frame.clear();
    frame.extend_from_slice(&sequence.to_le_bytes());
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(payload);
    output.write_all(frame)
}

fn normalized_demo_name(
    requested: Option<&str>,
    now: SystemTime,
) -> Result<String, DemoRecorderError> {
    let name = requested
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| legacy_demo_filename(now));
    if name.contains(['/', '\\']) || name == "." || name == ".." {
        return Err(DemoRecorderError::InvalidName);
    }
    Ok(if name.to_ascii_lowercase().ends_with(".dm_26") {
        name
    } else {
        format!("{name}.dm_26")
    })
}

/// Format the stock automatic `demoYYYY-MM-DD_HH-MM-SS.dm_26` name.
///
/// OpenJK uses `Com_RealTime` at `codemp/client/cl_main.cpp:253-261`. The
/// portable implementation uses UTC because `std` has no local-time API.
pub fn legacy_demo_filename(now: SystemTime) -> String {
    let seconds = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = (seconds / 86_400) as i64;
    let day_seconds = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = day_seconds / 3_600;
    let minute = day_seconds % 3_600 / 60;
    let second = day_seconds % 60;
    format!("demo{year:04}-{month:02}-{day:02}_{hour:02}-{minute:02}-{second:02}.dm_26")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u64, u64) {
    let shifted = days_since_epoch + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month as u64, day as u64)
}

/// Failure while starting, streaming, or stopping a demo recording.
#[derive(Debug)]
pub enum DemoRecorderError {
    /// A local continuation has no protocol stream to record.
    LocalContinuation,
    /// No active level is available to synthesize a gamestate.
    NotActive,
    /// A recording is already open.
    AlreadyRecording,
    /// The requested name would escape the demos directory.
    InvalidName,
    /// The target file could not be created without overwriting it.
    Create { path: PathBuf, error: io::Error },
    /// Filesystem write failed.
    Io(io::Error),
    /// Synthesizing the initial protocol message failed.
    GameState(GameStateWriteError),
}

impl fmt::Display for DemoRecorderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalContinuation => formatter.write_str("Local continuation has no demo stream; start recording after rejoining the server."),
            Self::NotActive => formatter.write_str("You must be in a level to record."),
            Self::AlreadyRecording => formatter.write_str("Already recording."),
            Self::InvalidName => formatter.write_str("Record: invalid demo name"),
            Self::Create { path, error } => {
                write!(
                    formatter,
                    "Record: Couldn't create {}: {error}",
                    path.display()
                )
            }
            Self::Io(error) => write!(formatter, "demo recording failed: {error}"),
            Self::GameState(error) => write!(formatter, "demo gamestate failed: {error}"),
        }
    }
}

impl Error for DemoRecorderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Create { error, .. } | Self::Io(error) => Some(error),
            Self::GameState(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for DemoRecorderError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<GameStateWriteError> for DemoRecorderError {
    fn from(value: GameStateWriteError) -> Self {
        Self::GameState(value)
    }
}
