use std::path::{Path, PathBuf};
use std::process::{Command, Child};
use anyhow::{Result, anyhow, Context};

// ─── Platform abstractions ───────────────────────────────────────

#[cfg(windows)]
mod platform {
    use std::os::windows::process::CommandExt;
    use std::os::windows::io::RawHandle;
    use anyhow::{Result, anyhow};

    pub const CREATE_NO_WINDOW: u32 = 0x08000000;
    pub const CREATE_NEW_CONSOLE: u32 = 0x00000010;

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
            unsafe { CloseHandle(self.handle); }
        }
    }

    /// Apply Windows-specific creation flags to a Command.
    pub fn apply_creation_flags(cmd: &mut Command, hide: bool) {
        let flags = if hide { CREATE_NO_WINDOW } else { CREATE_NEW_CONSOLE };
        cmd.creation_flags(flags);
    }

    pub type ProcessHandle = RawHandle;
    pub fn get_process_handle(child: &Child) -> RawHandle {
        use std::os::windows::io::AsRawHandle;
        child.as_raw_handle()
    }
}

#[cfg(unix)]
#[allow(dead_code)]
mod platform {
    use std::process::{Command, Child, Stdio};
    use anyhow::Result;

    pub fn apply_creation_flags(cmd: &mut Command, hide: bool) {
        if hide {
            cmd.stdout(Stdio::null());
            cmd.stderr(Stdio::null());
        }
    }

    pub type ProcessHandle = u32;
    pub fn get_process_handle(child: &Child) -> u32 {
        child.id()
    }

    /// No-op on Unix — we just kill children on drop.
    pub struct UnixJob;
    impl UnixJob {
        pub fn create() -> Result<Self> { Ok(UnixJob) }
        pub fn assign_process(&self, _pid: u32) -> Result<()> { Ok(()) }
    }
}

// ─── Path separator helpers ──────────────────────────────────────

#[cfg(windows)]
fn path_sep() -> &'static str { ";" }

#[cfg(not(windows))]
fn path_sep() -> &'static str { ":" }

fn join_path(parts: &[&str]) -> String {
    parts.join(path_sep())
}

// ─── Binary names ────────────────────────────────────────────────

#[cfg(windows)]
mod bin_names {
    use std::path::PathBuf;
    pub fn llama_server(backend_dir: &std::path::Path) -> PathBuf {
        backend_dir.join("llama-server.exe")
    }
    pub fn node_bin(node_dir: &std::path::Path) -> PathBuf {
        node_dir.join("node.exe")
    }
    pub fn node_path(node_dir: &std::path::Path) -> String {
        node_dir.to_string_lossy().to_string()
    }
    pub fn cline_script_name() -> &'static str { "run_cline.bat" }
    pub fn cline_command(cline_script: &std::path::Path) -> std::process::Command {
        let mut cmd = std::process::Command::new("cmd");
        let script_path = format!("..\\{}", cline_script.file_name().unwrap().to_string_lossy());
        cmd.args(["/C", &script_path]);
        cmd
    }
    pub fn cline_auth_command(_node_path: &std::path::Path) -> std::process::Command {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "node", "node_modules\\cline\\bin\\cline", "auth"]);
        cmd
    }
}

#[cfg(not(windows))]
mod bin_names {
    use std::path::PathBuf;
    pub fn llama_server(backend_dir: &std::path::Path) -> PathBuf {
        backend_dir.join("llama-server")
    }
    pub fn node_bin(node_dir: &std::path::Path) -> PathBuf {
        node_dir.join("bin").join("node")
    }
    pub fn node_path(node_dir: &std::path::Path) -> String {
        node_dir.join("bin").to_string_lossy().to_string()
    }
    pub fn cline_script_name() -> &'static str { "run_cline.sh" }
    pub fn cline_command(cline_script: &std::path::Path) -> std::process::Command {
        // Script has #!/bin/bash and is chmod'd +x; execute directly
        std::process::Command::new(cline_script)
    }
    pub fn cline_auth_command(node_bin: &std::path::Path) -> std::process::Command {
        let mut cmd = std::process::Command::new(node_bin);
        let cline_path = std::path::Path::new("node_modules").join("cline").join("bin").join("cline");
        cmd.args([cline_path.to_string_lossy().as_ref(), "auth"]);
        cmd
    }
}

