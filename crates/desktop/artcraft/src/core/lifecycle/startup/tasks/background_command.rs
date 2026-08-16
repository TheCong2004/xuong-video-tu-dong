use std::process::Command;

/// Configures a child process for background execution by the desktop app.
///
/// Redirecting stdio does not suppress the console window that Windows creates
/// for console-subsystem executables. `CREATE_NO_WINDOW` does, while the no-op
/// implementation on other platforms keeps all backend launch code portable.
pub fn background_command(mut command: Command) -> Command {
  #[cfg(target_os = "windows")]
  {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
  }

  command
}
