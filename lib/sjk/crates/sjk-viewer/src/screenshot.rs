//! Deferred screenshots of the final post-HUD render target.

use crate::capture;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};

const BYTES_PER_PIXEL: u32 = 4;
const COPY_ALIGNMENT: u32 = 256;
const MAX_AUTOMATIC_NAMES: u32 = 10_000;

pub(crate) struct Setup {
    pub(crate) surface_usage: wgpu::TextureUsages,
    pub(crate) manager: Manager,
}

pub(crate) fn setup(
    context: &crate::gpu_context::Context,
    console: Option<&crate::console::ViewerConsole>,
    fallback_directory: &Path,
) -> Setup {
    let copy_supported = context.surface.is_none()
        || context
            .surface_usages
            .contains(wgpu::TextureUsages::COPY_SRC);
    let surface_usage = wgpu::TextureUsages::RENDER_ATTACHMENT
        | if copy_supported {
            wgpu::TextureUsages::COPY_SRC
        } else {
            wgpu::TextureUsages::empty()
        };
    let directory = console
        .map(|console| console.config_directory().to_owned())
        .unwrap_or_else(|| fallback_directory.to_owned());
    let manager = Manager::new(directory, context.format, copy_supported);
    Setup {
        surface_usage,
        manager,
    }
}

/// Console-selected screenshot encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Format {
    Png,
    Jpeg,
}

/// One deferred renderer screenshot request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Request {
    format: Format,
    name: Option<String>,
    silent: bool,
    /// The world note (its identity service tag) that also gets a smaller copy.
    note: Option<u64>,
}

impl Request {
    /// A silent JPEG named `name` (a simple file name without extension).
    pub(crate) fn jpeg_named(name: &str) -> Self {
        Self {
            format: Format::Jpeg,
            name: Some(name.to_owned()),
            silent: true,
            note: None,
        }
    }

    /// Also hand a smaller copy to the identity service for the world note `tag`.
    pub(crate) fn for_note(self, tag: Option<u64>) -> Self {
        Self { note: tag, ..self }
    }
}

/// Parse OpenJK-style screenshot commands registered by the renderer.
///
/// `R_ScreenShot*_f` accepts an optional explicit name at
/// `codemp/rd-vanilla/tr_init.cpp:1164-1193,1196-1224`.
pub(crate) fn parse_request(tokens: &[String]) -> Result<Option<Request>, String> {
    let Some(command) = tokens.first() else {
        return Ok(None);
    };
    let format = if command.eq_ignore_ascii_case("screenshot") {
        Format::Png
    } else if command.eq_ignore_ascii_case("screenshotjpeg") {
        Format::Jpeg
    } else {
        return Ok(None);
    };
    if tokens.len() > 2 {
        return Err(format!("usage: {command} [filename|silent]"));
    }
    let argument = tokens.get(1).map(String::as_str);
    let silent = argument.is_some_and(|value| value.eq_ignore_ascii_case("silent"));
    let name = argument
        .filter(|_| !silent)
        .map(validate_name)
        .transpose()?;
    Ok(Some(Request {
        format,
        name,
        silent,
        note: None,
    }))
}

enum State {
    Idle,
    Encoded(Readback),
    /// The readback with the receiver of its own map callback, so a callback from an
    /// earlier readback can never be mistaken for this one.
    Mapping(Readback, Receiver<Result<(), wgpu::BufferAsyncError>>),
}

struct Readback {
    buffer: wgpu::Buffer,
    padded_row_bytes: u32,
    size: [u32; 2],
    path: PathBuf,
    format: Format,
    silent: bool,
    note: Option<u64>,
}

/// One-request-at-a-time asynchronous screenshot readback and writer.
pub(crate) struct Manager {
    directory: PathBuf,
    request: Option<Request>,
    state: State,
    write_sender: SyncSender<String>,
    write_receiver: Receiver<String>,
    write_pending: bool,
    texture_format: wgpu::TextureFormat,
    copy_supported: bool,
}