// ─── Shared helpers ──────────────────────────────────────────────

pub fn clean_absolute_path(path: &Path) -> Result<PathBuf> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    if abs.exists() {
        let canon = std::fs::canonicalize(&abs)?;
        Ok(canon)
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
    let new_path = join_path(&[&bin_names::node_path(&abs_node_dir), &current_path]);

    let base_url = format!("http://{}:{}/v1", host, port);

    println!("Registering Cline provider...");
    let auth_node_bin = bin_names::node_bin(&abs_node_dir);
    let mut auth_cmd = bin_names::cline_auth_command(&auth_node_bin);
    auth_cmd
        .args([
            "--provider", "openai",
            "--apikey", "llama.cpp",
            "--modelid", "local-model",
            "--baseurl", &base_url,
        ])
        .current_dir(workspace_dir)
        .env("HOME", &abs_home_dir)
        .env("PATH", &new_path)
        .env("CLINE_DATA_DIR", &abs_cline_dir)
        .env("CLINE_PROVIDER_SETTINGS_PATH", &abs_cline_dir.join("settings").join("providers.json"))
        .env("CLINE_GLOBAL_SETTINGS_PATH", &abs_cline_dir.join("settings").join("global-settings.json"));

    #[cfg(windows)]
    {
        auth_cmd.env("USERPROFILE", &abs_home_dir);
    }

    let status = auth_cmd.status()
        .context("Failed to run cline auth command")?;

    if !status.success() {
        return Err(anyhow!("Failed to configure Cline provider via auth command"));
    }

    // Write run script
    let script_name = bin_names::cline_script_name();
    let script_path = workspace_dir.join(script_name);

    #[cfg(windows)]
    {
        let bat_content = "@echo off\r\nsetlocal\r\nset \"PATH=%~dp0node;%PATH%\"\r\nset \"CLINE_DATA_DIR=%~dp0home\\.cline\"\r\nset \"CLINE_PROVIDER_SETTINGS_PATH=%~dp0home\\.cline\\settings\\providers.json\"\r\nset \"CLINE_GLOBAL_SETTINGS_PATH=%~dp0home\\.cline\\settings\\global-settings.json\"\r\n\"%~dp0node\\node.exe\" \"%~dp0node_modules\\cline\\bin\\cline\" %*\r\n";
        std::fs::write(&script_path, bat_content)?;
    }

    #[cfg(not(windows))]
    {
        let cline_script = workspace_dir.join("node_modules").join("cline").join("bin").join("cline");
        let sh_content = format!(
            r#"#!/bin/bash
export PATH="{}:$PATH"
export CLINE_DATA_DIR="{}"
export CLINE_PROVIDER_SETTINGS_PATH="{}"
export CLINE_GLOBAL_SETTINGS_PATH="{}"
exec "{}" "{}" "$@"
"#,
            bin_names::node_path(&abs_node_dir),
            abs_cline_dir.to_string_lossy(),
            abs_cline_dir.join("settings").join("providers.json").to_string_lossy(),
            abs_cline_dir.join("settings").join("global-settings.json").to_string_lossy(),
            bin_names::node_bin(&abs_node_dir).to_string_lossy(),
            cline_script.to_string_lossy(),
        );
        std::fs::write(&script_path, sh_content)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))?;
    }

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
    let backend_dir = llama_dir.join(backend);
    let server_exe = bin_names::llama_server(&backend_dir);
    if !server_exe.exists() {
        return Err(anyhow!("llama-server not found at {}", server_exe.display()));
    }

    println!("Starting llama.cpp server (backend: {})...", backend);
    let mut cmd = Command::new(&server_exe);
    cmd.args([
        "-m", &model_path.to_string_lossy(),
        "--host", host,
        "--port", &port.to_string(),
        "-c", &ctx_size.to_string(),
        "-ngl", &n_gpu_layers.to_string(),
    ]);

    platform::apply_creation_flags(&mut cmd, hide_second_terminal);

    let child = cmd.spawn()
        .context("Failed to spawn llama-server")?;

    Ok(child)
}

