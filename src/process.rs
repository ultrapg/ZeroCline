use std::path::{Path, PathBuf};
use std::process::{Command, Child};
use anyhow::{Result, anyhow, Context};
use std::os::windows::process::CommandExt;
use std::os::windows::io::RawHandle;

const CREATE_NO_WINDOW: u32 = 0x08000000;
const CREATE_NEW_CONSOLE: u32 = 0x00000010;

#[repr(C)]
struct IO_COUNTERS {
    read_operation_count: u64,
    write_operation_count: u64,
    other_operation_count: u64,
    read_transfer_count: u64,
    write_transfer_count: u64,
    other_transfer_count: u64,
}

#[repr(C)]
struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[repr(C)]
struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
    basic_limit_information: JOBOBJECT_BASIC_LIMIT_INFORMATION,
    io_info: IO_COUNTERS,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_limit: usize,
    peak_job_memory_limit: usize,
}

unsafe extern "system" {
    fn CreateJobObjectW(
        lpJobAttributes: *mut std::ffi::c_void,
        lpName: *const u16,
    ) -> *mut std::ffi::c_void;

    fn SetInformationJobObject(
        hJob: *mut std::ffi::c_void,
        JobObjectInformationClass: u32,
        lpJobObjectInformation: *const std::ffi::c_void,
        cbJobObjectInformationLength: u32,
    ) -> i32;

    fn AssignProcessToJobObject(
        hJob: *mut std::ffi::c_void,
        hProcess: *mut std::ffi::c_void,
    ) -> i32;

    fn CloseHandle(
        hObject: *mut std::ffi::c_void,
    ) -> i32;
}

const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x00002000;
const JOBOBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: u32 = 9;

pub struct WinJob {
    handle: RawHandle,
}

unsafe impl Send for WinJob {}
unsafe impl Sync for WinJob {}

impl WinJob {
    pub fn create() -> Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
            if handle.is_null() {
                return Err(anyhow!("Failed to create Job Object: {}", std::io::Error::last_os_error()));
            }

            let mut info = std::mem::zeroed::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>();
            info.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            let res = SetInformationJobObject(
                handle,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );

            if res == 0 {
                let err = std::io::Error::last_os_error();
                CloseHandle(handle);
                return Err(anyhow!("Failed to set Job Object information: {}", err));
            }

            Ok(WinJob { handle })
        }
    }

    pub fn assign_process(&self, process_handle: RawHandle) -> Result<()> {
        unsafe {
            let res = AssignProcessToJobObject(self.handle, process_handle);
            if res == 0 {
                return Err(anyhow!("Failed to assign process to Job Object: {}", std::io::Error::last_os_error()));
            }
            Ok(())
        }
    }
}

impl Drop for WinJob {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

pub fn clean_absolute_path(path: &Path) -> Result<PathBuf> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    
    // canonicalize to resolve symlinks and relative parts
    if abs.exists() {
        let canon = std::fs::canonicalize(&abs)?;
        let path_str = canon.to_string_lossy();
        let cleaned = if path_str.starts_with(r"\\?\") {
            PathBuf::from(&path_str[4..])
        } else {
            canon
        };
        Ok(cleaned)
    } else {
        Ok(abs)
    }
}