impl Manager {
    pub(crate) fn new(
        config_directory: PathBuf,
        texture_format: wgpu::TextureFormat,
        copy_supported: bool,
    ) -> Self {
        let (write_sender, write_receiver) = sync_channel(1);
        Self {
            directory: config_directory.join("screenshots"),
            request: None,
            state: State::Idle,
            write_sender,
            write_receiver,
            write_pending: false,
            texture_format,
            copy_supported,
        }
    }

    /// Where screenshots are written.
    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn request(&mut self, request: Request) {
        if self.request.is_none() && matches!(self.state, State::Idle) && !self.write_pending {
            self.request = Some(request);
        }
    }

    pub(crate) fn encode(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
        size: [u32; 2],
    ) -> Result<bool, String> {
        let Some(request) = self.request.take() else {
            return Ok(false);
        };
        if !self.copy_supported {
            return Err("surface does not support screenshot readback".to_owned());
        }
        let directory = &self.directory;
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let path = screenshot_path(directory, &request)?;
        let row_bytes = size[0] * BYTES_PER_PIXEL;
        let padded_row_bytes = row_bytes.div_ceil(COPY_ALIGNMENT) * COPY_ALIGNMENT;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK console screenshot readback"),
            size: u64::from(padded_row_bytes) * u64::from(size[1]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: Some(size[1]),
                },
            },
            wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
        );
        self.state = State::Encoded(Readback {
            buffer,
            padded_row_bytes,
            size,
            path,
            format: request.format,
            silent: request.silent,
            note: request.note,
        });
        Ok(true)
    }

    /// Called before every frame's swapchain acquire. Only an encoded readback is taken
    /// out of the state: a readback still mapping must stay, since dropping its buffer
    /// aborts the map (wgpu `MapAborted`).
    pub(crate) fn after_submit(&mut self) {
        if !matches!(self.state, State::Encoded(_)) {
            return;
        }
        let State::Encoded(readback) = std::mem::replace(&mut self.state, State::Idle) else {
            unreachable!("state checked above");
        };
        // One slot per readback: the callback never blocks the thread that polls.
        let (sender, receiver) = sync_channel(1);
        readback
            .buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.try_send(result);
            });
        self.state = State::Mapping(readback, receiver);
    }

    pub(crate) fn poll(&mut self, device: &wgpu::Device) -> Option<String> {
        if let Ok(message) = self.write_receiver.try_recv() {
            self.write_pending = false;
            return Some(message);
        }
        let State::Mapping(_, receiver) = &self.state else {
            return None;
        };
        let _ = device.poll(wgpu::PollType::Poll);
        match receiver.try_recv() {
            Ok(Ok(())) => self.finish_mapping(),
            Ok(Err(error)) => {
                self.state = State::Idle;
                Some(format!("^1Screenshot readback failed: {error}"))
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.state = State::Idle;
                Some("^1Screenshot readback channel closed".to_owned())
            }
        }
    }

    fn finish_mapping(&mut self) -> Option<String> {
        let State::Mapping(readback, _) = std::mem::replace(&mut self.state, State::Idle) else {
            return None;
        };
        let slice = readback.buffer.slice(..);
        let mapped = match slice.get_mapped_range() {
            Ok(mapped) => mapped,
            Err(error) => return Some(format!("^1Screenshot map failed: {error}")),
        };
        let row_bytes = (readback.size[0] * BYTES_PER_PIXEL) as usize;
        let mut pixels = vec![0; row_bytes * readback.size[1] as usize];
        for (source, destination) in mapped
            .chunks(readback.padded_row_bytes as usize)
            .zip(pixels.chunks_mut(row_bytes))
        {
            destination.copy_from_slice(&source[..row_bytes]);
        }
        if matches!(
            self.texture_format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        drop(mapped);
        readback.buffer.unmap();
        let sender = self.write_sender.clone();
        self.write_pending = true;
        std::thread::spawn(move || {
            let result = match readback.format {
                Format::Png => capture::write_png(&readback.path, readback.size, &pixels),
                Format::Jpeg => capture::write_jpeg(&readback.path, readback.size, &pixels),
            };
            if let Some(tag) = readback.note {
                match capture::preview_jpeg(readback.size, &pixels) {
                    Ok(jpeg) => crate::player_identity::note_image(tag, jpeg),
                    Err(error) => {
                        crate::log::progress(format_args!("note picture not sent: {error}"))
                    }
                }
            }
            let message = match result {
                Ok(()) if readback.silent => String::new(),
                Ok(()) => format!("Wrote {}", readback.path.display()),
                Err(error) => format!("^1Could not write screenshot: {error}"),
            };
            let _ = sender.send(message);
        });
        None
    }
}

