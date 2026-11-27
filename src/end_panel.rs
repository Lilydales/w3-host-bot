//! Direct Strike's end-of-match panel ("VICTORY"/"DEFEAT" + Quit) is drawn by the map, not
//! by Warcraft III, so no UI-socket message announces it. It is recognised from a window
//! capture instead, and its Quit button is clicked like a player would.
//!
//! Positions come from the map script (war3map.j): the panel's Quit button is a 0.08 x 0.04
//! frame centred at (0.675, 0.21) and the discord link box sits left of it, both on the
//! 0.8 x 0.6 frame grid that Warcraft III centres in a 4:3 area of the client window.
//! The reference patches were sampled from a real 640 x 360 capture of that panel.
use crate::window_capture::Frame;

/// Frame-grid box (x0, x1, y0, y1), sample grid size and reference grayscale cells.
struct Patch {
    area: [f32; 4],
    columns: usize,
    rows: usize,
    reference: &'static [u8],
}

const QUIT: Patch = Patch {
    area: [0.632, 0.718, 0.188, 0.232],
    columns: 16,
    rows: 8,
    reference: &[
        38, 37, 38, 44, 44, 42, 35, 32, 35, 39, 37, 35, 36, 35, 34, 34, 41, 49, 115, 118, 115,
        115, 113, 113, 110, 114, 112, 112, 113, 113, 110, 42, 41, 57, 55, 20, 20, 21, 21, 21, 19,
        20, 22, 22, 22, 20, 47, 53, 39, 52, 44, 20, 21, 21, 37, 46, 21, 24, 29, 22, 22, 22, 38,
        47, 37, 50, 44, 20, 20, 22, 79, 53, 94, 79, 87, 22, 22, 22, 38, 47, 35, 47, 43, 19, 20,
        22, 49, 99, 66, 60, 51, 21, 23, 23, 39, 48, 32, 47, 38, 19, 22, 23, 22, 22, 22, 21, 22,
        22, 23, 22, 38, 48, 33, 36, 49, 48, 51, 52, 51, 51, 49, 47, 46, 46, 46, 46, 47, 37,
    ],
};

const LINK: Patch = Patch {
    area: [0.31, 0.495, 0.188, 0.222],
    columns: 24,
    rows: 6,
    reference: &[
        42, 42, 43, 40, 34, 36, 35, 34, 33, 33, 35, 36, 35, 34, 34, 34, 34, 33, 35, 35, 34, 34,
        34, 34, 43, 76, 77, 75, 74, 74, 74, 73, 73, 74, 73, 74, 74, 74, 74, 73, 73, 73, 73, 74,
        73, 73, 80, 37, 45, 42, 32, 20, 20, 20, 31, 20, 30, 45, 49, 33, 42, 47, 47, 52, 18, 15,
        13, 13, 13, 12, 23, 41, 46, 39, 102, 105, 90, 109, 82, 126, 84, 81, 114, 108, 86, 92, 91,
        94, 109, 50, 13, 13, 13, 13, 21, 39, 46, 40, 20, 17, 21, 18, 25, 75, 45, 17, 18, 21, 20,
        18, 21, 17, 18, 15, 14, 14, 13, 13, 21, 41, 46, 46, 37, 32, 34, 35, 36, 39, 38, 37, 37,
        37, 37, 37, 37, 38, 38, 35, 39, 39, 37, 36, 42, 37,
    ],
};

/// Both patches must correlate at least this well. Random game scenes stayed below 0.66
/// for the button and 0.45 for the link box when this was tuned.
pub const THRESHOLD: f32 = 0.75;
/// Pixels searched around the expected position, to absorb rounding and borders.
const SEARCH: i32 = 3;

/// Client-area pixel position of a point on the map's 0.8 x 0.6 frame grid.
pub fn frame_point(width: f32, height: f32, x: f32, y: f32) -> (f32, f32) {
    let wide = height * 4.0 / 3.0;
    let left = (width - wide) / 2.0;
    (left + x / 0.8 * wide, height - y / 0.6 * height)
}

