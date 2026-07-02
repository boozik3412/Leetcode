use crate::agent::types::{AppEvent, ToolResult};
use crate::tools::policy::{request_approval_if, ApprovalMap, PolicyConfig};
use crate::workspace::Workspace;
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::json;
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

#[derive(Debug, Deserialize)]
pub struct FocusWindowArgs {
    pub title: Option<String>,
    pub process: Option<String>,
    pub exact: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct DesktopStepArgs {
    pub action: Option<String>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub button: Option<String>,
    pub clicks: Option<u8>,
    pub text: Option<String>,
    pub keys: Option<Vec<String>>,
    pub title: Option<String>,
    pub process: Option<String>,
    pub exact: Option<bool>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowInfo {
    pub title: String,
    pub process_name: String,
    pub process_id: u32,
    pub hwnd: isize,
    pub rect: Option<WindowRect>,
}

pub fn screenshot(
    workspace: &Workspace,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if !request_approval_if(
        policy.require_desktop_approval,
        events,
        approvals,
        "Capture desktop screenshot",
        "The agent wants to capture the current desktop and save it into the selected workspace.",
    ) {
        return ToolResult::error("screenshot denied by user");
    }

    match capture_screenshot_file(workspace, "screenshot") {
        Ok(path) => ToolResult::ok(path),
        Err(err) => ToolResult::error(err),
    }
}

pub fn active_window() -> ToolResult {
    match active_window_info() {
        Ok(info) => ToolResult::ok(
            serde_json::to_string_pretty(&info).unwrap_or_else(|_| "active window".to_string()),
        ),
        Err(err) => ToolResult::error(err),
    }
}

pub fn focus_window(
    args: FocusWindowArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if args.title.as_deref().unwrap_or("").trim().is_empty()
        && args.process.as_deref().unwrap_or("").trim().is_empty()
    {
        return ToolResult::error("focus_window requires title or process");
    }

    if !request_approval_if(
        policy.require_desktop_approval,
        events,
        approvals,
        "Focus desktop window",
        format!(
            "Title: {}\nProcess: {}\nExact: {}",
            args.title.as_deref().unwrap_or(""),
            args.process.as_deref().unwrap_or(""),
            args.exact.unwrap_or(false)
        ),
    ) {
        return ToolResult::error("focus_window denied by user");
    }

    match focus_window_impl(&args) {
        Ok(info) => ToolResult::ok(
            serde_json::to_string_pretty(&info).unwrap_or_else(|_| "window focused".to_string()),
        ),
        Err(err) => ToolResult::error(err),
    }
}

pub fn desktop_step(
    workspace: &Workspace,
    args: DesktopStepArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    let action = args
        .action
        .as_deref()
        .unwrap_or("observe")
        .trim()
        .to_ascii_lowercase();
    if !matches!(
        action.as_str(),
        "observe" | "click" | "type_text" | "hotkey" | "focus_window"
    ) {
        return ToolResult::error(
            "desktop_step action must be observe, click, type_text, hotkey, or focus_window",
        );
    }

    if let Err(err) = validate_desktop_step(&action, &args) {
        return ToolResult::error(err);
    }

    if !request_approval_if(
        policy.require_desktop_approval,
        events,
        approvals,
        format!("Desktop step: {action}"),
        desktop_step_approval_detail(&action, &args),
    ) {
        return ToolResult::error("desktop_step denied by user");
    }

    let before_screenshot = match capture_screenshot_file(workspace, "before") {
        Ok(path) => path,
        Err(err) => return ToolResult::error(err),
    };
    let before_window = active_window_info().ok();
    let action_result = match action.as_str() {
        "observe" => Ok("observed desktop".to_string()),
        "click" => mouse_click_impl(&MouseClickArgs {
            x: args.x.unwrap_or_default(),
            y: args.y.unwrap_or_default(),
            button: args.button.clone(),
            clicks: args.clicks,
        }),
        "type_text" => type_text_impl(&TypeTextArgs {
            text: args.text.clone().unwrap_or_default(),
        }),
        "hotkey" => hotkey_impl(&HotkeyArgs {
            keys: args.keys.clone().unwrap_or_default(),
        }),
        "focus_window" => focus_window_impl(&FocusWindowArgs {
            title: args.title.clone(),
            process: args.process.clone(),
            exact: args.exact,
        })
        .map(|window| format!("focused {} ({})", window.title, window.process_name)),
        _ => unreachable!(),
    };

    let action_result = match action_result {
        Ok(result) => result,
        Err(err) => return ToolResult::error(err),
    };
    std::thread::sleep(std::time::Duration::from_millis(250));
    let after_screenshot = match capture_screenshot_file(workspace, "after") {
        Ok(path) => path,
        Err(err) => return ToolResult::error(err),
    };
    let after_window = active_window_info().ok();

    ToolResult::ok(
        serde_json::to_string_pretty(&json!({
            "action": action,
            "result": action_result,
            "before_screenshot": before_screenshot,
            "after_screenshot": after_screenshot,
            "before_window": before_window,
            "after_window": after_window
        }))
        .unwrap_or_else(|_| "desktop step completed".to_string()),
    )
}

pub fn mouse_click(
    args: MouseClickArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    let button = args.button.clone().unwrap_or_else(|| "left".to_string());
    if mouse_button_flags(&button).is_none() {
        return ToolResult::error("mouse_click button must be left, right, or middle");
    }
    let clicks = args.clicks.unwrap_or(1).clamp(1, 3);

    if !request_approval_if(
        policy.require_desktop_approval,
        events,
        approvals,
        format!("Mouse {button} click at {}, {}", args.x, args.y),
        format!("Clicks: {clicks}\nCoordinates are absolute desktop pixels."),
    ) {
        return ToolResult::error("mouse_click denied by user");
    }

    mouse_click_impl(&args)
        .map(ToolResult::ok)
        .unwrap_or_else(ToolResult::error)
}

pub fn type_text(
    args: TypeTextArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if args.text.is_empty() {
        return ToolResult::error("type_text text is empty");
    }

    let preview = preview_text(&args.text, 1_000);
    if !request_approval_if(
        policy.require_desktop_approval,
        events,
        approvals,
        format!("Type {} characters", args.text.chars().count()),
        preview,
    ) {
        return ToolResult::error("type_text denied by user");
    }

    type_text_impl(&args)
        .map(ToolResult::ok)
        .unwrap_or_else(ToolResult::error)
}

pub fn hotkey(
    args: HotkeyArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    policy: &PolicyConfig,
) -> ToolResult {
    if args.keys.is_empty() {
        return ToolResult::error("hotkey keys is empty");
    }
    if let Err(err) = validate_hotkey_keys(&args.keys) {
        return ToolResult::error(err);
    }

    let rendered_keys = args.keys.join("+");
    if !request_approval_if(
        policy.require_desktop_approval,
        events,
        approvals,
        format!("Press hotkey {rendered_keys}"),
        "The agent wants to send a keyboard shortcut to the active desktop window.",
    ) {
        return ToolResult::error("hotkey denied by user");
    }

    hotkey_impl(&args)
        .map(ToolResult::ok)
        .unwrap_or_else(ToolResult::error)
}

fn capture_screenshot_file(workspace: &Workspace, prefix: &str) -> Result<String, String> {
    let rel_path = format!(
        "assets/generated/screenshots/{prefix}-{}.png",
        uuid::Uuid::new_v4()
    );
    let output_path = workspace
        .resolve_for_write(&rel_path)
        .map_err(|err| err.to_string())?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
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
        run_powershell_script(&script, "screenshot")?;
        Ok(rel_path)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = workspace;
        let _ = prefix;
        Err("screenshot is currently implemented only on Windows".to_string())
    }
}

fn mouse_click_impl(args: &MouseClickArgs) -> Result<String, String> {
    let button = args.button.clone().unwrap_or_else(|| "left".to_string());
    let Some((down_flag, up_flag)) = mouse_button_flags(&button) else {
        return Err("mouse_click button must be left, right, or middle".to_string());
    };
    let clicks = args.clicks.unwrap_or(1).clamp(1, 3);

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
            .map(|_| format!("clicked {button} at {}, {}", args.x, args.y))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = down_flag;
        let _ = up_flag;
        Err("mouse_click is currently implemented only on Windows".to_string())
    }
}

