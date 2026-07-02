mod config;
mod setup;
mod process;

use anyhow::{Result, Context};

#[tokio::main]
async fn main() -> Result<()> {
    let exe_path = std::env::current_exe().context("Failed to get current executable path")?;
    let root_dir = exe_path.parent().ok_or_else(|| anyhow::anyhow!("No parent directory for executable"))?;

    let config_path = root_dir.join("zerocline_config.json");
    let config = config::ZeroClineConfig::load_or_create(&config_path)?;

    let workspace_dir = root_dir.join("workspace");
    let llama_dir = root_dir.join("llama");
    let gguf_dir = root_dir.join("gguf");

    // Check if the structure exists
    let structure_missing = !workspace_dir.exists()
        || !llama_dir.exists()
        || !gguf_dir.exists()
        || !llama_dir.join(&config.backend).join("llama-server.exe").exists()
        || !workspace_dir.join("node").join("node.exe").exists()
        || !workspace_dir.join("node_modules").join("cline").exists();

    // Auto-generate model config if missing and the model folder exists
    let model_dir = gguf_dir.join(&config.default_model);
    let model_config_path = model_dir.join("config.json");
    if !model_config_path.exists() && gguf_dir.exists() {
        println!("Model config.json not found for '{}'. Generating a default config...", config.default_model);
        let _ = setup::generate_default_model_config(&model_dir, &config.default_model);
    }

    // Check if default model exists
    let mut model_exists = false;
    if model_config_path.exists() {
        if let Ok(model_config) = config::ModelConfig::load(&model_config_path) {
            let model_file = model_dir.join(&model_config.filename);
            if model_file.exists() {
                model_exists = true;
            }
        }
    }

    if structure_missing || !model_exists {
        println!("Required directories or files are missing. Running auto-setup...");
        setup::run_auto_setup(root_dir, &config.backend, &config.default_model).await?;
    }

    // Load default model configuration
    let model_config = config::ModelConfig::load(&model_config_path)?;
    let model_file_path = model_dir.join(&model_config.filename);

    // Sync configuration for Cline
    process::write_cline_config(
        &workspace_dir,
        &config.llama_host,
        config.llama_port,
        model_config.ctx_size,
        model_config.thinking,
    )?;

    // Create a Job Object to ensure child processes are terminated when the parent exits or is closed
    let job = process::WinJob::create()?;

    // Start llama server
    let mut llama_server = process::start_llama_server(
        &llama_dir,
        &config.backend,
        &model_file_path,
        &config.llama_host,
        config.llama_port,
        model_config.ctx_size,
        model_config.n_gpu_layers,
        config.hide_second_terminal,
    )?;

    // Assign llama_server to the job object
    use std::os::windows::io::AsRawHandle;
    job.assign_process(llama_server.as_raw_handle())?;

    // Wait for llama server to initialize and load model
    println!("Waiting for llama.cpp server to initialize and load model...");
    let health_url = format!("http://{}:{}/health", config.llama_host, config.llama_port);
    let client = reqwest::Client::new();
    let start_time = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(60);
    let mut server_ready = false;

    while start_time.elapsed() < timeout {
        if let Ok(resp) = client.get(&health_url).send().await {
            if resp.status().is_success() {
                server_ready = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    if !server_ready {
        println!("Warning: llama.cpp server health check timed out. Attempting to start agent anyway...");
    } else {
        println!("llama.cpp server is ready and model is loaded.");
    }

    // Launch Cline agent terminal window and wait for it
    let mut cline_terminal = process::run_cline_agent(&workspace_dir)?;

    // Assign cline_terminal to the job object
    job.assign_process(cline_terminal.as_raw_handle())?;

    println!("Cline coding agent started. Waiting for it to exit...");
    let _ = cline_terminal.wait();

    // Cline agent terminal has closed, stop llama server
    println!("Cline terminal closed. Stopping llama.cpp server...");
    let _ = llama_server.kill();
    let _ = llama_server.wait();
    println!("Stopped. Goodbye!");

    Ok(())
}
