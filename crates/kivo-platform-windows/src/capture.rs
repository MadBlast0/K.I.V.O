//! Screen capture with Windows.Graphics.Capture (TOOLS_AND_CONTROL §3, TOOL-14): one frame of a
//! monitor or a window, on request only, copied out of the GPU into memory. Nothing is captured
//! continuously and nothing is written to disk here.

use crate::com::os_error;
use kivo_platform::{CaptureTarget, Image, PlatformError, PlatformResult, Screen};
use std::time::{Duration, Instant};
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::{HMODULE, HWND, POINT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    MonitorFromWindow,
};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::Interface;

/// A rectangle inside the captured image: x, y, width, height.
type Crop = (i32, i32, u32, u32);

/// How long to wait for the first frame.
const FRAME_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct WindowsScreen;

fn d3d_device() -> PlatformResult<(ID3D11Device, ID3D11DeviceContext, IDirect3DDevice)> {
    let mut device = None;
    let mut context = None;
    // SAFETY: standard device creation; the out-params are filled on success.
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
        .map_err(|e| os_error(&e))?;
    }
    let (Some(device), Some(context)) = (device, context) else {
        return Err(PlatformError::Unsupported);
    };
    let dxgi: IDXGIDevice = device.cast().map_err(|e| os_error(&e))?;
    // SAFETY: wraps the DXGI device for WinRT.
    let winrt = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }.map_err(|e| os_error(&e))?;
    Ok((device, context, winrt.cast().map_err(|e| os_error(&e))?))
}

fn item_for(target: CaptureTarget) -> PlatformResult<(GraphicsCaptureItem, Option<Crop>)> {
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(|e| os_error(&e))?;
    // SAFETY: the handles come from the OS for the current session.
    unsafe {
        match target {
            CaptureTarget::ActiveMonitor => {
                let monitor = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTONEAREST);
                Ok((
                    interop
                        .CreateForMonitor(monitor)
                        .map_err(|e| os_error(&e))?,
                    None,
                ))
            }
            CaptureTarget::Window { id } => {
                let hwnd = HWND(id.0 as *mut _);
                let item = interop
                    .CreateForWindow(hwnd)
                    .map_err(|_| PlatformError::NotFound("that window".into()))?;
                Ok((item, None))
            }
            CaptureTarget::Region { rect } => {
                let center = POINT {
                    x: rect.center().x,
                    y: rect.center().y,
                };
                let monitor: HMONITOR = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
                let mut info = MONITORINFO {
                    cbSize: u32::try_from(size_of::<MONITORINFO>()).unwrap_or(0),
                    ..Default::default()
                };
                let _ = GetMonitorInfoW(monitor, &mut info);
                // The region relative to the monitor's top-left.
                let crop = (
                    rect.x - info.rcMonitor.left,
                    rect.y - info.rcMonitor.top,
                    rect.width,
                    rect.height,
                );
                Ok((
                    interop
                        .CreateForMonitor(monitor)
                        .map_err(|e| os_error(&e))?,
                    Some(crop),
                ))
            }
        }
    }
}

fn grab(target: CaptureTarget) -> PlatformResult<Image> {
    let (device, context, winrt_device) = d3d_device()?;
    let (item, crop) = item_for(target)?;
    let size = item.Size().map_err(|e| os_error(&e))?;
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &winrt_device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        1,
        size,
    )
    .map_err(|e| os_error(&e))?;
    let session = pool.CreateCaptureSession(&item).map_err(|e| os_error(&e))?;
    let _ = session.SetIsCursorCaptureEnabled(false);
    // No yellow capture border for a single frame (Windows 11; ignored where unsupported).
    let _ = session.SetIsBorderRequired(false);
    session.StartCapture().map_err(|e| os_error(&e))?;
    let started = Instant::now();
    let frame = loop {
        if let Ok(frame) = pool.TryGetNextFrame() {
            break frame;
        }
        if started.elapsed() > FRAME_TIMEOUT {
            let _ = session.Close();
            let _ = pool.Close();
            return Err(PlatformError::Os {
                code: 0,
                message: "no frame arrived".into(),
            });
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let access: IDirect3DDxgiInterfaceAccess = frame
        .Surface()
        .map_err(|e| os_error(&e))?
        .cast()
        .map_err(|e| os_error(&e))?;
    // SAFETY: the surface is a D3D11 texture on `device`.
    let texture: ID3D11Texture2D = unsafe { access.GetInterface() }.map_err(|e| os_error(&e))?;
    let image = read_texture(&device, &context, &texture, crop);
    let _ = frame.Close();
    let _ = session.Close();
    let _ = pool.Close();
    image
}

/// Copies a BGRA texture into CPU memory as RGBA, optionally cropped.
fn read_texture(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    texture: &ID3D11Texture2D,
    crop: Option<Crop>,
) -> PlatformResult<Image> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    // SAFETY: fills `desc`.
    unsafe { texture.GetDesc(&mut desc) };
    let staging_desc = D3D11_TEXTURE2D_DESC {
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
        ..desc
    };
    let mut staging = None;
    // SAFETY: standard staging copy and map; the mapping is released before returning.
    unsafe {
        device
            .CreateTexture2D(&staging_desc, None, Some(&mut staging))
            .map_err(|e| os_error(&e))?;
        let staging = staging.ok_or(PlatformError::Unsupported)?;
        context.CopyResource(&staging, texture);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context
            .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .map_err(|e| os_error(&e))?;
        let (full_w, full_h) = (desc.Width, desc.Height);
        let (x0, y0, w, h) = match crop {
            Some((x, y, w, h)) => {
                let x = u32::try_from(x.max(0)).unwrap_or(0).min(full_w);
                let y = u32::try_from(y.max(0)).unwrap_or(0).min(full_h);
                (x, y, w.min(full_w - x), h.min(full_h - y))
            }
            None => (0, 0, full_w, full_h),
        };
        let pitch = mapped.RowPitch as usize;
        let data = std::slice::from_raw_parts(mapped.pData.cast::<u8>(), pitch * full_h as usize);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for row in y0..y0 + h {
            let start = row as usize * pitch + x0 as usize * 4;
            for px in data[start..start + w as usize * 4].chunks_exact(4) {
                rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
        }
        context.Unmap(&staging, 0);
        Ok(Image {
            width: w,
            height: h,
            rgba,
        })
    }
}

impl Screen for WindowsScreen {
    fn capture(&self, target: CaptureTarget) -> PlatformResult<Image> {
        grab(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_one_frame_of_the_active_monitor() {
        let image = WindowsScreen.capture(CaptureTarget::ActiveMonitor).unwrap();
        assert!(image.width >= 640 && image.height >= 480, "{image:?}");
        assert_eq!(image.rgba.len(), (image.width * image.height * 4) as usize);
        // A real desktop is never one flat colour.
        let first = &image.rgba[..4];
        assert!(image.rgba.chunks_exact(4).any(|p| p != first));
    }

    #[test]
    fn a_region_is_cropped_from_its_monitor() {
        let rect = kivo_platform::Rect {
            x: 10,
            y: 10,
            width: 200,
            height: 100,
        };
        let image = WindowsScreen
            .capture(CaptureTarget::Region { rect })
            .unwrap();
        assert_eq!((image.width, image.height), (200, 100));
    }
}
