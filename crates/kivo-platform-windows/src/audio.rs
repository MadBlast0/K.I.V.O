//! Audio capture and playback through WASAPI (VOICE §1, VOICE-01). Shared mode, event-driven: the
//! stream thread sleeps until the device has a buffer ready, so nothing polls. Streams ask for
//! 32-bit float at the device's own rate and channel count (Windows converts if it must); KIVO
//! resamples to 16 kHz mono itself. Stream threads run under MMCSS "Audio" for steady timing.

use kivo_platform::{
    AudioDevice, AudioIo, AudioStream, DeviceId, FrameSink, FrameSource, PlatformError,
    PlatformResult, StreamFormat,
};
use std::sync::mpsc;
use std::thread::JoinHandle;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
    DEVICE_STATE_ACTIVE, EDataFlow, IAudioCaptureClient, IAudioClient, IAudioRenderClient,
    IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, eCapture, eConsole, eRender,
};
use windows::Win32::Media::Multimedia::WAVE_FORMAT_IEEE_FLOAT;
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize, STGM_READ,
};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW, INFINITE,
    SetEvent, WaitForMultipleObjects,
};
use windows::core::{HSTRING, PWSTR, w};

/// The shared-mode buffer asked for: 100 ms (the engine picks its own period inside it).
const BUFFER_100NS: i64 = 1_000_000;

fn os_error(e: &windows::core::Error) -> PlatformError {
    PlatformError::Os {
        code: i64::from(e.code().0),
        message: e.message(),
    }
}

/// COM for the calling thread, released when dropped.
struct Com;

impl Com {
    fn init() -> PlatformResult<Self> {
        // SAFETY: initializing COM on this thread; balanced by `CoUninitialize` in Drop. An
        // already-initialized thread (S_FALSE) still needs the matching uninitialize.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|e| os_error(&e))?;
        Ok(Self)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: balances the successful `CoInitializeEx` in `init` on this thread.
        unsafe { CoUninitialize() };
    }
}

fn enumerator() -> PlatformResult<IMMDeviceEnumerator> {
    // SAFETY: creating the system device enumerator on a COM-initialized thread.
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(|e| os_error(&e))
}

fn device_id(device: &IMMDevice) -> PlatformResult<String> {
    // SAFETY: GetId allocates a string that is freed here after copying.
    unsafe {
        let id: PWSTR = device.GetId().map_err(|e| os_error(&e))?;
        let text = id.to_string().unwrap_or_default();
        CoTaskMemFree(Some(id.0.cast_const().cast()));
        Ok(text)
    }
}

fn friendly_name(device: &IMMDevice) -> String {
    // SAFETY: reading one property; the allocated string is freed after copying.
    let name = unsafe {
        device
            .OpenPropertyStore(STGM_READ)
            .and_then(|store| store.GetValue(&PKEY_Device_FriendlyName))
            .and_then(|value| PropVariantToStringAlloc(&raw const value))
            .map(|s| {
                let text = s.to_string().unwrap_or_default();
                CoTaskMemFree(Some(s.0.cast_const().cast()));
                text
            })
    };
    name.unwrap_or_else(|_| "Audio device".into())
}

fn list(flow: EDataFlow) -> PlatformResult<Vec<AudioDevice>> {
    let _com = Com::init()?;
    let enumerator = enumerator()?;
    // SAFETY: plain COM calls; indexes stay below the count.
    unsafe {
        let default = enumerator
            .GetDefaultAudioEndpoint(flow, eConsole)
            .ok()
            .and_then(|d| device_id(&d).ok());
        let collection = enumerator
            .EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)
            .map_err(|e| os_error(&e))?;
        let mut devices = Vec::new();
        for i in 0..collection.GetCount().map_err(|e| os_error(&e))? {
            let device = collection.Item(i).map_err(|e| os_error(&e))?;
            let id = device_id(&device)?;
            devices.push(AudioDevice {
                is_default: default.as_deref() == Some(id.as_str()),
                name: friendly_name(&device),
                id: DeviceId(id),
            });
        }
        Ok(devices)
    }
}

fn open_device(flow: EDataFlow, id: Option<&DeviceId>) -> PlatformResult<IMMDevice> {
    let enumerator = enumerator()?;
    // SAFETY: plain COM calls on the enumerator.
    unsafe {
        match id {
            Some(id) => enumerator
                .GetDevice(&HSTRING::from(id.0.as_str()))
                .map_err(|_| PlatformError::NotFound(format!("the audio device {}", id.0))),
            None => enumerator
                .GetDefaultAudioEndpoint(flow, eConsole)
                .map_err(|_| PlatformError::NotFound("a default audio device".into())),
        }
    }
}