pub fn run_cline_agent(workspace_dir: &Path) -> Result<Child> {
    use std::io::IsTerminal;

    let home_dir = workspace_dir.join("home");
    let node_dir = workspace_dir.join("node");
    let project_dir = workspace_dir.join("project");

    std::fs::create_dir_all(&project_dir)?;

    let abs_home_dir = clean_absolute_path(&home_dir)?;
    let abs_node_dir = clean_absolute_path(&node_dir)?;
    let abs_cline_dir = clean_absolute_path(&home_dir.join(".cline"))?;

    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = join_path(&[&bin_names::node_path(&abs_node_dir), &current_path]);

    println!("Launching Cline coding agent...");
    let script_path = workspace_dir.join(bin_names::cline_script_name());

    #[cfg(windows)]
    {
        let mut cmd = bin_names::cline_command(&script_path);
        cmd.env("HOME", &abs_home_dir)
            .env("PATH", &new_path)
            .env("CLINE_DATA_DIR", &abs_cline_dir)
            .env("CLINE_PROVIDER_SETTINGS_PATH", &abs_cline_dir.join("settings").join("providers.json"))
            .env("CLINE_GLOBAL_SETTINGS_PATH", &abs_cline_dir.join("settings").join("global-settings.json"))
            .env("USERPROFILE", &abs_home_dir)
            .current_dir(&project_dir);
        let child = cmd.spawn().context("Failed to launch Cline agent")?;
        return Ok(child);
    }

    #[cfg(not(windows))]
    {
        if std::io::stdin().is_terminal() {
            let mut cmd = bin_names::cline_command(&script_path);
            cmd.env("HOME", &abs_home_dir)
                .env("PATH", &new_path)
                .env("CLINE_DATA_DIR", &abs_cline_dir)
                .env("CLINE_PROVIDER_SETTINGS_PATH", &abs_cline_dir.join("settings").join("providers.json"))
                .env("CLINE_GLOBAL_SETTINGS_PATH", &abs_cline_dir.join("settings").join("global-settings.json"))
                .current_dir(&project_dir);
            let child = cmd.spawn().context("Failed to launch Cline agent")?;
            return Ok(child);
        }

        // No TTY — spawn Cline in a terminal emulator window
        let terminals: &[(&str, &[&str])] = &[
            ("x-terminal-emulator", &["-e"]),
            ("gnome-terminal", &["--wait", "--"]),
            ("konsole", &["-e"]),
            ("xterm", &["-e"]),
            ("urxvt", &["-e"]),
            ("rxvt", &["-e"]),
            ("alacritty", &["-e"]),
            ("kitty", &["-e"]),
            ("terminator", &["-e"]),
            ("xfce4-terminal", &["-e"]),
            ("lxterminal", &["-e"]),
        ];

        for (term, args) in terminals {
            if let Ok(child) = std::process::Command::new(term)
                .args(*args)
                .arg(&script_path)
                .env("HOME", &abs_home_dir)
                .env("PATH", &new_path)
                .env("CLINE_DATA_DIR", &abs_cline_dir)
                .env("CLINE_PROVIDER_SETTINGS_PATH", &abs_cline_dir.join("settings").join("providers.json"))
                .env("CLINE_GLOBAL_SETTINGS_PATH", &abs_cline_dir.join("settings").join("global-settings.json"))
                .current_dir(&project_dir)
                .spawn()
            {
                return Ok(child);
            }
        }

        Err(anyhow::anyhow!("No terminal emulator found and stdin is not a terminal. Cannot launch Cline interactively."))
    }
}

// ─── Process manager (cross-platform cleanup) ────────────────────

pub struct ProcessManager {
    pub children: Vec<Child>,
    #[cfg(windows)]
    job: platform::WinJob,
    #[cfg(unix)]
    _job: platform::UnixJob,
}

impl ProcessManager {
    pub fn new() -> Result<Self> {
        #[cfg(windows)]
        { let job = platform::WinJob::create()?; return Ok(Self { children: vec![], job }); }
        #[cfg(unix)]
        { let _job = platform::UnixJob::create()?; return Ok(Self { children: vec![], _job }); }
    }

    pub fn add(&mut self, child: Child) -> Result<()> {
        #[cfg(windows)]
        { self.job.assign_process(platform::get_process_handle(&child))?; }
        self.children.push(child);
        Ok(())
    }

    /// Wait for the last child process to finish
    pub fn wait_last(&mut self) -> Result<()> {
        if let Some(child) = self.children.last_mut() {
            child.wait()?;
        }
        Ok(())
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
