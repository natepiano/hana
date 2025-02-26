use crate::error::{Error, Result};
use error_stack::{Report, ResultExt};
use tracing::trace;

pub fn activate_parent_window() -> Result<()> {
    let ui_pid = get_probably_editor_parent()?;
    trace!("Found UI parent process PID: {}", ui_pid);

    let app_path = get_app_path_from_ps(ui_pid)?;

    std::process::Command::new("open")
        .arg(app_path.clone())
        .spawn()
        .change_context(Error::WindowActivation)
        .attach_printable_lazy(|| {
            format!("Failed to execute 'open' command for cmd: {}", app_path)
        })?;

    Ok(())
}

fn get_probably_editor_parent() -> Result<i32> {
    let shell_pid = unsafe { libc::getppid() };
    trace!("Our parent shell PID: {}", shell_pid);

    // First get the login process info
    let output = std::process::Command::new("ps")
        .args(["-fp", &shell_pid.to_string()])
        .output()
        .change_context(Error::ParentCapture)
        .attach_printable("Failed to execute ps command for shell")?;

    let output_str = String::from_utf8(output.stdout)
        .change_context(Error::ParentCapture)
        .attach_printable("Failed to parse shell ps output as UTF-8")?;

    trace!("ps output for shell:\n{}", output_str);

    let lines: Vec<&str> = output_str.lines().collect();
    if lines.len() < 2 {
        return Err(
            Report::new(Error::ParentCapture).attach_printable("No process info found for shell")
        );
    }

    // Get the login process PID
    let login_pid = parse_parent_pid(&lines[1])?;
    trace!("Login process PID: {}", login_pid);

    // Now get the UI parent process info
    let output = std::process::Command::new("ps")
        .args(["-fp", &login_pid.to_string()])
        .output()
        .change_context(Error::ParentCapture)
        .attach_printable("Failed to execute ps command for login process")?;

    let output_str = String::from_utf8(output.stdout)
        .change_context(Error::ParentCapture)
        .attach_printable("Failed to parse login ps output as UTF-8")?;

    trace!("ps output for login process:\n{}", output_str);

    let lines: Vec<&str> = output_str.lines().collect();
    if lines.len() < 2 {
        return Err(Report::new(Error::ParentCapture)
            .attach_printable("No process info found for login process"));
    }

    let ui_pid = parse_parent_pid(&lines[1])?;
    trace!("Found UI parent process PID: {}", ui_pid);
    Ok(ui_pid)
}

fn parse_parent_pid(ps_line: &str) -> Result<i32> {
    let fields: Vec<&str> = ps_line.split_whitespace().collect();

    if fields.len() < 3 {
        return Err(Report::new(Error::ParentCapture).attach_printable("Invalid ps output format"));
    }

    fields[2]
        .parse::<i32>()
        .change_context(Error::ParentCapture)
        .attach_printable("Failed to parse PID from ps output")
}

fn get_app_path_from_ps(pid: i32) -> Result<String> {
    let output = std::process::Command::new("ps")
        .args(["-fp", &pid.to_string()])
        .output()
        .change_context(Error::WindowActivation)
        .attach_printable("Failed to execute ps command")?;

    let output_str = String::from_utf8(output.stdout)
        .change_context(Error::WindowActivation)
        .attach_printable("Failed to parse ps output as UTF-8")?;

    let cmd = output_str
        .lines()
        .nth(1)
        .and_then(|line| line.split_whitespace().last())
        .and_then(|path| path.split("/Contents/MacOS/").next())
        .ok_or_else(|| {
            Report::new(Error::WindowActivation)
                .attach_printable("Failed to extract application path from ps output")
        })?;

    Ok(cmd.to_string())
}