/// Where the panel's Quit button is, in client pixels.
pub fn quit_point(width: f32, height: f32) -> (f32, f32) {
    frame_point(width, height, 0.675, 0.21)
}

fn gray(frame: &Frame, x: usize, y: usize) -> f32 {
    let i = (y * frame.width as usize + x) * 4;
    let p = &frame.rgba[i..i + 3];
    (f32::from(p[0]) * 299.0 + f32::from(p[1]) * 587.0 + f32::from(p[2]) * 114.0) / 1000.0
}

fn sample(frame: &Frame, patch: &Patch, dx: f32, dy: f32) -> Vec<f32> {
    let (w, h) = (frame.width as f32, frame.height as f32);
    let [x0, x1, y0, y1] = patch.area;
    let (px0, py0) = frame_point(w, h, x0, y1);
    let (px1, py1) = frame_point(w, h, x1, y0);
    let (px0, px1, py0, py1) = (px0 + dx, px1 + dx, py0 + dy, py1 + dy);
    let clamp = |v: f32, max: u32| (v.max(0.0) as usize).min(max as usize - 1);
    let mut cells = Vec::with_capacity(patch.columns * patch.rows);
    for row in 0..patch.rows {
        let b0 = clamp(py0 + (py1 - py0) * row as f32 / patch.rows as f32, frame.height);
        let b1 = clamp(py0 + (py1 - py0) * (row + 1) as f32 / patch.rows as f32, frame.height).max(b0 + 1);
        for column in 0..patch.columns {
            let a0 = clamp(px0 + (px1 - px0) * column as f32 / patch.columns as f32, frame.width);
            let a1 = clamp(px0 + (px1 - px0) * (column + 1) as f32 / patch.columns as f32, frame.width)
                .max(a0 + 1);
            let (mut sum, mut count) = (0.0, 0.0);
            for y in b0..b1.min(frame.height as usize) {
                for x in a0..a1.min(frame.width as usize) {
                    sum += gray(frame, x, y);
                    count += 1.0;
                }
            }
            cells.push(if count > 0.0 { sum / count } else { 0.0 });
        }
    }
    cells
}

/// Normalised cross-correlation: 1 = same picture, ignoring brightness and contrast.
fn correlation(cells: &[f32], reference: &[u8]) -> f32 {
    let n = cells.len() as f32;
    let mean_a = cells.iter().sum::<f32>() / n;
    let mean_b = reference.iter().map(|&v| f32::from(v)).sum::<f32>() / n;
    let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (a, &b) in cells.iter().zip(reference) {
        let (a, b) = (a - mean_a, f32::from(b) - mean_b);
        ab += a * b;
        aa += a * a;
        bb += b * b;
    }
    if aa <= f32::EPSILON || bb <= f32::EPSILON {
        0.0
    } else {
        ab / (aa * bb).sqrt()
    }
}

fn best(frame: &Frame, patch: &Patch) -> f32 {
    let mut best = -1.0f32;
    for dy in -SEARCH..=SEARCH {
        for dx in -SEARCH..=SEARCH {
            best = best.max(correlation(&sample(frame, patch, dx as f32, dy as f32), patch.reference));
        }
    }
    best
}

/// (Quit button, link box) similarity, each from -1 to 1.
pub fn scores(frame: &Frame) -> (f32, f32) {
    if frame.width < 100 || frame.height < 100 || frame.rgba.len() < (frame.width * frame.height * 4) as usize {
        return (0.0, 0.0);
    }
    (best(frame, &QUIT), best(frame, &LINK))
}

pub fn visible(frame: &Frame) -> bool {
    let (quit, link) = scores(frame);
    quit >= THRESHOLD && link >= THRESHOLD
}

