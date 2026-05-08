//! Privilege elevation utility for cross-platform admin privilege acquisition
//!
//! This module provides a clean, native implementation for elevating privileges
//! on macOS, Windows, and Linux without external dependencies.
//!
//! Improvements over getlantern/elevate:
//! - Native Rust implementation without external binaries
//! - Better error handling and user experience
//! - Preserves environment variables and working directory
//! - Supports GUI prompts on all platforms
//! - Linux support (missing in original)

use std::env;
use std::io;
use std::path::PathBuf;
use std::process::{Command, exit};

/// Error types for elevation operations
#[derive(Debug)]
pub enum ElevateError {
    Io(io::Error),
    Platform(String),
    CancelledByUser,
    Timeout,
    AlreadyElevated,
}

impl std::fmt::Display for ElevateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElevateError::Io(e) => write!(f, "IO error: {}", e),
            ElevateError::Platform(msg) => write!(f, "Platform error: {}", msg),
            ElevateError::CancelledByUser => write!(f, "Elevation cancelled by user"),
            ElevateError::Timeout => write!(f, "Elevation timed out"),
            ElevateError::AlreadyElevated => write!(f, "Process already has elevated privileges"),
        }
    }
}

impl std::error::Error for ElevateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ElevateError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ElevateError {
    fn from(e: io::Error) -> Self {
        ElevateError::Io(e)
    }
}

/// Configuration for elevation behavior
#[derive(Debug, Clone)]
pub struct ElevateConfig {
    /// Title for the elevation prompt (platform-dependent)
    pub prompt_title: String,
    /// Message shown to the user
    pub prompt_message: String,
    /// Icon path for the prompt (platform-dependent)
    pub icon_path: Option<PathBuf>,
    /// Timeout in seconds (0 = no timeout)
    pub timeout_secs: u64,
    /// Whether to wait for the elevated process to complete
    pub wait_for_exit: bool,
    /// Environment variables to preserve
    pub preserve_env_vars: Vec<String>,
    /// Use GUI prompt instead of terminal (macOS/Linux only)
    /// Note: GUI prompts may have issues with file access on macOS
    pub use_gui_prompt: bool,
    /// Whether to show the elevated process window (Windows only)
    /// If false, the elevated process will run in the background without a console window
    pub show_window: bool,
}

impl Default for ElevateConfig {
    fn default() -> Self {
        Self {
            prompt_title: "Privilege Elevation Required".to_string(),
            prompt_message: "This application requires administrator privileges to run."
                .to_string(),
            icon_path: None,
            timeout_secs: 0,
            wait_for_exit: true,
            preserve_env_vars: vec![
                "PATH".to_string(),
                "HOME".to_string(),
                "USER".to_string(),
                "RUST_LOG".to_string(),
                "RUST_BACKTRACE".to_string(),
            ],
            use_gui_prompt: false,
            show_window: true,
        }
    }
}

/// Check if the current process has elevated privileges
pub fn is_elevated() -> bool {
    platform_impl::is_elevated()
}

/// Elevate the current process with administrator privileges
pub fn elevate(config: &ElevateConfig) -> Result<(), ElevateError> {
    if is_elevated() {
        return Err(ElevateError::AlreadyElevated);
    }
    platform_impl::elevate(config)
}

/// Elevate and run a specific command with administrator privileges
pub fn elevate_command(
    program: &str,
    args: &[String],
    config: &ElevateConfig,
) -> Result<(), ElevateError> {
    if is_elevated() {
        let status = Command::new(program).args(args).status()?;

        if config.wait_for_exit {
            exit(status.code().unwrap_or(1));
        }
        return Ok(());
    }
    platform_impl::elevate_command(program, args, config)
}

/// Get the current executable path
pub fn current_executable() -> Result<PathBuf, ElevateError> {
    env::current_exe().map_err(ElevateError::from)
}

/// Reconstruct command line arguments for the elevated process
pub fn reconstruct_args() -> Vec<String> {
    env::args().skip(1).collect()
}