fn type_text_impl(args: &TypeTextArgs) -> Result<String, String> {
    if args.text.is_empty() {
        return Err("type_text text is empty".to_string());
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
            .map(|_| format!("typed {} characters", args.text.chars().count()))
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("type_text is currently implemented only on Windows".to_string())
    }
}

fn hotkey_impl(args: &HotkeyArgs) -> Result<String, String> {
    if args.keys.is_empty() {
        return Err("hotkey keys is empty".to_string());
    }
    let codes = args
        .keys
        .iter()
        .map(|key| virtual_key_code(key).ok_or_else(|| format!("Unsupported hotkey key: {key}")))
        .collect::<Result<Vec<_>, _>>()?;
    let rendered_keys = args.keys.join("+");

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

        run_powershell_script(&script, "hotkey").map(|_| format!("pressed {rendered_keys}"))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = codes;
        Err("hotkey is currently implemented only on Windows".to_string())
    }
}

fn active_window_info() -> Result<WindowInfo, String> {
    #[cfg(target_os = "windows")]
    {
        let info_script = window_info_powershell("hwnd");
        let script = native_window_script(&format!(
            r#"
$hwnd = [NativeWindow]::GetForegroundWindow()
{info_script}
"#
        ));
        let output = run_powershell_script_output(&script, "active_window")?;
        serde_json::from_str::<WindowInfo>(output.trim()).map_err(|err| err.to_string())
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("active_window is currently implemented only on Windows".to_string())
    }
}

