use crate::{MessageError, MessageReader, MessageWriter};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UserCommand {
    pub server_time: i32,
    pub angles: [i32; 3],
    pub buttons: u16,
    pub weapon: u8,
    pub force_selection: u8,
    pub inventory_selection: u8,
    pub generic_command: u8,
    pub forward_move: i8,
    pub right_move: i8,
    pub up_move: i8,
}

pub fn write_delta_user_command(
    writer: &mut MessageWriter,
    key: i32,
    previous: &UserCommand,
    command: &UserCommand,
) -> Result<(), MessageError> {
    let time_delta = command.server_time.wrapping_sub(previous.server_time);
    if (0..256).contains(&time_delta) {
        writer.write_bits(1, 1)?;
        writer.write_bits(time_delta as u32, 8)?;
    } else {
        writer.write_bits(0, 1)?;
        writer.write_i32(command.server_time)?;
    }

    if controls_equal(previous, command) {
        writer.write_bits(0, 1)?;
        return Ok(());
    }
    writer.write_bits(1, 1)?;
    let key = key ^ command.server_time;
    for index in 0..3 {
        write_delta_key(
            writer,
            key,
            previous.angles[index],
            command.angles[index],
            16,
        )?;
    }
    write_delta_key(
        writer,
        key,
        i32::from(previous.forward_move),
        i32::from(command.forward_move),
        8,
    )?;
    write_delta_key(
        writer,
        key,
        i32::from(previous.right_move),
        i32::from(command.right_move),
        8,
    )?;
    write_delta_key(
        writer,
        key,
        i32::from(previous.up_move),
        i32::from(command.up_move),
        8,
    )?;
    write_delta_key(
        writer,
        key,
        i32::from(previous.buttons),
        i32::from(command.buttons),
        16,
    )?;
    write_delta_key(
        writer,
        key,
        i32::from(previous.weapon),
        i32::from(command.weapon),
        8,
    )?;
    write_delta_key(
        writer,
        key,
        i32::from(previous.force_selection),
        i32::from(command.force_selection),
        8,
    )?;
    write_delta_key(
        writer,
        key,
        i32::from(previous.inventory_selection),
        i32::from(command.inventory_selection),
        8,
    )?;
    write_delta_key(
        writer,
        key,
        i32::from(previous.generic_command),
        i32::from(command.generic_command),
        8,
    )?;
    Ok(())
}

/// Decode one usercmd against `previous` (`MSG_ReadDeltaUsercmdKey`,
/// `codemp/qcommon/msg.cpp:764-806`), including the server's `-128 → -127`
/// move clamps. Used to verify outgoing move packets and by tooling that
/// reads them back.
pub fn read_delta_user_command(
    reader: &mut MessageReader<'_>,
    key: i32,
    previous: &UserCommand,
) -> Result<UserCommand, MessageError> {
    let server_time = if reader.read_bits(1)? == 1 {
        previous
            .server_time
            .wrapping_add(reader.read_bits(8)? as i32)
    } else {
        reader.read_i32()?
    };
    if reader.read_bits(1)? == 0 {
        return Ok(UserCommand {
            server_time,
            ..*previous
        });
    }
    let key = key ^ server_time;
    let mut angles = [0; 3];
    for (angle, old) in angles.iter_mut().zip(previous.angles) {
        *angle = read_delta_key(reader, key, old, 16)?;
    }
    let read_move = |reader: &mut MessageReader<'_>, old: i8| -> Result<i8, MessageError> {
        let value = read_delta_key(reader, key, i32::from(old), 8)? as i8;
        Ok(if value == -128 { -127 } else { value })
    };
    let forward_move = read_move(reader, previous.forward_move)?;
    let right_move = read_move(reader, previous.right_move)?;
    let up_move = read_move(reader, previous.up_move)?;
    let buttons = read_delta_key(reader, key, i32::from(previous.buttons), 16)? as u16;
    let weapon = read_delta_key(reader, key, i32::from(previous.weapon), 8)? as u8;
    let force_selection =
        read_delta_key(reader, key, i32::from(previous.force_selection), 8)? as u8;
    let inventory_selection =
        read_delta_key(reader, key, i32::from(previous.inventory_selection), 8)? as u8;
    let generic_command =
        read_delta_key(reader, key, i32::from(previous.generic_command), 8)? as u8;
    Ok(UserCommand {
        server_time,
        angles,
        buttons,
        weapon,
        force_selection,
        inventory_selection,
        generic_command,
        forward_move,
        right_move,
        up_move,
    })
}

fn read_delta_key(
    reader: &mut MessageReader<'_>,
    key: i32,
    previous: i32,
    width: u8,
) -> Result<i32, MessageError> {
    if reader.read_bits(1)? == 0 {
        return Ok(previous);
    }
    let mask = if width == 32 {
        u32::MAX
    } else {
        (1_u32 << width) - 1
    };
    Ok(((reader.read_bits(width)? ^ (key as u32)) & mask) as i32)
}

/// Stock `Com_HashKey` over the first 32 bytes of a reliable command, which keys the
/// usercmd encryption.
///
/// `codemp/qcommon/common.cpp:679-688` promotes a signed `char`, and that build selects
/// signed char explicitly (`CMakeLists.txt:306`); TaystJK matches. Widening the byte as
/// unsigned makes every command containing a byte >= 0x80 produce a different key from the
/// server's, so the server misdecodes the angles and move axes in our usercmds. Player names
/// with high-bit characters are the common source, which is why this bites on populated
/// servers and not on a quiet one. Keep this independent of the host platform's char
/// signedness.
pub fn legacy_command_hash(command: &[u8]) -> i32 {
    let mut hash = 0_i32;
    for (index, byte) in command.iter().copied().take(32).enumerate() {
        if byte == 0 {
            break;
        }
        hash = hash.wrapping_add(i32::from(byte as i8) * (119 + index as i32));
    }
    hash ^ (hash >> 10) ^ (hash >> 20)
}

fn controls_equal(left: &UserCommand, right: &UserCommand) -> bool {
    left.angles == right.angles
        && left.buttons == right.buttons
        && left.weapon == right.weapon
        && left.force_selection == right.force_selection
        && left.inventory_selection == right.inventory_selection
        && left.generic_command == right.generic_command
        && left.forward_move == right.forward_move
        && left.right_move == right.right_move
        && left.up_move == right.up_move
}

fn write_delta_key(
    writer: &mut MessageWriter,
    key: i32,
    previous: i32,
    value: i32,
    width: u8,
) -> Result<(), MessageError> {
    if previous == value {
        writer.write_bits(0, 1)
    } else {
        writer.write_bits(1, 1)?;
        let mask = if width == 32 {
            u32::MAX
        } else {
            (1_u32 << width) - 1
        };
        writer.write_bits(((value ^ key) as u32) & mask, width)
    }
}
