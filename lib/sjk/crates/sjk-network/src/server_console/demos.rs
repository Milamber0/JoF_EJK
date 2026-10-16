//! `svrecord` and `svstoprecord` (`SV_Record_f`, `SV_StopRecord_f`, `SV_RecordDemo`,
//! `SV_StopRecordDemo`).
use super::{Arguments, LegacyConsoleHost};
use crate::LegacyClientPhase;

/// `PROTOCOL_VERSION`, the demo file's extension.
const PROTOCOL: i32 = 26;
/// `MAX_OSPATH`: a demo's name is cut to this, less one.
const OSPATH_BYTES: usize = 256;

fn atoi(text: &[u8]) -> i32 {
    crate::server_bans::atoi_text(text)
}

/// `SV_Record_f`: a demo of the client named by number, or of the first in the game not
/// recorded yet, under the name given or `demo<date and time>`.
pub(super) fn record(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    let count = arguments.count();
    if count > 3 {
        print(b"record <demoname> <clientnum>\n");
        return;
    }
    let client = if count == 3 {
        let index = atoi(arguments.get(2));
        match usize::try_from(index)
            .ok()
            .filter(|&client| client < host.client_count())
        {
            Some(client) => client,
            None => {
                print(format!("Unknown client number {index}.\n").as_bytes());
                return;
            }
        }
    } else {
        let found = (0..host.client_count()).find(|&client| {
            host.phase(client) != LegacyClientPhase::Free
                && !host.demo_recording(client)
                && host.phase(client) == LegacyClientPhase::Active
        });
        let Some(client) = found else {
            print(b"No active client could be found.\n");
            return;
        };
        client
    };
    if host.demo_recording(client) {
        print(b"Already recording.\n");
        return;
    }
    if host.phase(client) != LegacyClientPhase::Active {
        print(b"Client is not active.\n");
        return;
    }
    let name = if count >= 2 {
        let name = arguments.get(1);
        name[..name.len().min(OSPATH_BYTES - 1)].to_vec()
    } else {
        let name = format!("demo{}", host.timestamp()).into_bytes();
        if host.file_exists(&path(&name)) {
            print(b"Record: Couldn't create a file\n");
            return;
        }
        name
    };
    // `SV_RecordDemo`.
    let path = path(&name);
    print(format!("recording to {path}.\n").as_bytes());
    if !host.start_demo(client, &name, &path) {
        print(b"ERROR: couldn't open.\n");
    }
}

/// `demos/<name>.dm_26`.
fn path(name: &[u8]) -> String {
    format!("demos/{}.dm_{PROTOCOL}", String::from_utf8_lossy(name))
}

/// `SV_StopRecord_f`: the demo of the client named, or of the first recorded.
pub(super) fn stop(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    let client = if arguments.count() == 2 {
        let index = atoi(arguments.get(1));
        match usize::try_from(index)
            .ok()
            .filter(|&client| client < host.client_count())
        {
            Some(client) => client,
            None => {
                print(format!("Unknown client number {index}.\n").as_bytes());
                return;
            }
        }
    } else {
        let Some(client) = (0..host.client_count()).find(|&client| host.demo_recording(client))
        else {
            print(b"No demo being recorded.\n");
            return;
        };
        client
    };
    // `SV_StopRecordDemo`.
    if !host.demo_recording(client) {
        print(format!("Client {client} is not recording a demo.\n").as_bytes());
        return;
    }
    host.stop_demo(client);
    print(format!("Stopped demo for client {client}.\n").as_bytes());
}