fn focus_window_impl(args: &FocusWindowArgs) -> Result<WindowInfo, String> {
    #[cfg(target_os = "windows")]
    {
        let title = encoded_optional(&args.title);
        let process = encoded_optional(&args.process);
        let exact = if args.exact.unwrap_or(false) {
            "$true"
        } else {
            "$false"
        };
        let info_script = window_info_powershell("hwnd");
        let script = native_window_script(&format!(
            r#"
function Decode-Optional([string]$value) {{
    if ([string]::IsNullOrWhiteSpace($value)) {{ return "" }}
    return [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($value))
}}

$targetTitle = Decode-Optional "{title}"
$targetProcess = Decode-Optional "{process}"
$exact = {exact}
$windows = New-Object System.Collections.Generic.List[object]
$callback = [NativeWindow+EnumWindowsProc] {{
    param([IntPtr]$hwnd, [IntPtr]$lparam)
    if (-not [NativeWindow]::IsWindowVisible($hwnd)) {{ return $true }}
    $length = [NativeWindow]::GetWindowTextLength($hwnd)
    if ($length -le 0) {{ return $true }}
    $builder = New-Object System.Text.StringBuilder ($length + 1)
    [NativeWindow]::GetWindowText($hwnd, $builder, $builder.Capacity) | Out-Null
    $titleValue = $builder.ToString()
    $pid = 0
    [NativeWindow]::GetWindowThreadProcessId($hwnd, [ref]$pid) | Out-Null
    $processName = ""
    try {{ $processName = (Get-Process -Id $pid -ErrorAction Stop).ProcessName }} catch {{ }}
    $windows.Add([pscustomobject]@{{
        Hwnd = $hwnd
        Title = $titleValue
        ProcessName = $processName
        ProcessId = $pid
    }}) | Out-Null
    return $true
}}
[NativeWindow]::EnumWindows($callback, [IntPtr]::Zero) | Out-Null

$matches = $windows | Where-Object {{
    $titleOk = $true
    $processOk = $true
    if (-not [string]::IsNullOrWhiteSpace($targetTitle)) {{
        if ($exact) {{
            $titleOk = $_.Title -ieq $targetTitle
        }} else {{
            $titleOk = $_.Title.ToLowerInvariant().Contains($targetTitle.ToLowerInvariant())
        }}
    }}
    if (-not [string]::IsNullOrWhiteSpace($targetProcess)) {{
        if ($exact) {{
            $processOk = $_.ProcessName -ieq $targetProcess
        }} else {{
            $processOk = $_.ProcessName.ToLowerInvariant().Contains($targetProcess.ToLowerInvariant())
        }}
    }}
    $titleOk -and $processOk
}}
$match = $matches | Select-Object -First 1
if ($null -eq $match) {{ throw "No matching visible window found" }}
[NativeWindow]::ShowWindow($match.Hwnd, 9) | Out-Null
Start-Sleep -Milliseconds 60
[NativeWindow]::SetForegroundWindow($match.Hwnd) | Out-Null
$hwnd = $match.Hwnd
{info_script}
"#
        ));
        let output = run_powershell_script_output(&script, "focus_window")?;
        serde_json::from_str::<WindowInfo>(output.trim()).map_err(|err| err.to_string())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = args;
        Err("focus_window is currently implemented only on Windows".to_string())
    }
}