#[cfg(windows)]
mod native {
    use std::mem::zeroed;
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT},
        Graphics::Gdi::ClientToScreen,
        UI::{
            Input::KeyboardAndMouse::{
                INPUT, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, SendInput,
            },
            WindowsAndMessaging::{
                EnumWindows, GetClientRect, GetCursorPos, GetWindowTextW, GetWindowThreadProcessId,
                IsWindowVisible, PostMessageW, SetCursorPos, SetForegroundWindow, WM_LBUTTONDOWN,
                WM_LBUTTONUP, WM_MOUSEMOVE,
            },
        },
    };

    struct Search {
        pid: u32,
        found: Option<HWND>,
    }

    unsafe extern "system" fn find(window: HWND, data: LPARAM) -> i32 {
        let search = unsafe { &mut *(data as *mut Search) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(window, &mut pid) };
        if pid == search.pid && unsafe { IsWindowVisible(window) } != 0 {
            let mut title = [0u16; 128];
            let len = unsafe { GetWindowTextW(window, title.as_mut_ptr(), 128) }.max(0) as usize;
            if String::from_utf16_lossy(&title[..len]).starts_with("Warcraft III") {
                search.found = Some(window);
                return 0;
            }
        }
        1
    }

    fn window(pid: u32) -> Result<(HWND, f32, f32), String> {
        let mut search = Search { pid, found: None };
        unsafe { EnumWindows(Some(find), &mut search as *mut Search as LPARAM) };
        let window = search.found.ok_or("The game window was not found.")?;
        let mut rect: RECT = unsafe { zeroed() };
        if unsafe { GetClientRect(window, &mut rect) } == 0 {
            return Err("The game window size is unavailable.".into());
        }
        let (w, h) = ((rect.right - rect.left) as f32, (rect.bottom - rect.top) as f32);
        if w < 100.0 || h < 100.0 {
            return Err("The game window is too small.".into());
        }
        Ok((window, w, h))
    }

    /// Brings the game window to the front. A tap of Alt first lets a background process
    /// take the foreground, which Windows otherwise refuses.
    pub fn focus(pid: u32) -> Result<bool, String> {
        use windows_sys::Win32::UI::{
            Input::KeyboardAndMouse::{INPUT_KEYBOARD, KEYEVENTF_KEYUP, VK_MENU},
            WindowsAndMessaging::{GetForegroundWindow, IsIconic, SW_RESTORE, ShowWindow},
        };
        let (window, _, _) = window(pid)?;
        unsafe {
            if GetForegroundWindow() == window {
                return Ok(false);
            }
            if IsIconic(window) != 0 {
                ShowWindow(window, SW_RESTORE);
            }
            let mut keys: [INPUT; 2] = zeroed();
            for (input, flags) in keys.iter_mut().zip([0, KEYEVENTF_KEYUP]) {
                input.r#type = INPUT_KEYBOARD;
                input.Anonymous.ki.wVk = VK_MENU;
                input.Anonymous.ki.dwFlags = flags;
            }
            SendInput(2, keys.as_ptr(), size_of::<INPUT>() as i32);
            if SetForegroundWindow(window) == 0 {
                return Err("Windows did not allow bringing the game to the front.".into());
            }
        }
        Ok(true)
    }

    /// Posts a click to the window without touching the user's mouse.
    pub fn post_click(pid: u32) -> Result<(), String> {
        let (window, w, h) = window(pid)?;
        let (x, y) = super::quit_point(w, h);
        let position = ((y as isize & 0xffff) << 16) | (x as isize & 0xffff);
        unsafe {
            PostMessageW(window, WM_MOUSEMOVE, 0, position);
            PostMessageW(window, WM_LBUTTONDOWN, 0x0001, position);
            PostMessageW(window, WM_LBUTTONUP, 0, position);
        }
        Ok(())
    }

    /// A real mouse click: brings the game forward, clicks Quit, puts the pointer back.
    pub fn input_click(pid: u32) -> Result<(), String> {
        let (window, w, h) = window(pid)?;
        let (x, y) = super::quit_point(w, h);
        let mut point = POINT { x: x as i32, y: y as i32 };
        let mut previous: POINT = unsafe { zeroed() };
        unsafe {
            if ClientToScreen(window, &mut point) == 0 {
                return Err("The game window position is unavailable.".into());
            }
            GetCursorPos(&mut previous);
            SetForegroundWindow(window);
            SetCursorPos(point.x, point.y);
            let mut inputs: [INPUT; 2] = zeroed();
            for (input, flags) in inputs.iter_mut().zip([MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP]) {
                input.r#type = INPUT_MOUSE;
                input.Anonymous.mi.dwFlags = flags;
            }
            let sent = SendInput(2, inputs.as_ptr(), size_of::<INPUT>() as i32);
            std::thread::sleep(std::time::Duration::from_millis(60));
            SetCursorPos(previous.x, previous.y);
            if sent != 2 {
                return Err("Windows blocked the click.".into());
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
pub use native::{focus, input_click, post_click};

#[cfg(not(windows))]
pub fn focus(_: u32) -> Result<bool, String> {
    Err("Focusing the game requires Windows.".into())
}

#[cfg(not(windows))]
pub fn post_click(_: u32) -> Result<(), String> {
    Err("Clicking requires Windows.".into())
}

#[cfg(not(windows))]
pub fn input_click(_: u32) -> Result<(), String> {
    Err("Clicking requires Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A noisy 640 x 360 picture, optionally with the reference patches painted in place.
    fn picture(width: u32, height: u32, panel: bool) -> Frame {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        let mut seed = 7u32;
        for _ in 0..width * height {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let v = (seed >> 16) as u8;
            rgba.extend_from_slice(&[v, v / 2, v / 3, 255]);
        }
        let mut frame = Frame { width, height, rgba };
        if panel {
            for patch in [&QUIT, &LINK] {
                let (w, h) = (width as f32, height as f32);
                let [x0, x1, y0, y1] = patch.area;
                let (px0, py0) = frame_point(w, h, x0, y1);
                let (px1, py1) = frame_point(w, h, x1, y0);
                for y in py0.floor() as u32..py1.ceil() as u32 {
                    for x in px0.floor() as u32..px1.ceil() as u32 {
                        let column = (((x as f32 - px0) / (px1 - px0)) * patch.columns as f32)
                            .clamp(0.0, patch.columns as f32 - 1.0) as usize;
                        let row = (((y as f32 - py0) / (py1 - py0)) * patch.rows as f32)
                            .clamp(0.0, patch.rows as f32 - 1.0) as usize;
                        let v = patch.reference[row * patch.columns + column];
                        let i = ((y * width + x) * 4) as usize;
                        frame.rgba[i..i + 3].copy_from_slice(&[v, v, v]);
                    }
                }
            }
        }
        frame
    }

    #[test]
    fn the_end_panel_is_recognised_at_any_window_shape_and_noise_is_not() {
        for (w, h) in [(640, 360), (640, 400), (640, 480), (1280, 720)] {
            assert!(visible(&picture(w, h, true)), "{w}x{h}: {:?}", scores(&picture(w, h, true)));
            assert!(!visible(&picture(w, h, false)), "{w}x{h}");
        }
        let blank = Frame { width: 640, height: 360, rgba: vec![30; 640 * 360 * 4] };
        assert!(!visible(&blank));
        assert!(!visible(&Frame { width: 10, height: 10, rgba: vec![] }));
    }

    #[test]
    fn the_quit_button_is_found_on_the_map_frame_grid() {
        let (x, y) = quit_point(1280.0, 720.0);
        assert!((x - 970.0).abs() < 0.5 && (y - 468.0).abs() < 0.5);
        let (x, y) = quit_point(1024.0, 768.0);
        assert!((x - 864.0).abs() < 0.5 && (y - 499.2).abs() < 0.5);
    }
}
