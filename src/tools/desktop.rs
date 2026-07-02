use crate::agent::types::{AppEvent, ToolResult};
use crate::tools::policy::{request_approval, ApprovalMap};
use crate::workspace::Workspace;
use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
use std::fs;
use std::process::Command;
use std::sync::mpsc::Sender;

#[derive(Debug, Deserialize)]
pub struct MouseClickArgs {
    pub x: i32,
    pub y: i32,
    pub button: Option<String>,
    pub clicks: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct TypeTextArgs {
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct HotkeyArgs {
    pub keys: Vec<String>,
}

pub fn screenshot(
    workspace: &Workspace,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    if !request_approval(
        events,
        approvals,
        "Capture desktop screenshot",
        "The agent wants to capture the current desktop and save it into the selected workspace.",
    ) {
        return ToolResult::error("screenshot denied by user");
    }

    let rel_path = format!(
        "assets/generated/screenshots/screenshot-{}.png",
        uuid::Uuid::new_v4()
    );
    let output_path = match workspace.resolve_for_write(&rel_path) {
        Ok(path) => path,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    if let Some(parent) = output_path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            return ToolResult::error(err.to_string());
        }
    }

    #[cfg(target_os = "windows")]
    {
        let escaped_path = output_path.to_string_lossy().replace('\'', "''");
        let script = format!(
            r#"$path = '{escaped_path}'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
$bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bounds.Size)
$bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
"#
        );
        let status = Command::new("powershell.exe")
            .arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-Command")
            .arg(script)
            .status();

        match status {
            Ok(status) if status.success() => ToolResult::ok(rel_path),
            Ok(status) => ToolResult::error(format!("screenshot failed with status {status}")),
            Err(err) => ToolResult::error(err.to_string()),
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        ToolResult::error("screenshot is currently implemented only on Windows")
    }
}

pub fn mouse_click(
    args: MouseClickArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let button = args.button.unwrap_or_else(|| "left".to_string());
    let Some((down_flag, up_flag)) = mouse_button_flags(&button) else {
        return ToolResult::error("mouse_click button must be left, right, or middle");
    };
    let clicks = args.clicks.unwrap_or(1).clamp(1, 3);

    if !request_approval(
        events,
        approvals,
        format!("Mouse {button} click at {}, {}", args.x, args.y),
        format!("Clicks: {clicks}\nCoordinates are absolute desktop pixels."),
    ) {
        return ToolResult::error("mouse_click denied by user");
    }

    #[cfg(target_os = "windows")]
    {
        let script = format!(
            r#"
$signature = @"
using System;
using System.Runtime.InteropServices;
public static class NativeMouse {{
    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int X, int Y);

    [DllImport("user32.dll")]
    public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
}}
"@
Add-Type -TypeDefinition $signature
[NativeMouse]::SetCursorPos({x}, {y}) | Out-Null
Start-Sleep -Milliseconds 60
for ($i = 0; $i -lt {clicks}; $i++) {{
    [NativeMouse]::mouse_event({down_flag}, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 35
    [NativeMouse]::mouse_event({up_flag}, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 90
}}
"#,
            x = args.x,
            y = args.y,
        );

        run_powershell_script(&script, "mouse_click")
            .map(|_| ToolResult::ok(format!("clicked {button} at {}, {}", args.x, args.y)))
            .unwrap_or_else(ToolResult::error)
    }

    #[cfg(not(target_os = "windows"))]
    {
        ToolResult::error("mouse_click is currently implemented only on Windows")
    }
}

pub fn type_text(
    args: TypeTextArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    if args.text.is_empty() {
        return ToolResult::error("type_text text is empty");
    }

    let preview = preview_text(&args.text, 1_000);
    if !request_approval(
        events,
        approvals,
        format!("Type {} characters", args.text.chars().count()),
        preview,
    ) {
        return ToolResult::error("type_text denied by user");
    }

    #[cfg(target_os = "windows")]
    {
        let encoded = general_purpose::STANDARD.encode(args.text.as_bytes());
        let script = format!(
            r#"
$text = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}'))
$signature = @"
using System;
using System.Runtime.InteropServices;
using System.Threading;

public static class NativeKeyboard {{
    [StructLayout(LayoutKind.Sequential)]
    public struct INPUT {{
        public uint type;
        public InputUnion U;
    }}

    [StructLayout(LayoutKind.Explicit)]
    public struct InputUnion {{
        [FieldOffset(0)]
        public KEYBDINPUT ki;
    }}

    [StructLayout(LayoutKind.Sequential)]
    public struct KEYBDINPUT {{
        public ushort wVk;
        public ushort wScan;
        public uint dwFlags;
        public uint time;
        public UIntPtr dwExtraInfo;
    }}

    [DllImport("user32.dll", SetLastError=true)]
    public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

    public const uint INPUT_KEYBOARD = 1;
    public const uint KEYEVENTF_KEYUP = 0x0002;
    public const uint KEYEVENTF_UNICODE = 0x0004;

    public static void TypeText(string text) {{
        foreach (char ch in text) {{
            INPUT[] inputs = new INPUT[2];
            inputs[0].type = INPUT_KEYBOARD;
            inputs[0].U.ki.wVk = 0;
            inputs[0].U.ki.wScan = ch;
            inputs[0].U.ki.dwFlags = KEYEVENTF_UNICODE;
            inputs[1].type = INPUT_KEYBOARD;
            inputs[1].U.ki.wVk = 0;
            inputs[1].U.ki.wScan = ch;
            inputs[1].U.ki.dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;
            SendInput(2, inputs, Marshal.SizeOf(typeof(INPUT)));
            Thread.Sleep(2);
        }}
    }}
}}
"@
Add-Type -TypeDefinition $signature
[NativeKeyboard]::TypeText($text)
"#
        );

        run_powershell_script(&script, "type_text")
            .map(|_| ToolResult::ok(format!("typed {} characters", args.text.chars().count())))
            .unwrap_or_else(ToolResult::error)
    }

    #[cfg(not(target_os = "windows"))]
    {
        ToolResult::error("type_text is currently implemented only on Windows")
    }
}

pub fn hotkey(args: HotkeyArgs, events: &Sender<AppEvent>, approvals: &ApprovalMap) -> ToolResult {
    if args.keys.is_empty() {
        return ToolResult::error("hotkey keys is empty");
    }
    let mut codes = Vec::new();
    for key in &args.keys {
        let Some(code) = virtual_key_code(key) else {
            return ToolResult::error(format!("Unsupported hotkey key: {key}"));
        };
        codes.push(code);
    }

    let rendered_keys = args.keys.join("+");
    if !request_approval(
        events,
        approvals,
        format!("Press hotkey {rendered_keys}"),
        "The agent wants to send a keyboard shortcut to the active desktop window.",
    ) {
        return ToolResult::error("hotkey denied by user");
    }

    #[cfg(target_os = "windows")]
    {
        let codes = codes
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let script = format!(
            r#"
$signature = @"
using System;
using System.Runtime.InteropServices;
using System.Threading;

public static class NativeHotkey {{
    [StructLayout(LayoutKind.Sequential)]
    public struct INPUT {{
        public uint type;
        public InputUnion U;
    }}

    [StructLayout(LayoutKind.Explicit)]
    public struct InputUnion {{
        [FieldOffset(0)]
        public KEYBDINPUT ki;
    }}

    [StructLayout(LayoutKind.Sequential)]
    public struct KEYBDINPUT {{
        public ushort wVk;
        public ushort wScan;
        public uint dwFlags;
        public uint time;
        public UIntPtr dwExtraInfo;
    }}

    [DllImport("user32.dll", SetLastError=true)]
    public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

    public const uint INPUT_KEYBOARD = 1;
    public const uint KEYEVENTF_KEYUP = 0x0002;

    public static void Key(ushort vk, bool up) {{
        INPUT[] inputs = new INPUT[1];
        inputs[0].type = INPUT_KEYBOARD;
        inputs[0].U.ki.wVk = vk;
        inputs[0].U.ki.wScan = 0;
        inputs[0].U.ki.dwFlags = up ? KEYEVENTF_KEYUP : 0;
        SendInput(1, inputs, Marshal.SizeOf(typeof(INPUT)));
        Thread.Sleep(25);
    }}

    public static void Hotkey(ushort[] keys) {{
        foreach (ushort key in keys) {{
            Key(key, false);
        }}
        for (int i = keys.Length - 1; i >= 0; i--) {{
            Key(keys[i], true);
        }}
    }}
}}
"@
Add-Type -TypeDefinition $signature
[ushort[]]$keys = @({codes})
[NativeHotkey]::Hotkey($keys)
"#
        );

        run_powershell_script(&script, "hotkey")
            .map(|_| ToolResult::ok(format!("pressed {rendered_keys}")))
            .unwrap_or_else(ToolResult::error)
    }

    #[cfg(not(target_os = "windows"))]
    {
        ToolResult::error("hotkey is currently implemented only on Windows")
    }
}

fn mouse_button_flags(button: &str) -> Option<(u32, u32)> {
    match button.trim().to_ascii_lowercase().as_str() {
        "left" => Some((0x0002, 0x0004)),
        "right" => Some((0x0008, 0x0010)),
        "middle" => Some((0x0020, 0x0040)),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn run_powershell_script(script: &str, action: &str) -> Result<(), String> {
    let output = Command::new("powershell.exe")
        .arg("-NoLogo")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(script)
        .output()
        .map_err(|err| err.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{action} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn virtual_key_code(key: &str) -> Option<u16> {
    let normalized = key.trim().to_ascii_lowercase();
    if normalized.len() == 1 {
        let ch = normalized.chars().next()?;
        if ch.is_ascii_alphabetic() || ch.is_ascii_digit() {
            return Some(ch.to_ascii_uppercase() as u16);
        }
    }

    match normalized.as_str() {
        "ctrl" | "control" => Some(0x11),
        "shift" => Some(0x10),
        "alt" => Some(0x12),
        "win" | "windows" | "meta" => Some(0x5B),
        "enter" | "return" => Some(0x0D),
        "esc" | "escape" => Some(0x1B),
        "tab" => Some(0x09),
        "space" => Some(0x20),
        "backspace" => Some(0x08),
        "delete" | "del" => Some(0x2E),
        "insert" | "ins" => Some(0x2D),
        "home" => Some(0x24),
        "end" => Some(0x23),
        "pageup" | "page_up" | "pgup" => Some(0x21),
        "pagedown" | "page_down" | "pgdn" => Some(0x22),
        "up" | "arrowup" | "arrow_up" => Some(0x26),
        "down" | "arrowdown" | "arrow_down" => Some(0x28),
        "left" | "arrowleft" | "arrow_left" => Some(0x25),
        "right" | "arrowright" | "arrow_right" => Some(0x27),
        "capslock" => Some(0x14),
        "numlock" => Some(0x90),
        "scrolllock" => Some(0x91),
        "printscreen" | "prtsc" => Some(0x2C),
        "pause" => Some(0x13),
        _ if normalized.starts_with('f') => normalized[1..]
            .parse::<u16>()
            .ok()
            .filter(|number| (1..=24).contains(number))
            .map(|number| 0x70 + number - 1),
        _ => None,
    }
}

fn preview_text(text: &str, max_chars: usize) -> String {
    let mut preview = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        preview.push_str("\n... truncated ...");
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_hotkey_names() {
        assert_eq!(virtual_key_code("ctrl"), Some(0x11));
        assert_eq!(virtual_key_code("L"), Some(0x4C));
        assert_eq!(virtual_key_code("f5"), Some(0x74));
        assert_eq!(virtual_key_code("arrow_left"), Some(0x25));
        assert_eq!(virtual_key_code("arrowleft"), Some(0x25));
    }

    #[test]
    fn maps_mouse_buttons() {
        assert_eq!(mouse_button_flags("left"), Some((0x0002, 0x0004)));
        assert_eq!(mouse_button_flags("middle"), Some((0x0020, 0x0040)));
        assert_eq!(mouse_button_flags("nope"), None);
    }
}