impl crate::GpuState {
    pub(crate) fn poll_console_screenshot(&mut self) {
        if let Some(message) = self.screenshots.poll(&self.device)
            && !message.is_empty()
        {
            if let Some(console) = &mut self.console {
                console.push_log(message);
            }
        }
    }

    pub(crate) fn encode_console_screenshot(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        frame: Option<&wgpu::SurfaceTexture>,
    ) -> bool {
        let Some(frame) = frame else { return false };
        let texture = &frame.texture;
        match self.screenshots.encode(
            &self.device,
            encoder,
            texture,
            [self.size.width, self.size.height],
        ) {
            Ok(encoded) => encoded,
            Err(error) => {
                if let Some(console) = &mut self.console {
                    console.push_log(format!("^1Screenshot failed: {error}"));
                }
                false
            }
        }
    }
}

fn screenshot_path(directory: &Path, request: &Request) -> Result<PathBuf, String> {
    let extension = match request.format {
        Format::Png => "png",
        Format::Jpeg => "jpg",
    };
    if let Some(name) = &request.name {
        return Ok(directory.join(format!("{name}.{extension}")));
    }
    for index in 0..MAX_AUTOMATIC_NAMES {
        let path = directory.join(format!("shot{index:04}.{extension}"));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err("ScreenShot: Couldn't create a file".to_owned())
}

fn validate_name(name: &str) -> Result<String, String> {
    if name.is_empty()
        || name.contains(['/', '\\'])
        || name == "."
        || name == ".."
        || name.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err("screenshot filename must be a simple file name".to_owned());
    }
    Ok(name
        .strip_suffix(".png")
        .or_else(|| name.strip_suffix(".jpg"))
        .unwrap_or(name)
        .to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frames keep calling `after_submit` while the readback maps; the readback must
    /// survive them and reach the file (it was dropped, aborting the map).
    #[test]
    fn readback_survives_frames_while_mapping() {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
            eprintln!("no GPU adapter; skipped");
            return;
        };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).expect("device");
        let directory = std::env::temp_dir().join(format!("sjk-shot-{}", std::process::id()));
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let mut manager = Manager::new(directory.clone(), format, true);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 64,
                height: 32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let frame = |manager: &mut Manager, shot: bool| {
            let mut encoder = device.create_command_encoder(&Default::default());
            if shot {
                assert_eq!(
                    manager.encode(&device, &mut encoder, &texture, [64, 32]),
                    Ok(true)
                );
            }
            queue.submit([encoder.finish()]);
            manager.after_submit();
        };
        manager.request(Request::jpeg_named("readback"));
        frame(&mut manager, true);
        let mut message = None;
        for _ in 0..2000 {
            // The next frame's acquire runs before the map has landed.
            frame(&mut manager, false);
            message = manager.poll(&device);
            if message.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let written = directory.join("screenshots").join("readback.jpg");
        let exists = written.exists();
        let _ = fs::remove_dir_all(&directory);
        assert_eq!(message.as_deref(), Some(""), "silent shot reports nothing");
        assert!(exists, "screenshot written");
    }
}