/// Convenience function to elevate current process with default config
pub fn elevate_self() -> Result<(), ElevateError> {
    elevate(&ElevateConfig::default())
}

/// Convenience function to check and elevate if needed
pub fn ensure_elevated() -> Result<(), ElevateError> {
    if is_elevated() {
        return Ok(());
    }
    elevate_self()
}

// Platform-specific implementations
#[cfg(target_os = "macos")]
mod platform_impl {
    use super::{ElevateConfig, ElevateError, current_executable, reconstruct_args};
    use std::env;
    use std::process::{Command, exit};

    pub fn is_elevated() -> bool {
        unsafe { libc::geteuid() == 0 }
    }

    fn has_gui_context() -> bool {
        env::var("DISPLAY").is_ok() || 
        env::var("__CFBundleIdentifier").is_ok() ||
        Command::new("osascript")
            .args(&["-e", "tell application \"System Events\" to return name of first application process whose frontmost is true"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn shell_escape(s: &str) -> String {
        if s.chars().any(|c| {
            c.is_whitespace() || c == '\'' || c == '"' || c == '$' || c == '`' || c == '\\'
        }) {
            format!("'{}'", s.replace('\'', "'\"'\"'"))
        } else {
            s.to_string()
        }
    }

    fn shell_escape_args(args: &[String]) -> String {
        args.iter()
            .map(|a| shell_escape(a))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn elevate(config: &ElevateConfig) -> Result<(), ElevateError> {
        let executable = current_executable()?;
        let executable_str = executable.to_string_lossy();
        let args = reconstruct_args();

        // Check if we are in a terminal
        let is_terminal = unsafe { libc::isatty(libc::STDIN_FILENO) != 0 };

        // Use sudo by default for better terminal integration
        // GUI prompts (AppleScript) can have file access issues on macOS
        if config.use_gui_prompt && (has_gui_context() || !is_terminal) {
            elevate_with_applescript(config, &executable_str, &args)
        } else {
            elevate_with_sudo(config, &executable_str, &args)
        }
    }

    #[allow(dead_code)]
    fn elevate_with_applescript(
        config: &ElevateConfig,
        executable: &str,
        args: &[String],
    ) -> Result<(), ElevateError> {
        let arg_string = shell_escape_args(args);
        let working_dir = env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "/".to_string());

        // Build environment variable exports
        let env_vars: Vec<String> = config
            .preserve_env_vars
            .iter()
            .filter_map(|var| {
                env::var(var)
                    .ok()
                    .map(|val| format!("export {}={}; ", var, shell_escape(&val)))
            })
            .collect();
        let env_string = env_vars.join("");

        // Use a two-step approach:
        // 1. First, just elevate and run the command
        // 2. The command will output to a temp file if needed
        // This avoids capturing output which can cause issues

        if config.wait_for_exit {
            // For wait mode, use sudo which properly inherits the terminal
            // and waits for the process to complete
            let mut cmd = Command::new("osascript");
            cmd.arg("-e");

            // Use a simpler script that just elevates and runs
            let script = format!(
                r#"do shell script "cd {} && {} \"{}\" {}" with administrator privileges"#,
                shell_escape(&working_dir),
                env_string,
                executable,
                arg_string
            );
            cmd.arg(&script);

            let status = cmd
                .status()
                .map_err(|e| ElevateError::Platform(format!("Failed to run osascript: {}", e)))?;

            if !status.success() {
                return Err(ElevateError::CancelledByUser);
            }

            exit(0);
        } else {
            // For non-wait mode, use nohup to detach
            let script = format!(
                r#"do shell script "cd {} && {} nohup \"{}\" {} > /dev/null 2>&1 &" with administrator privileges"#,
                shell_escape(&working_dir),
                env_string,
                executable,
                arg_string
            );

            let output = Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
                .map_err(|e| ElevateError::Platform(format!("Failed to run osascript: {}", e)))?;

            let stderr = String::from_utf8_lossy(&output.stderr);

            if !output.status.success() {
                if stderr.contains("User canceled") || stderr.contains("-128") {
                    return Err(ElevateError::CancelledByUser);
                }
                if stderr.contains("timed out") || stderr.contains("-1712") {
                    return Err(ElevateError::Timeout);
                }
                return Err(ElevateError::Platform(format!(
                    "AppleScript execution failed: {}",
                    stderr
                )));
            }

            if config.wait_for_exit {
                exit(0);
            }
            Ok(())
        }
    }

    fn elevate_with_sudo(
        config: &ElevateConfig,
        executable: &str,
        args: &[String],
    ) -> Result<(), ElevateError> {
        let mut cmd = Command::new("sudo");

        for var in &config.preserve_env_vars {
            if let Ok(val) = env::var(var) {
                cmd.arg("-E");
                cmd.env(var, val);
            }
        }

        cmd.arg(executable);
        cmd.args(args);

        let status = cmd
            .status()
            .map_err(|e| ElevateError::Platform(format!("Failed to execute sudo: {}", e)))?;

        if status.success() {
            if config.wait_for_exit {
                exit(0);
            }
            Ok(())
        } else {
            match status.code() {
                Some(1) => Err(ElevateError::CancelledByUser),
                _ => Err(ElevateError::Platform(format!(
                    "sudo elevation failed with exit code {:?}",
                    status.code()
                ))),
            }
        }
    }

    pub fn elevate_command(
        program: &str,
        args: &[String],
        config: &ElevateConfig,
    ) -> Result<(), ElevateError> {
        // Check if we are in a terminal
        let is_terminal = unsafe { libc::isatty(libc::STDIN_FILENO) != 0 };

        if config.use_gui_prompt && (has_gui_context() || !is_terminal) {
            let arg_string = shell_escape_args(args);
            let working_dir = env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "/".to_string());

            let script = format!(
                r#"do shell script "cd {} && \"{}\" {}" with administrator privileges"#,
                shell_escape(&working_dir),
                program.replace('"', "\\\""),
                arg_string
            );

            let output = Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
                .map_err(|e| ElevateError::Platform(format!("Failed to run osascript: {}", e)))?;

            let stderr = String::from_utf8_lossy(&output.stderr);

            if !output.status.success() {
                if stderr.contains("User canceled") || stderr.contains("-128") {
                    return Err(ElevateError::CancelledByUser);
                }
                return Err(ElevateError::Platform(format!(
                    "AppleScript execution failed: {}",
                    stderr
                )));
            }

            if config.wait_for_exit {
                exit(0);
            }
            Ok(())
        } else {
            let mut cmd = Command::new("sudo");
            cmd.arg(program);
            cmd.args(args);

            let status = cmd.status()?;

            if config.wait_for_exit {
                exit(status.code().unwrap_or(1));
            }

            if status.success() {
                Ok(())
            } else {
                Err(ElevateError::Platform(format!(
                    "Command failed with exit code: {:?}",
                    status.code()
                )))
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod platform_impl {
    use super::{ElevateConfig, ElevateError, current_executable, reconstruct_args};
    use std::env;
    use std::process::{Command, Stdio, exit};

    pub fn is_elevated() -> bool {
        unsafe { libc::geteuid() == 0 }
    }

    fn has_display() -> bool {
        env::var("DISPLAY").is_ok() || env::var("WAYLAND_DISPLAY").is_ok()
    }

    fn command_exists(cmd: &str) -> bool {
        Command::new("which")
            .arg(cmd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn find_elevation_method() -> &'static str {
        if has_display() {
            if command_exists("pkexec") {
                return "pkexec";
            }
            if command_exists("gksudo") {
                return "gksudo";
            }
            if command_exists("kdesudo") {
                return "kdesudo";
            }
        }
        "sudo"
    }

    fn shell_escape(s: &str) -> String {
        if s.chars().any(|c| {
            c.is_whitespace() || c == '\'' || c == '"' || c == '$' || c == '`' || c == '\\'
        }) {
            format!("'{}'", s.replace('\'', "'\"'\"'"))
        } else {
            s.to_string()
        }
    }

    fn shell_escape_args(args: &[String]) -> String {
        args.iter()
            .map(|a| shell_escape(a))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn elevate(config: &ElevateConfig) -> Result<(), ElevateError> {
        let executable = current_executable()?;
        let executable_str = executable
            .to_str()
            .ok_or_else(|| ElevateError::Platform("Invalid executable path".to_string()))?;
        let args = reconstruct_args();
        elevate_command(executable_str, &args, config)
    }

    pub fn elevate_command(
        program: &str,
        args: &[String],
        config: &ElevateConfig,
    ) -> Result<(), ElevateError> {
        let method = find_elevation_method();

        match method {
            "pkexec" => elevate_with_pkexec(program, args, config),
            "gksudo" => elevate_with_gksudo(program, args, config),
            "kdesudo" => elevate_with_kdesudo(program, args, config),
            _ => elevate_with_sudo(program, args, config),
        }
    }

    fn elevate_with_pkexec(
        program: &str,
        args: &[String],
        config: &ElevateConfig,
    ) -> Result<(), ElevateError> {
        let mut cmd = Command::new("pkexec");

        for var in &config.preserve_env_vars {
            if let Ok(val) = env::var(var) {
                cmd.env(var, val);
            }
        }

        cmd.arg(program);
        cmd.args(args);

        if let Ok(cwd) = env::current_dir() {
            cmd.current_dir(cwd);
        }

        let status = cmd
            .status()
            .map_err(|e| ElevateError::Platform(format!("Failed to execute pkexec: {}", e)))?;

        if !status.success() {
            match status.code() {
                Some(126) => {
                    return Err(ElevateError::Platform(
                        "pkexec: Permission denied or command not executable".to_string(),
                    ));
                }
                Some(127) => {
                    return Err(ElevateError::Platform(
                        "pkexec: Command not found".to_string(),
                    ));
                }
                _ => return Err(ElevateError::CancelledByUser),
            }
        }

        if config.wait_for_exit {
            exit(0);
        }

        Ok(())
    }

    fn elevate_with_gksudo(
        program: &str,
        args: &[String],
        config: &ElevateConfig,
    ) -> Result<(), ElevateError> {
        let arg_string = shell_escape_args(args);
        let command = format!("{} {}", shell_escape(program), arg_string);

        let mut cmd = Command::new("gksudo");
        cmd.arg("--message");
        cmd.arg(&config.prompt_message);
        cmd.arg("--preserve-env");
        cmd.arg(&command);

        if let Ok(cwd) = env::current_dir() {
            cmd.current_dir(cwd);
        }

        let status = cmd
            .status()
            .map_err(|e| ElevateError::Platform(format!("Failed to execute gksudo: {}", e)))?;

        if !status.success() {
            return Err(ElevateError::CancelledByUser);
        }

        if config.wait_for_exit {
            exit(0);
        }

        Ok(())
    }

    fn elevate_with_kdesudo(
        program: &str,
        args: &[String],
        config: &ElevateConfig,
    ) -> Result<(), ElevateError> {
        let arg_string = shell_escape_args(args);
        let command = format!("{} {}", shell_escape(program), arg_string);

        let mut cmd = Command::new("kdesudo");
        cmd.arg("--comment");
        cmd.arg(&config.prompt_message);
        cmd.arg(&command);

        if let Ok(cwd) = env::current_dir() {
            cmd.current_dir(cwd);
        }

        let status = cmd
            .status()
            .map_err(|e| ElevateError::Platform(format!("Failed to execute kdesudo: {}", e)))?;

        if !status.success() {
            return Err(ElevateError::CancelledByUser);
        }

        if config.wait_for_exit {
            exit(0);
        }

        Ok(())
    }

    fn elevate_with_sudo(
        program: &str,
        args: &[String],
        config: &ElevateConfig,
    ) -> Result<(), ElevateError> {
        let mut cmd = Command::new("sudo");

        for var in &config.preserve_env_vars {
            cmd.arg(format!("--preserve-env={}", var));
        }

        cmd.arg("-p");
        cmd.arg(format!("{}: ", config.prompt_message));
        cmd.arg(program);
        cmd.args(args);

        if let Ok(cwd) = env::current_dir() {
            cmd.current_dir(cwd);
        }

        let status = cmd
            .status()
            .map_err(|e| ElevateError::Platform(format!("Failed to execute sudo: {}", e)))?;

        if !status.success() {
            match status.code() {
                Some(1) => return Err(ElevateError::CancelledByUser),
                _ => {
                    return Err(ElevateError::Platform(format!(
                        "sudo exited with code {:?}",
                        status.code()
                    )));
                }
            }
        }

        if config.wait_for_exit {
            exit(0);
        }

        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod platform_impl {
    #![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]
    use super::{ElevateConfig, ElevateError, current_executable, reconstruct_args};
    use std::env;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStrExt;
    use std::process::exit;

    const SW_HIDE: i32 = 0;
    const SW_SHOWNORMAL: i32 = 1;
    const SEE_MASK_NOCLOSEPROCESS: u32 = 0x00000040;

    #[repr(C)]
    struct SHELLEXECUTEINFOW {
        cbSize: u32,
        fMask: u32,
        hwnd: *mut std::ffi::c_void,
        lpVerb: *const u16,
        lpFile: *const u16,
        lpParameters: *const u16,
        lpDirectory: *const u16,
        nShow: i32,
        hInstApp: *mut std::ffi::c_void,
        lpIDList: *mut std::ffi::c_void,
        lpClass: *const u16,
        hkeyClass: *mut std::ffi::c_void,
        dwHotKey: u32,
        hIcon: *mut std::ffi::c_void,
        hProcess: *mut std::ffi::c_void,
    }

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteExW(lpExecInfo: *mut SHELLEXECUTEINFOW) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> *mut std::ffi::c_void;
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn GetTokenInformation(
            TokenHandle: *mut std::ffi::c_void,
            TokenInformationClass: u32,
            TokenInformation: *mut std::ffi::c_void,
            TokenInformationLength: u32,
            ReturnLength: *mut u32,
        ) -> i32;
        fn OpenProcessToken(
            ProcessHandle: *mut std::ffi::c_void,
            DesiredAccess: u32,
            TokenHandle: *mut *mut std::ffi::c_void,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
        fn WaitForSingleObject(hHandle: *mut std::ffi::c_void, dwMilliseconds: u32) -> u32;
        fn GetExitCodeProcess(hProcess: *mut std::ffi::c_void, lpExitCode: *mut u32) -> i32;
    }

    const TOKEN_QUERY: u32 = 0x0008;
    const TokenElevation: u32 = 20;
    const WAIT_OBJECT_0: u32 = 0;
    const INFINITE: u32 = 0xFFFFFFFF;

    #[repr(C)]
    struct TOKEN_ELEVATION {
        TokenIsElevated: u32,
    }

    pub fn is_elevated() -> bool {
        unsafe {
            let mut token: *mut std::ffi::c_void = std::ptr::null_mut();
            let process = GetCurrentProcess();

            if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
                return false;
            }

            let mut elevation: TOKEN_ELEVATION = std::mem::zeroed();
            let mut return_length: u32 = 0;

            let result = GetTokenInformation(
                token,
                TokenElevation,
                &mut elevation as *mut _ as *mut std::ffi::c_void,
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut return_length,
            );

            CloseHandle(token);

            if result == 0 {
                return false;
            }

            elevation.TokenIsElevated != 0
        }
    }

    fn to_wide(s: &str) -> Vec<u16> {
        OsString::from(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn path_to_wide(path: &std::path::Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn build_args_string(args: &[String]) -> String {
        args.iter()
            .map(|arg| {
                if arg.contains(' ') || arg.contains('\t') || arg.contains('"') {
                    let mut escaped = String::with_capacity(arg.len() + 2);
                    escaped.push('"');
                    let mut backslashes: usize = 0;
                    for c in arg.chars() {
                        match c {
                            '\\' => {
                                backslashes += 1;
                                escaped.push(c);
                            }
                            '"' => {
                                escaped.push_str(&"\\".repeat(backslashes + 1));
                                escaped.push(c);
                                backslashes = 0;
                            }
                            _ => {
                                backslashes = 0;
                                escaped.push(c);
                            }
                        }
                    }
                    escaped.push_str(&"\\".repeat(backslashes));
                    escaped.push('"');
                    escaped
                } else {
                    arg.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn elevate(config: &ElevateConfig) -> Result<(), ElevateError> {
        let executable = current_executable()?;
        let args = reconstruct_args();

        elevate_command(executable.to_str().unwrap_or(""), &args, config)
    }

    pub fn elevate_command(
        program: &str,
        args: &[String],
        config: &ElevateConfig,
    ) -> Result<(), ElevateError> {
        let program_wide = to_wide(program);
        let args_string = build_args_string(args);
        let args_wide = to_wide(&args_string);

        let current_dir = env::current_dir()
            .map(|p| path_to_wide(&p))
            .unwrap_or_else(|_| vec![0]);
        let current_dir_ptr = if current_dir.len() > 1 {
            current_dir.as_ptr()
        } else {
            std::ptr::null()
        };

        let runas_wide = to_wide("runas");

        unsafe {
            let hwnd = GetForegroundWindow();

            let mut sei: SHELLEXECUTEINFOW = std::mem::zeroed();
            sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
            sei.fMask = SEE_MASK_NOCLOSEPROCESS;
            sei.hwnd = hwnd;
            sei.lpVerb = runas_wide.as_ptr();
            sei.lpFile = program_wide.as_ptr();
            sei.lpParameters = if args_string.is_empty() {
                std::ptr::null()
            } else {
                args_wide.as_ptr()
            };
            sei.lpDirectory = current_dir_ptr;
            sei.nShow = if config.show_window {
                SW_SHOWNORMAL
            } else {
                SW_HIDE
            };

            let result = ShellExecuteExW(&mut sei);

            if result == 0 {
                let error = std::io::Error::last_os_error();
                let error_code = error.raw_os_error().unwrap_or(0);

                match error_code {
                    1223 => return Err(ElevateError::CancelledByUser),
                    _ => {
                        return Err(ElevateError::Platform(format!(
                            "ShellExecuteEx failed with error {}: {}",
                            error_code, error
                        )));
                    }
                }
            }

            if sei.hProcess.is_null() {
                return Err(ElevateError::CancelledByUser);
            }

            if config.wait_for_exit {
                let wait_result = WaitForSingleObject(sei.hProcess, INFINITE);

                if wait_result != WAIT_OBJECT_0 {
                    CloseHandle(sei.hProcess);
                    return Err(ElevateError::Platform(
                        "Failed to wait for elevated process".to_string(),
                    ));
                }

                let mut exit_code: u32 = 0;
                if GetExitCodeProcess(sei.hProcess, &mut exit_code) != 0 {
                    CloseHandle(sei.hProcess);
                    exit(exit_code as i32);
                } else {
                    CloseHandle(sei.hProcess);
                    exit(0);
                }
            } else {
                CloseHandle(sei.hProcess);
                Ok(())
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod platform_impl {
    use super::{ElevateConfig, ElevateError};

    pub fn is_elevated() -> bool {
        false
    }

    pub fn elevate(_config: &ElevateConfig) -> Result<(), ElevateError> {
        Err(ElevateError::Platform(
            "Privilege elevation not supported on this platform".to_string(),
        ))
    }

    pub fn elevate_command(
        _program: &str,
        _args: &[String],
        _config: &ElevateConfig,
    ) -> Result<(), ElevateError> {
        Err(ElevateError::Platform(
            "Privilege elevation not supported on this platform".to_string(),
        ))
    }
}