fn validate_desktop_step(action: &str, args: &DesktopStepArgs) -> Result<(), String> {
    match action {
        "observe" => Ok(()),
        "click" => {
            if args.x.is_none() || args.y.is_none() {
                return Err("desktop_step click requires x and y".to_string());
            }
            if let Some(button) = &args.button {
                if mouse_button_flags(button).is_none() {
                    return Err(
                        "desktop_step click button must be left, right, or middle".to_string()
                    );
                }
            }
            Ok(())
        }
        "type_text" => {
            if args.text.as_deref().unwrap_or("").is_empty() {
                return Err("desktop_step type_text requires text".to_string());
            }
            Ok(())
        }
        "hotkey" => validate_hotkey_keys(args.keys.as_deref().unwrap_or(&[])),
        "focus_window" => {
            if args.title.as_deref().unwrap_or("").trim().is_empty()
                && args.process.as_deref().unwrap_or("").trim().is_empty()
            {
                return Err("desktop_step focus_window requires title or process".to_string());
            }
            Ok(())
        }
        _ => Err("unsupported desktop_step action".to_string()),
    }
}

fn desktop_step_approval_detail(action: &str, args: &DesktopStepArgs) -> String {
    format!(
        "Action: {action}\nCoordinates: {}, {}\nButton: {}\nText: {}\nKeys: {}\nTitle: {}\nProcess: {}\nNote: {}\n\nThe app will capture a screenshot before and after the action.",
        args.x.map(|value| value.to_string()).unwrap_or_default(),
        args.y.map(|value| value.to_string()).unwrap_or_default(),
        args.button.as_deref().unwrap_or(""),
        args.text
            .as_deref()
            .map(|text| preview_text(text, 400))
            .unwrap_or_default(),
        args.keys
            .as_ref()
            .map(|keys| keys.join("+"))
            .unwrap_or_default(),
        args.title.as_deref().unwrap_or(""),
        args.process.as_deref().unwrap_or(""),
        args.note.as_deref().unwrap_or("")
    )
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
    run_powershell_script_output(script, action).map(|_| ())
}

#[cfg(target_os = "windows")]
fn run_powershell_script_output(script: &str, action: &str) -> Result<String, String> {
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
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!(
            "{action} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[cfg(target_os = "windows")]
fn native_window_script(body: &str) -> String {
    format!(
        r#"
$signature = @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class NativeWindow {{
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {{
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }}

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);

    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern int GetWindowTextLength(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out int processId);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);
}}
"@
Add-Type -TypeDefinition $signature
{body}
"#
    )
}

#[cfg(target_os = "windows")]
fn window_info_powershell(hwnd_var: &str) -> String {
    format!(
        r#"
$titleLength = [NativeWindow]::GetWindowTextLength(${hwnd_var})
$titleBuilder = New-Object System.Text.StringBuilder ($titleLength + 1)
[NativeWindow]::GetWindowText(${hwnd_var}, $titleBuilder, $titleBuilder.Capacity) | Out-Null
$pid = 0
[NativeWindow]::GetWindowThreadProcessId(${hwnd_var}, [ref]$pid) | Out-Null
$processName = ""
try {{ $processName = (Get-Process -Id $pid -ErrorAction Stop).ProcessName }} catch {{ }}
$rect = New-Object NativeWindow+RECT
$hasRect = [NativeWindow]::GetWindowRect(${hwnd_var}, [ref]$rect)
$rectObject = $null
if ($hasRect) {{
    $rectObject = [pscustomobject]@{{
        left = $rect.Left
        top = $rect.Top
        right = $rect.Right
        bottom = $rect.Bottom
    }}
}}
[pscustomobject]@{{
    title = $titleBuilder.ToString()
    process_name = $processName
    process_id = $pid
    hwnd = ${hwnd_var}.ToInt64()
    rect = $rectObject
}} | ConvertTo-Json -Compress -Depth 4
"#
    )
}

fn validate_hotkey_keys(keys: &[String]) -> Result<(), String> {
    if keys.is_empty() {
        return Err("hotkey keys is empty".to_string());
    }
    for key in keys {
        if virtual_key_code(key).is_none() {
            return Err(format!("Unsupported hotkey key: {key}"));
        }
    }
    Ok(())
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

fn encoded_optional(value: &Option<String>) -> String {
    value
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| general_purpose::STANDARD.encode(value.as_bytes()))
        .unwrap_or_default()
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

    #[test]
    fn validates_desktop_step_args() {
        assert!(validate_desktop_step(
            "observe",
            &DesktopStepArgs {
                action: None,
                x: None,
                y: None,
                button: None,
                clicks: None,
                text: None,
                keys: None,
                title: None,
                process: None,
                exact: None,
                note: None,
            }
        )
        .is_ok());
        assert!(validate_desktop_step(
            "click",
            &DesktopStepArgs {
                action: None,
                x: Some(10),
                y: None,
                button: None,
                clicks: None,
                text: None,
                keys: None,
                title: None,
                process: None,
                exact: None,
                note: None,
            }
        )
        .is_err());
    }
}
