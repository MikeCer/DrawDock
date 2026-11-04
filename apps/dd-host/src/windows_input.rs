use std::{io, mem::size_of};

const INPUT_MOUSE: u32 = 0;
const MOUSEEVENTF_MOVE: u32 = 0x0001;
const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
const MOUSEEVENTF_ABSOLUTE: u32 = 0x8000;
const MOUSEEVENTF_VIRTUALDESK: u32 = 0x4000;
const SM_XVIRTUALSCREEN: i32 = 76;
const SM_YVIRTUALSCREEN: i32 = 77;
const SM_CXVIRTUALSCREEN: i32 = 78;
const SM_CYVIRTUALSCREEN: i32 = 79;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Draw,
    Pointer,
}

impl Mode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "draw" => Ok(Self::Draw),
            "pointer" => Ok(Self::Pointer),
            _ => Err(format!("unsupported input mode {value:?}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draw => "draw",
            Self::Pointer => "pointer",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Button {
    Left,
    Right,
}

impl Button {
    fn down_flag(self) -> u32 {
        match self {
            Self::Left => MOUSEEVENTF_LEFTDOWN,
            Self::Right => MOUSEEVENTF_RIGHTDOWN,
        }
    }

    fn up_flag(self) -> u32 {
        match self {
            Self::Left => MOUSEEVENTF_LEFTUP,
            Self::Right => MOUSEEVENTF_RIGHTUP,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputBounds {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug)]
struct ActivePointer {
    generation: u64,
    button: Button,
}

#[derive(Clone, Debug)]
struct PointerState {
    mode: Mode,
    mode_request_id: u64,
    last_generation: u64,
    active: Option<ActivePointer>,
    last_position: Option<(f32, f32)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct NativePointerEvent {
    x: f32,
    y: f32,
    flags: u32,
}

impl PointerState {
    fn new() -> Self {
        Self {
            mode: Mode::Draw,
            mode_request_id: 0,
            last_generation: 0,
            active: None,
            last_position: None,
        }
    }

    fn set_mode(
        &mut self,
        mode: Mode,
        request_id: u64,
    ) -> Result<Option<NativePointerEvent>, String> {
        if request_id <= self.mode_request_id {
            return Err("stale mode request".into());
        }
        let release = self.release_event();
        self.mode = mode;
        self.mode_request_id = request_id;
        Ok(release)
    }

    fn pointer(
        &mut self,
        action: &str,
        generation: u64,
        x: f32,
        y: f32,
        button: &str,
    ) -> Result<Option<NativePointerEvent>, String> {
        if self.mode != Mode::Pointer {
            return Err("pointer command rejected while Draw mode is active".into());
        }
        if !x.is_finite()
            || !y.is_finite()
            || !(0.0..=1.0).contains(&x)
            || !(0.0..=1.0).contains(&y)
        {
            return Err("pointer coordinates must be finite and normalized".into());
        }
        let button = match button {
            "" | "left" => Button::Left,
            "right" => Button::Right,
            _ => return Err(format!("unsupported pointer button {button:?}")),
        };
        let flags = match action {
            "hover" | "move" => {
                if self
                    .active
                    .is_some_and(|active| active.generation != generation)
                    || (self.active.is_none() && generation != self.last_generation)
                {
                    return Err("stale pointer generation".into());
                }
                MOUSEEVENTF_MOVE
            }
            "down" => {
                if self.active.is_some() || generation <= self.last_generation {
                    return Err("stale or overlapping pointer down".into());
                }
                MOUSEEVENTF_MOVE | button.down_flag()
            }
            "up" => {
                let active = self.active.ok_or("pointer up without an active down")?;
                if active.generation != generation {
                    return Err("stale pointer up".into());
                }
                MOUSEEVENTF_MOVE | active.button.up_flag()
            }
            "cancel" => {
                let Some(active) = self.active else {
                    return Ok(None);
                };
                if active.generation != generation {
                    return Err("stale pointer cancellation".into());
                }
                MOUSEEVENTF_MOVE | active.button.up_flag()
            }
            _ => return Err(format!("unsupported pointer action {action:?}")),
        };

        match action {
            "down" => {
                self.active = Some(ActivePointer { generation, button });
                self.last_generation = generation;
            }
            "up" | "cancel" => self.active = None,
            _ => {}
        }
        self.last_position = Some((x, y));
        Ok(Some(NativePointerEvent { x, y, flags }))
    }

    fn release_event(&mut self) -> Option<NativePointerEvent> {
        let active = self.active.take()?;
        let (x, y) = self.last_position?;
        Some(NativePointerEvent {
            x,
            y,
            flags: MOUSEEVENTF_MOVE | active.button.up_flag(),
        })
    }

    fn release_all(&mut self) -> Result<Option<NativePointerEvent>, String> {
        if self.active.is_some() && self.last_position.is_none() {
            return Err("cannot release remote mouse button without a valid position".into());
        }
        Ok(self.release_event())
    }
}

#[derive(Debug)]
pub struct WindowsInput {
    output: OutputBounds,
    state: PointerState,
}

impl WindowsInput {
    pub fn select_output(&mut self, output: OutputBounds) -> Result<(), String> {
        if output.width <= 0 || output.height <= 0 {
            return Err("selected output has invalid pointer geometry".into());
        }
        self.release_all()?;
        self.output = output;
        Ok(())
    }

    pub fn new(output: OutputBounds) -> Result<Self, String> {
        if output.width <= 0 || output.height <= 0 {
            return Err("selected output has invalid pointer geometry".into());
        }
        Ok(Self {
            output,
            state: PointerState::new(),
        })
    }

    pub fn set_mode(&mut self, mode: Mode, request_id: u64) -> Result<(), String> {
        let previous = self.state.clone();
        let event = self.state.set_mode(mode, request_id)?;
        if let Some(event) = event
            && let Err(error) = send_mouse(self.output, event.x, event.y, event.flags)
        {
            self.state = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn pointer(
        &mut self,
        action: &str,
        generation: u64,
        x: f32,
        y: f32,
        button: &str,
    ) -> Result<bool, String> {
        let previous = self.state.clone();
        let event = self.state.pointer(action, generation, x, y, button)?;
        if let Some(event) = event
            && let Err(error) = send_mouse(self.output, event.x, event.y, event.flags)
        {
            self.state = previous;
            return Err(error);
        }
        Ok(matches!(action, "down" | "up" | "cancel"))
    }

    pub fn release_all(&mut self) -> Result<(), String> {
        let previous = self.state.clone();
        let event = self.state.release_all()?;
        if let Some(event) = event
            && let Err(error) = send_mouse(self.output, event.x, event.y, event.flags)
        {
            self.state = previous;
            return Err(error);
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MouseInput {
    dx: i32,
    dy: i32,
    mouse_data: u32,
    flags: u32,
    time: u32,
    extra_info: usize,
}

#[repr(C)]
union InputData {
    mouse: MouseInput,
    alignment: [usize; 4],
}

#[repr(C)]
struct Input {
    kind: u32,
    data: InputData,
}

fn send_mouse(output: OutputBounds, x: f32, y: f32, flags: u32) -> Result<(), String> {
    let virtual_left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let virtual_top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let virtual_width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let virtual_height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    if virtual_width <= 1 || virtual_height <= 1 {
        return Err("Windows returned invalid virtual desktop dimensions".into());
    }
    let desktop_x = f64::from(output.left) + f64::from(x) * f64::from(output.width - 1);
    let desktop_y = f64::from(output.top) + f64::from(y) * f64::from(output.height - 1);
    let absolute_x = (((desktop_x - f64::from(virtual_left)) * 65_535.0)
        / f64::from(virtual_width - 1))
    .round()
    .clamp(0.0, 65_535.0) as i32;
    let absolute_y = (((desktop_y - f64::from(virtual_top)) * 65_535.0)
        / f64::from(virtual_height - 1))
    .round()
    .clamp(0.0, 65_535.0) as i32;
    let input = Input {
        kind: INPUT_MOUSE,
        data: InputData {
            mouse: MouseInput {
                dx: absolute_x,
                dy: absolute_y,
                mouse_data: 0,
                flags: flags | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                time: 0,
                extra_info: 0,
            },
        },
    };
    let sent = unsafe { SendInput(1, &input, size_of::<Input>() as i32) };
    if sent != 1 {
        return Err(format!(
            "SendInput did not inject the pointer event: {}. UIPI may block higher-integrity targets; DrawDock never elevates.",
            io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_mode_rejects_all_remote_pointer_commands() {
        let mut state = PointerState::new();
        assert!(state.pointer("down", 1, 0.5, 0.5, "left").is_err());
    }

    #[test]
    fn stale_coordinates_and_overlapping_down_are_rejected() {
        let mut state = PointerState::new();
        state.set_mode(Mode::Pointer, 1).unwrap();
        assert!(state.pointer("hover", 0, f32::NAN, 0.5, "").is_err());
        assert!(state.pointer("down", 1, 0.5, 0.5, "left").is_ok());
        assert!(state.pointer("down", 2, 0.6, 0.6, "right").is_err());
        assert!(state.pointer("up", 0, 0.6, 0.6, "left").is_err());
    }

    #[test]
    fn mode_change_releases_the_active_button_before_changing_mode() {
        let mut state = PointerState::new();
        state.set_mode(Mode::Pointer, 1).unwrap();
        state.pointer("down", 1, 0.5, 0.5, "right").unwrap();
        let release = state.set_mode(Mode::Draw, 2).unwrap();
        assert!(release.is_some());
        assert!(state.active.is_none());
        assert!(state.pointer("down", 2, 0.5, 0.5, "left").is_err());
    }

    #[test]
    fn release_all_produces_a_matching_button_up_without_os_injection() {
        let mut state = PointerState::new();
        state.set_mode(Mode::Pointer, 1).unwrap();
        state.pointer("down", 1, 0.5, 0.5, "right").unwrap();
        let release = state.release_all().unwrap().unwrap();
        assert_eq!(release.flags & MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_RIGHTUP);
        assert!(state.active.is_none());
    }
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetSystemMetrics(index: i32) -> i32;
    fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
}
