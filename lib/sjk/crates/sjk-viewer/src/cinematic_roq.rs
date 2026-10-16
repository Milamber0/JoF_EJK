//! A decoder for id Software's RoQ video, the format of Jedi Academy's cinematics and of
//! the `videoMap` shader stages that play one on a surface (`world_videos.rs`).
//!
//! The file is a run of chunks, each an 8-byte header (`u16` id, `u32` size, `u16`
//! argument, little-endian) and its payload; the first header (`0x1084`) carries the
//! frame rate in its argument and no payload. `0x1001` gives the size, `0x1002` replaces
//! the codebooks (2x2 cells of four luma and one chroma pair, and 4x4 cells of four 2x2
//! indices) and `0x1011` codes one frame against the previous one, per 16x16 macroblock
//! split into four 8x8 blocks and, where needed, four 4x4 blocks: unchanged, moved from
//! the previous frame, one codebook cell scaled up, or split further. Audio and the rare
//! JPEG intra frames are skipped (a JPEG frame keeps the picture). The colour conversion
//! is Quake III's (`cl_cin.c`), full-range BT.601.
//!
//! References: Dr. Tim Ferguson's RoQ notes and FFmpeg's `roqvideodec.c`; no code taken.

use std::sync::Arc;

const SIGNATURE: u16 = 0x1084;
const INFO: u16 = 0x1001;
const CODEBOOK: u16 = 0x1002;
const QUAD_VQ: u16 = 0x1011;
const QUAD_JPEG: u16 = 0x1012;
const HEADER_BYTES: usize = 8;
/// Larger pictures are not videos a shader plays (retail's largest is 640x480).
const MAX_SIDE: u16 = 2048;

/// Why a RoQ file cannot be played.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RoqError {
    /// Not a RoQ file: no `0x1084` signature chunk.
    Signature,
    /// No size chunk before the first frame, or a size of zero or above [`MAX_SIDE`].
    Size,
}

impl std::fmt::Display for RoqError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Signature => f.write_str("not a RoQ video"),
            Self::Size => f.write_str("RoQ video without a usable picture size"),
        }
    }
}

impl std::error::Error for RoqError {}

/// One chunk's header.
#[derive(Clone, Copy)]
struct Chunk {
    id: u16,
    size: usize,
    argument: u16,
}

/// A playing RoQ video: the file, the read position and the decoder's state.
pub(crate) struct Roq {
    data: Arc<[u8]>,
    /// Offset of the first chunk after the signature, where a loop starts again.
    start: usize,
    cursor: usize,
    width: usize,
    height: usize,
    /// The decoding planes' size: the picture rounded up to whole macroblocks.
    stride: usize,
    rows: usize,
    fps: f32,
    cells: [[u8; 6]; 256],
    quads: [[u8; 4]; 256],
    /// Luma, then the two chroma planes at full resolution, of the frame being built
    /// and of the one before it.
    current: Vec<u8>,
    previous: Vec<u8>,
    frames: u64,
}