/// Activates and initializes an event-driven shared-mode client in 32-bit float.
fn start_client(device: &IMMDevice, event: HANDLE) -> PlatformResult<(IAudioClient, StreamFormat)> {
    // SAFETY: COM calls on a live device; the mix format is freed after it is read.
    unsafe {
        let client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| os_error(&e))?;
        let mix = client.GetMixFormat().map_err(|e| os_error(&e))?;
        let (rate, channels) = ((*mix).nSamplesPerSec, (*mix).nChannels);
        CoTaskMemFree(Some(mix.cast_const().cast()));
        let block = channels * 4;
        #[allow(clippy::cast_possible_truncation, reason = "the format tag is 3")]
        let format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_IEEE_FLOAT as u16,
            nChannels: channels,
            nSamplesPerSec: rate,
            nAvgBytesPerSec: rate * u32::from(block),
            nBlockAlign: block,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                BUFFER_100NS,
                0,
                &raw const format,
                None,
            )
            .map_err(|e| os_error(&e))?;
        client.SetEventHandle(event).map_err(|e| os_error(&e))?;
        Ok((
            client,
            StreamFormat {
                sample_rate: rate,
                channels,
            },
        ))
    }
}

struct Event(HANDLE);

impl Event {
    fn new() -> PlatformResult<Self> {
        // SAFETY: an unnamed auto-reset event, closed in Drop.
        unsafe { CreateEventW(None, false, false, None) }
            .map(Self)
            .map_err(|e| os_error(&e))
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: closing the handle this struct owns.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

// SAFETY: event handles may be signalled and waited on from any thread.
unsafe impl Send for Event {}
// SAFETY: as above; the handle itself is never changed.
unsafe impl Sync for Event {}

/// A running stream: its thread and the event that stops it.
struct Stream {
    format: StreamFormat,
    stop: std::sync::Arc<Event>,
    thread: Option<JoinHandle<()>>,
}

impl AudioStream for Stream {
    fn format(&self) -> StreamFormat {
        self.format
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: signalling the event this stream owns.
        let _ = unsafe { SetEvent(self.stop.0) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Moves audio between the device and KIVO each time the device signals. It holds COM
/// interfaces, so it is built on the stream thread by a `MakePump`.
type Pump = Box<dyn FnMut(&IAudioClient, StreamFormat) -> PlatformResult<()>>;
type MakePump = Box<dyn FnOnce() -> Pump + Send>;

/// Opens a device on a new thread, starts it, then runs the pump each time it signals, until the
/// stream is dropped or the device goes away.
fn spawn(
    flow: EDataFlow,
    id: Option<DeviceId>,
    name: &str,
    make_pump: MakePump,
) -> PlatformResult<Stream> {
    let stop = std::sync::Arc::new(Event::new()?);
    let thread_stop = std::sync::Arc::clone(&stop);
    let (ready_tx, ready_rx) = mpsc::channel::<PlatformResult<StreamFormat>>();
    let thread = std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let setup = || -> PlatformResult<(Com, Event, IAudioClient, StreamFormat)> {
                let com = Com::init()?;
                let ready = Event::new()?;
                let device = open_device(flow, id.as_ref())?;
                let (client, format) = start_client(&device, ready.0)?;
                Ok((com, ready, client, format))
            };
            let (_com, ready, client, format) = match setup() {
                Ok(parts) => parts,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let mut pump = make_pump();
            let mut task = 0;
            // SAFETY: registering this thread with MMCSS; reverted before the thread ends.
            let mmcss = unsafe { AvSetMmThreadCharacteristicsW(w!("Audio"), &raw mut task) };
            // Playback fills its first buffer before the stream starts.
            let started = pump(&client, format)
                // SAFETY: starting the initialized client.
                .and_then(|()| unsafe { client.Start() }.map_err(|e| os_error(&e)));
            let failed = started.is_err();
            let _ = ready_tx.send(started.map(|()| format));
            if !failed {
                loop {
                    // SAFETY: waiting on two live event handles.
                    let woke = unsafe {
                        WaitForMultipleObjects(&[ready.0, thread_stop.0], false, INFINITE)
                    };
                    if woke != WAIT_OBJECT_0 {
                        break; // stop requested (or the wait failed)
                    }
                    if let Err(e) = pump(&client, format) {
                        tracing::warn!(%e, "audio stream ended (device removed or changed?)");
                        break;
                    }
                }
            }
            // SAFETY: stopping the client and leaving MMCSS on the thread that joined it.
            unsafe {
                let _ = client.Stop();
                if let Ok(handle) = mmcss {
                    let _ = AvRevertMmThreadCharacteristics(handle);
                }
            }
        })
        .map_err(|e| PlatformError::Os {
            code: 0,
            message: e.to_string(),
        })?;
    let format = ready_rx.recv().map_err(|_| PlatformError::Cancelled)??;
    Ok(Stream {
        format,
        stop,
        thread: Some(thread),
    })
}

/// Pulls every captured packet into `sink` (silent packets as zeros).
fn capture_pump(mut sink: FrameSink) -> Pump {
    let mut capture: Option<IAudioCaptureClient> = None;
    let mut silence = Vec::new();
    Box::new(move |client, format| {
        let capture = match &capture {
            Some(c) => c,
            // SAFETY: the capture service of an initialized client.
            None => capture.insert(unsafe { client.GetService() }.map_err(|e| os_error(&e))?),
        };
        let channels = usize::from(format.channels);
        // SAFETY: GetBuffer/ReleaseBuffer pairs; the slice lives only between them.
        unsafe {
            while capture.GetNextPacketSize().map_err(|e| os_error(&e))? > 0 {
                let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
                capture
                    .GetBuffer(&raw mut data, &raw mut frames, &raw mut flags, None, None)
                    .map_err(|e| os_error(&e))?;
                let samples = frames as usize * channels;
                #[allow(clippy::cast_sign_loss, reason = "a flag bit")]
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    silence.resize(samples, 0.0);
                    sink(&silence, format);
                } else {
                    sink(
                        std::slice::from_raw_parts(data.cast::<f32>(), samples),
                        format,
                    );
                }
                capture.ReleaseBuffer(frames).map_err(|e| os_error(&e))?;
            }
        }
        Ok(())
    })
}

/// Fills the free part of the device buffer from `source`.
fn playback_pump(mut source: FrameSource) -> Pump {
    let mut render: Option<(IAudioRenderClient, u32)> = None;
    Box::new(move |client, format| {
        let (render, size) = match &render {
            Some(r) => r,
            None => render.insert((
                // SAFETY: the render service and buffer size of an initialized client.
                unsafe { client.GetService() }.map_err(|e| os_error(&e))?,
                unsafe { client.GetBufferSize() }.map_err(|e| os_error(&e))?,
            )),
        };
        // SAFETY: GetBuffer/ReleaseBuffer pair over exactly the free frames.
        unsafe {
            let padding = client.GetCurrentPadding().map_err(|e| os_error(&e))?;
            let frames = size.saturating_sub(padding);
            if frames == 0 {
                return Ok(());
            }
            let data = render.GetBuffer(frames).map_err(|e| os_error(&e))?;
            let samples = frames as usize * usize::from(format.channels);
            let buffer = std::slice::from_raw_parts_mut(data.cast::<f32>(), samples);
            buffer.fill(0.0);
            source(buffer, format);
            render.ReleaseBuffer(frames, 0).map_err(|e| os_error(&e))?;
        }
        Ok(())
    })
}

pub struct WindowsAudio;

impl AudioIo for WindowsAudio {
    fn input_devices(&self) -> PlatformResult<Vec<AudioDevice>> {
        list(eCapture)
    }

    fn output_devices(&self) -> PlatformResult<Vec<AudioDevice>> {
        list(eRender)
    }

    fn open_capture(
        &self,
        device: Option<&DeviceId>,
        sink: FrameSink,
    ) -> PlatformResult<Box<dyn AudioStream>> {
        let make: MakePump = Box::new(move || capture_pump(sink));
        Ok(Box::new(spawn(
            eCapture,
            device.cloned(),
            "kivo-capture",
            make,
        )?))
    }

    fn open_playback(
        &self,
        device: Option<&DeviceId>,
        source: FrameSource,
    ) -> PlatformResult<Box<dyn AudioStream>> {
        let make: MakePump = Box::new(move || playback_pump(source));
        Ok(Box::new(spawn(
            eRender,
            device.cloned(),
            "kivo-playback",
            make,
        )?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn lists_devices_with_one_default_per_direction() {
        for devices in [WindowsAudio.input_devices(), WindowsAudio.output_devices()] {
            let devices = devices.unwrap();
            println!("{devices:#?}");
            assert!(devices.iter().filter(|d| d.is_default).count() <= 1);
            assert!(
                devices
                    .iter()
                    .all(|d| !d.id.0.is_empty() && !d.name.is_empty())
            );
        }
    }

    /// Opens the default microphone for half a second. Skipped quietly on machines without one
    /// (CI runners); run it on a PC with a mic to check real capture.
    #[test]
    fn captures_from_the_default_microphone() {
        if WindowsAudio.input_devices().map_or(true, |d| d.is_empty()) {
            eprintln!("no microphone; skipped");
            return;
        }
        let frames = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&frames);
        let stream = WindowsAudio
            .open_capture(
                None,
                Box::new(move |samples, format| {
                    counted.fetch_add(
                        samples.len() / usize::from(format.channels),
                        Ordering::Relaxed,
                    );
                }),
            )
            .unwrap();
        let format = stream.format();
        std::thread::sleep(Duration::from_millis(500));
        drop(stream);
        let got = frames.load(Ordering::Relaxed);
        println!("{format:?}: {got} frames in 500 ms");
        // Half a second at the device rate, allowing for start-up and the last partial period.
        let expected = format.sample_rate as usize / 2;
        assert!(got > expected / 2 && got <= expected + format.sample_rate as usize / 10);
    }

    /// Plays 300 ms of silence on the default output and checks the device kept asking for it.
    #[test]
    fn plays_to_the_default_output() {
        if WindowsAudio.output_devices().map_or(true, |d| d.is_empty()) {
            eprintln!("no speakers; skipped");
            return;
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let stream = WindowsAudio
            .open_playback(
                None,
                Box::new(move |buffer, _| {
                    buffer.fill(0.0);
                    counted.fetch_add(1, Ordering::Relaxed);
                }),
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        drop(stream);
        assert!(
            calls.load(Ordering::Relaxed) >= 5,
            "the device pulled buffers repeatedly"
        );
    }
}