pub fn write_cline_config(workspace_dir: &Path, host: &str, port: u16, _ctx_size: usize, _thinking: bool) -> Result<()> {
    let home_dir = workspace_dir.join("home");
    let node_dir = workspace_dir.join("node");

    let abs_home_dir = clean_absolute_path(&home_dir)?;
    let abs_node_dir = clean_absolute_path(&node_dir)?;
    let abs_cline_dir = clean_absolute_path(&home_dir.join(".cline"))?;

    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{};{}", abs_node_dir.to_string_lossy(), current_path);

    let base_url = format!("http://{}:{}/v1", host, port);

    println!("Registering Cline provider...");
    let status = Command::new("cmd")
        .args([
            "/C",
            "node",
            "node_modules\\cline\\bin\\cline",
            "auth",
            "--provider", "openai",
            "--apikey", "llama.cpp",
            "--modelid", "local-model",
            "--baseurl", &base_url,
        ])
        .current_dir(workspace_dir)
        .env("USERPROFILE", &abs_home_dir)
        .env("HOME", &abs_home_dir)
        .env("PATH", &new_path)
        .env("CLINE_DATA_DIR", &abs_cline_dir)
        .env("CLINE_PROVIDER_SETTINGS_PATH", &abs_cline_dir.join("settings").join("providers.json"))
        .env("CLINE_GLOBAL_SETTINGS_PATH", &abs_cline_dir.join("settings").join("global-settings.json"))
        .status()
        .context("Failed to run cline auth command")?;

    if !status.success() {
        return Err(anyhow!("Failed to configure Cline provider via auth command"));
    }

    // 3. Write run_cline.bat in workspace
    let bat_path = workspace_dir.join("run_cline.bat");
    let bat_content = "@echo off\r\nsetlocal\r\nset \"PATH=%~dp0node;%PATH%\"\r\nset \"CLINE_DATA_DIR=%~dp0home\\.cline\"\r\nset \"CLINE_PROVIDER_SETTINGS_PATH=%~dp0home\\.cline\\settings\\providers.json\"\r\nset \"CLINE_GLOBAL_SETTINGS_PATH=%~dp0home\\.cline\\settings\\global-settings.json\"\r\n\"%~dp0node\\node.exe\" \"%~dp0node_modules\\cline\\bin\\cline\" %*\r\n";
    std::fs::write(&bat_path, bat_content)?;

    Ok(())
}

pub fn start_llama_server(
    llama_dir: &Path,
    backend: &str,
    model_path: &Path,
    host: &str,
    port: u16,
    ctx_size: usize,
    n_gpu_layers: usize,
    hide_second_terminal: bool,
) -> Result<Child> {
    let server_exe = llama_dir.join(backend).join("llama-server.exe");
    if !server_exe.exists() {
        return Err(anyhow!("llama-server.exe not found at {}", server_exe.display()));
    }

    let flags = if hide_second_terminal {
        CREATE_NO_WINDOW
    } else {
        CREATE_NEW_CONSOLE
    };

    println!("Starting llama.cpp server (backend: {})...", backend);
    let child = Command::new(&server_exe)
        .args([
            "-m", &model_path.to_string_lossy(),
            "--host", host,
            "--port", &port.to_string(),
            "-c", &ctx_size.to_string(),
            "-ngl", &n_gpu_layers.to_string(),
        ])
        .creation_flags(flags)
        .spawn()
        .context("Failed to spawn llama-server.exe")?;

    Ok(child)
}

pub fn run_cline_agent(workspace_dir: &Path) -> Result<Child> {
    let home_dir = workspace_dir.join("home");
    let node_dir = workspace_dir.join("node");
    let project_dir = workspace_dir.join("project");

    std::fs::create_dir_all(&project_dir)?;

    let abs_home_dir = clean_absolute_path(&home_dir)?;
    let abs_node_dir = clean_absolute_path(&node_dir)?;
    let abs_cline_dir = clean_absolute_path(&home_dir.join(".cline"))?;

    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{};{}", abs_node_dir.to_string_lossy(), current_path);

    println!("Launching Cline coding agent directly in the current terminal...");
    let child = Command::new("cmd")
        .args(["/C", "..\\run_cline.bat"])
        .env("USERPROFILE", &abs_home_dir)
        .env("HOME", &abs_home_dir)
        .env("PATH", &new_path)
        .env("CLINE_DATA_DIR", &abs_cline_dir)
        .env("CLINE_PROVIDER_SETTINGS_PATH", &abs_cline_dir.join("settings").join("providers.json"))
        .env("CLINE_GLOBAL_SETTINGS_PATH", &abs_cline_dir.join("settings").join("global-settings.json"))
        .current_dir(&project_dir)
        .spawn()
        .context("Failed to launch Cline agent in the current terminal")?;

    Ok(child)
}