impl Roq {
    /// Open a RoQ file: read its signature and size, ready to decode the first frame.
    pub(crate) fn open(data: Arc<[u8]>) -> Result<Self, RoqError> {
        let signature = chunk_at(&data, 0).ok_or(RoqError::Signature)?;
        if signature.id != SIGNATURE {
            return Err(RoqError::Signature);
        }
        // Quake III: a frame rate of zero means 30.
        let fps = if signature.argument == 0 {
            30.0
        } else {
            f32::from(signature.argument)
        };
        let start = HEADER_BYTES;
        let mut cursor = start;
        let (width, height) = loop {
            let Some(chunk) = chunk_at(&data, cursor) else {
                return Err(RoqError::Size);
            };
            let payload = cursor + HEADER_BYTES;
            if chunk.id == INFO {
                let bytes = data.get(payload..payload + 4).ok_or(RoqError::Size)?;
                let width = u16::from_le_bytes([bytes[0], bytes[1]]);
                let height = u16::from_le_bytes([bytes[2], bytes[3]]);
                if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
                    return Err(RoqError::Size);
                }
                break (usize::from(width), usize::from(height));
            }
            if chunk.id == QUAD_VQ || chunk.id == QUAD_JPEG {
                return Err(RoqError::Size);
            }
            cursor = payload.saturating_add(chunk.size);
        };
        let stride = width.div_ceil(16) * 16;
        let rows = height.div_ceil(16) * 16;
        let mut current = vec![0; stride * rows * 3];
        // Black: luma 0, chroma at its middle.
        current[stride * rows..].fill(128);
        Ok(Self {
            data,
            start,
            cursor: start,
            width,
            height,
            stride,
            rows,
            fps,
            cells: [[0, 0, 0, 0, 128, 128]; 256],
            quads: [[0; 4]; 256],
            previous: current.clone(),
            current,
            frames: 0,
        })
    }

    pub(crate) fn width(&self) -> u32 {
        self.width as u32
    }

    pub(crate) fn height(&self) -> u32 {
        self.height as u32
    }

    /// Frames per second the video is played at.
    pub(crate) fn fps(&self) -> f32 {
        self.fps
    }

    /// Frames decoded so far, across loops.
    pub(crate) fn frames(&self) -> u64 {
        self.frames
    }

    /// Decode the next frame, starting over at the end of the file (`videoMap` loops).
    /// False when the file holds no frame at all.
    pub(crate) fn advance(&mut self) -> bool {
        let mut wrapped = false;
        loop {
            let Some(chunk) = chunk_at(&self.data, self.cursor) else {
                if wrapped {
                    return false;
                }
                wrapped = true;
                self.cursor = self.start;
                continue;
            };
            let payload = self.cursor + HEADER_BYTES;
            let end = payload.saturating_add(chunk.size).min(self.data.len());
            self.cursor = payload.saturating_add(chunk.size);
            match chunk.id {
                CODEBOOK => self.read_codebook(chunk, payload, end),
                QUAD_VQ => {
                    self.decode(chunk.argument, payload, end);
                    self.frames += 1;
                    return true;
                }
                QUAD_JPEG => {
                    self.frames += 1;
                    return true;
                }
                _ => {}
            }
        }
    }

    /// The current picture as RGBA, `width * height * 4` bytes.
    pub(crate) fn write_rgba(&self, out: &mut [u8]) {
        let plane = self.stride * self.rows;
        for y in 0..self.height {
            for x in 0..self.width {
                let at = y * self.stride + x;
                let rgb = yuv_to_rgb(
                    self.previous[at],
                    self.previous[plane + at],
                    self.previous[2 * plane + at],
                );
                let to = (y * self.width + x) * 4;
                if let Some(pixel) = out.get_mut(to..to + 4) {
                    pixel.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
                }
            }
        }
    }

    fn read_codebook(&mut self, chunk: Chunk, payload: usize, end: usize) {
        let data = &self.data[payload.min(end)..end];
        let mut pairs = usize::from(chunk.argument >> 8);
        let mut quads = usize::from(chunk.argument & 0xff);
        if pairs == 0 {
            pairs = 256;
        }
        if quads == 0 && pairs * 6 < chunk.size {
            quads = 256;
        }
        let mut read = 0;
        for cell in self.cells.iter_mut().take(pairs) {
            let Some(bytes) = data.get(read..read + 6) else {
                return;
            };
            cell.copy_from_slice(bytes);
            read += 6;
        }
        for quad in self.quads.iter_mut().take(quads) {
            let Some(bytes) = data.get(read..read + 4) else {
                return;
            };
            quad.copy_from_slice(bytes);
            read += 4;
        }
    }

    /// One `0x1011` frame: the previous picture changed where the codes say.
    fn decode(&mut self, argument: u16, payload: usize, end: usize) {
        self.current.copy_from_slice(&self.previous);
        let mean = [
            i32::from((argument >> 8) as u8 as i8),
            i32::from(argument as u8 as i8),
        ];
        let data = Arc::clone(&self.data);
        let mut reader = Reader {
            data: &data[payload.min(end)..end],
            at: 0,
            flags: 0,
            left: 0,
        };
        'frame: for top in (0..self.rows).step_by(16) {
            for left in (0..self.stride).step_by(16) {
                for block in 0..4 {
                    let x = left + 8 * (block & 1);
                    let y = top + 8 * (block >> 1);
                    let Some(code) = reader.code() else {
                        break 'frame;
                    };
                    match code {
                        0 => {}
                        1 => {
                            let Some(byte) = reader.byte() else {
                                break 'frame;
                            };
                            let (dx, dy) = motion(byte, mean);
                            self.moved(x, y, 8, dx, dy);
                        }
                        2 => {
                            let Some(index) = reader.byte() else {
                                break 'frame;
                            };
                            let quad = self.quads[usize::from(index)];
                            for (part, cell) in quad.iter().enumerate() {
                                let (cx, cy) = (x + 4 * (part & 1), y + 4 * (part >> 1));
                                self.cell(cx, cy, usize::from(*cell), 2);
                            }
                        }
                        _ => {
                            for part in 0..4 {
                                let (sx, sy) = (x + 4 * (part & 1), y + 4 * (part >> 1));
                                let Some(code) = reader.code() else {
                                    break 'frame;
                                };
                                match code {
                                    0 => {}
                                    1 => {
                                        let Some(byte) = reader.byte() else {
                                            break 'frame;
                                        };
                                        let (dx, dy) = motion(byte, mean);
                                        self.moved(sx, sy, 4, dx, dy);
                                    }
                                    2 => {
                                        let Some(index) = reader.byte() else {
                                            break 'frame;
                                        };
                                        let quad = self.quads[usize::from(index)];
                                        for (piece, cell) in quad.iter().enumerate() {
                                            let (cx, cy) =
                                                (sx + 2 * (piece & 1), sy + 2 * (piece >> 1));
                                            self.cell(cx, cy, usize::from(*cell), 1);
                                        }
                                    }
                                    _ => {
                                        for piece in 0..4 {
                                            let Some(index) = reader.byte() else {
                                                break 'frame;
                                            };
                                            let (cx, cy) =
                                                (sx + 2 * (piece & 1), sy + 2 * (piece >> 1));
                                            self.cell(cx, cy, usize::from(index), 1);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        std::mem::swap(&mut self.current, &mut self.previous);
    }

    /// A 2x2 codebook cell at (`x`, `y`), each texel drawn `scale` times wide and high.
    fn cell(&mut self, x: usize, y: usize, index: usize, scale: usize) {
        let cell = self.cells[index];
        let plane = self.stride * self.rows;
        for (corner, luma) in cell[..4].iter().enumerate() {
            let (ox, oy) = (x + scale * (corner & 1), y + scale * (corner >> 1));
            for dy in 0..scale {
                for dx in 0..scale {
                    let (px, py) = (ox + dx, oy + dy);
                    if px >= self.stride || py >= self.rows {
                        continue;
                    }
                    let at = py * self.stride + px;
                    self.current[at] = *luma;
                    self.current[plane + at] = cell[4];
                    self.current[2 * plane + at] = cell[5];
                }
            }
        }
    }

    /// A `size` block at (`x`, `y`) copied from the previous picture `dx`, `dy` away; a
    /// vector pointing outside the picture leaves the block as it was.
    fn moved(&mut self, x: usize, y: usize, size: usize, dx: i32, dy: i32) {
        let (sx, sy) = (x as i64 + i64::from(dx), y as i64 + i64::from(dy));
        if sx < 0 || sy < 0 || sx as usize + size > self.stride || sy as usize + size > self.rows {
            return;
        }
        let (sx, sy) = (sx as usize, sy as usize);
        let plane = self.stride * self.rows;
        for channel in 0..3 {
            let base = channel * plane;
            for row in 0..size {
                let from = base + (sy + row) * self.stride + sx;
                let to = base + (y + row) * self.stride + x;
                self.current[to..to + size].copy_from_slice(&self.previous[from..from + size]);
            }
        }
    }
}

/// The motion of an `FCC` code: its byte's two nibbles against the frame's mean.
fn motion(byte: u8, mean: [i32; 2]) -> (i32, i32) {
    (
        8 - i32::from(byte >> 4) - mean[0],
        8 - i32::from(byte & 0xf) - mean[1],
    )
}

/// The frame's bytes with the 2-bit codes, read sixteen bits (eight codes) at a time
/// from the top.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
    flags: u16,
    left: u8,
}

impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.at)?;
        self.at += 1;
        Some(byte)
    }

    fn code(&mut self) -> Option<u8> {
        if self.left == 0 {
            let low = self.byte()?;
            let high = self.byte()?;
            self.flags = u16::from_le_bytes([low, high]);
            self.left = 8;
        }
        self.left -= 1;
        Some(((self.flags >> (u32::from(self.left) * 2)) & 3) as u8)
    }
}

fn chunk_at(data: &[u8], at: usize) -> Option<Chunk> {
    let bytes = data.get(at..at.checked_add(HEADER_BYTES)?)?;
    let size = u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
    Some(Chunk {
        id: u16::from_le_bytes([bytes[0], bytes[1]]),
        // The signature's size is 0xffffffff: it has no payload.
        size: if size == u32::MAX { 0 } else { size as usize },
        argument: u16::from_le_bytes([bytes[6], bytes[7]]),
    })
}

/// Quake III's conversion (`cl_cin.c`, `ROQ_*_tab`): full-range BT.601.
fn yuv_to_rgb(y: u8, u: u8, v: u8) -> [u8; 3] {
    let (y, u, v) = (f32::from(y), f32::from(u) - 128.0, f32::from(v) - 128.0);
    let clamp = |value: f32| value.round().clamp(0.0, 255.0) as u8;
    [
        clamp(y + 1.402 * v),
        clamp(y - 0.344_14 * u - 0.714_14 * v),
        clamp(y + 1.772 * u),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: u16, argument: u16, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&id.to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&argument.to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn header(fps: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SIGNATURE.to_le_bytes());
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        bytes.extend_from_slice(&fps.to_le_bytes());
        bytes
    }

    fn info(width: u16, height: u16) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&width.to_le_bytes());
        payload.extend_from_slice(&height.to_le_bytes());
        payload.extend_from_slice(&[8, 0, 4, 0]);
        chunk(INFO, 0, &payload)
    }

    /// Codes packed eight to a little-endian word, first code in the top bits.
    fn flags(codes: &[u8]) -> [u8; 2] {
        let mut word = 0_u16;
        for (index, code) in codes.iter().enumerate() {
            word |= u16::from(*code) << (14 - 2 * index);
        }
        word.to_le_bytes()
    }

    fn rgba(video: &Roq) -> Vec<u8> {
        let mut out = vec![0; (video.width() * video.height() * 4) as usize];
        video.write_rgba(&mut out);
        out
    }

    fn pixel(image: &[u8], width: usize, x: usize, y: usize) -> [u8; 3] {
        let at = (y * width + x) * 4;
        [image[at], image[at + 1], image[at + 2]]
    }

    /// A 16x16 video: a codebook of grey cells, one frame painting the four 8x8 blocks
    /// with four cells scaled up, then a frame moving the top left block right.
    fn two_frames() -> Vec<u8> {
        let mut file = header(15);
        file.extend(info(16, 16));
        // Cells: 0 black, 1 white, 2 mid grey with red chroma, 3 dark.
        let mut book = Vec::new();
        for cell in [
            [0, 0, 0, 0, 128, 128],
            [255, 255, 255, 255, 128, 128],
            [128, 128, 128, 128, 128, 255],
            [40, 40, 40, 40, 128, 128],
        ] {
            book.extend_from_slice(&cell);
        }
        // Quads (4x4 from four cells): quad i is all cell i.
        for quad in 0..4_u8 {
            book.extend_from_slice(&[quad; 4]);
        }
        file.extend(chunk(CODEBOOK, (4 << 8) | 4, &book));
        let mut frame = Vec::new();
        frame.extend_from_slice(&flags(&[2, 2, 2, 2]));
        frame.extend_from_slice(&[0, 1, 2, 3]);
        file.extend(chunk(QUAD_VQ, 0, &frame));
        // Second frame: the top right block takes the top left one's picture (8 left),
        // the rest is unchanged.
        let mut second = Vec::new();
        second.extend_from_slice(&flags(&[0, 1, 0, 0]));
        // The source lies 8 to the left: dx = 8 - nibble - mean = 8 - 8 - 8 with a mean
        // of 8 in the argument's high byte, dy = 8 - 8 - 0.
        second.push(0x88);
        file.extend(chunk(QUAD_VQ, 8 << 8, &second));
        file
    }

    #[test]
    fn decodes_codebook_cells_and_motion() {
        let mut video = Roq::open(two_frames().into()).expect("a RoQ file");
        assert_eq!((video.width(), video.height(), video.fps()), (16, 16, 15.0));
        assert!(video.advance());
        let first = rgba(&video);
        assert_eq!(pixel(&first, 16, 0, 0), [0, 0, 0]);
        assert_eq!(pixel(&first, 16, 15, 0), [255, 255, 255]);
        // Mid grey with V raised: red up, green down, blue unchanged.
        let red = pixel(&first, 16, 3, 12);
        assert!(red[0] > 200 && red[1] < 128 && red[2] == 128, "{red:?}");
        assert_eq!(pixel(&first, 16, 12, 12), [40, 40, 40]);
        assert!(video.advance());
        let second = rgba(&video);
        // The top right block now shows the top left one; the rest stayed.
        assert_eq!(pixel(&second, 16, 12, 3), [0, 0, 0]);
        assert_eq!(pixel(&second, 16, 0, 0), [0, 0, 0]);
        assert_eq!(pixel(&second, 16, 12, 12), [40, 40, 40]);
        assert_eq!(video.frames(), 2);
    }

    #[test]
    fn loops_back_to_the_first_frame() {
        let mut video = Roq::open(two_frames().into()).expect("a RoQ file");
        assert!(video.advance());
        assert!(video.advance());
        // The third frame is the first again.
        assert!(video.advance());
        assert_eq!(pixel(&rgba(&video), 16, 15, 0), [255, 255, 255]);
        assert_eq!(video.frames(), 3);
    }

    #[test]
    fn split_blocks_take_small_cells() {
        let mut file = header(30);
        file.extend(info(16, 16));
        let mut book = Vec::new();
        book.extend_from_slice(&[10, 20, 30, 40, 128, 128]);
        book.extend_from_slice(&[200, 200, 200, 200, 128, 128]);
        file.extend(chunk(CODEBOOK, 2 << 8, &book));
        let mut frame = Vec::new();
        // Block 0 split: its first 4x4 as four direct cells, the rest unchanged.
        frame.extend_from_slice(&flags(&[3, 3, 0, 0, 0, 0, 0, 0]));
        frame.extend_from_slice(&[0, 1, 1, 1]);
        file.extend(chunk(QUAD_VQ, 0, &frame));
        let mut video = Roq::open(file.into()).expect("a RoQ file");
        assert!(video.advance());
        let image = rgba(&video);
        // The first 2x2 cell keeps its four luma values in reading order.
        assert_eq!(pixel(&image, 16, 0, 0), [10, 10, 10]);
        assert_eq!(pixel(&image, 16, 1, 0), [20, 20, 20]);
        assert_eq!(pixel(&image, 16, 0, 1), [30, 30, 30]);
        assert_eq!(pixel(&image, 16, 1, 1), [40, 40, 40]);
        assert_eq!(pixel(&image, 16, 2, 0), [200, 200, 200]);
        // Outside the split 4x4 the picture is still black.
        assert_eq!(pixel(&image, 16, 6, 6), [0, 0, 0]);
    }

    #[test]
    fn rejects_other_files_and_odd_sizes() {
        assert_eq!(
            Roq::open(Arc::from(&b"RIFF0000WAVE"[..])).err(),
            Some(RoqError::Signature)
        );
        let mut no_size = header(30);
        no_size.extend(chunk(QUAD_VQ, 0, &[0, 0]));
        assert_eq!(Roq::open(no_size.into()).err(), Some(RoqError::Size));
        let mut huge = header(30);
        huge.extend(info(4096, 16));
        assert_eq!(Roq::open(huge.into()).err(), Some(RoqError::Size));
    }

    #[test]
    fn a_truncated_frame_keeps_what_it_decoded() {
        let mut file = header(30);
        file.extend(info(32, 16));
        file.extend(chunk(CODEBOOK, 1 << 8, &[90, 90, 90, 90, 128, 128]));
        // Codes for the first macroblock only; the second has no bytes left.
        let mut frame = flags(&[2, 2, 2, 2]).to_vec();
        frame.extend_from_slice(&[0, 0, 0, 0]);
        file.extend(chunk(QUAD_VQ, 0, &frame));
        let mut video = Roq::open(file.into()).expect("a RoQ file");
        assert!(video.advance());
        let image = rgba(&video);
        assert_eq!(pixel(&image, 32, 0, 0), [90, 90, 90]);
        assert_eq!(pixel(&image, 32, 20, 0), [0, 0, 0]);
    }
}
