use std::fs::File;
use std::io::Write;
use std::path::Path;
use anyhow::{Result, anyhow, Context};
use futures_util::StreamExt;
use reqwest::Client;

// ─── Platform-agnostic download/extract helpers ──────────────────

pub async fn download_file(url: &str, dest: &Path) -> Result<()> {
    println!("Downloading {}...", url);
    let client = Client::new();
    let response = client.get(url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .context("Failed to send request")?;

    if !response.status().is_success() {
        return Err(anyhow!("Failed to download: HTTP status {}", response.status()));
    }

    let total_size = response.content_length();
    let mut file = File::create(dest).context("Failed to create destination file")?;
    let mut stream = response.bytes_stream();

    let mut downloaded: u64 = 0;
    let mut last_printed = std::time::Instant::now();

    while let Some(item) = stream.next().await {
        let chunk = item.context("Error while downloading chunk")?;
        file.write_all(&chunk).context("Failed to write to file")?;
        downloaded += chunk.len() as u64;

        if last_printed.elapsed().as_millis() > 500 {
            if let Some(total) = total_size {
                let percent = (downloaded as f64 / total as f64) * 100.0;
                print!("\rDownloaded: {:.2} MB / {:.2} MB ({:.1}%)",
                       downloaded as f64 / 1024.0 / 1024.0,
                       total as f64 / 1024.0 / 1024.0,
                       percent);
            } else {
                print!("\rDownloaded: {:.2} MB (unknown size)", downloaded as f64 / 1024.0 / 1024.0);
            }
            std::io::stdout().flush()?;
            last_printed = std::time::Instant::now();
        }
    }
    println!("\nDownload finished successfully.");
    Ok(())
}

pub fn extract_zip(zip_path: &Path, output_dir: &Path) -> Result<()> {
    println!("Extracting {} to {}...", zip_path.display(), output_dir.display());
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    std::fs::create_dir_all(output_dir)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => output_dir.join(path),
            None => continue,
        };

        if file.name().ends_with('/') {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    std::fs::create_dir_all(p)?;
                }
            }
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }
    println!("Extraction completed.");
    Ok(())
}

/// Extract a `.tar.xz` archive (Linux Node.js distribution).
pub fn extract_tar_xz(tar_path: &Path, output_dir: &Path) -> Result<()> {
    println!("Extracting {} to {}...", tar_path.display(), output_dir.display());
    let file = File::open(tar_path)?;
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(output_dir)?;
    println!("Extraction completed.");
    Ok(())
}

/// Extract a `.tar.gz` archive (Linux llama.cpp builds).
pub fn extract_tar_gz(tar_path: &Path, output_dir: &Path) -> Result<()> {
    println!("Extracting {} to {}...", tar_path.display(), output_dir.display());
    let file = File::open(tar_path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(output_dir)?;
    println!("Extraction completed.");
    Ok(())
}

/// Extract and flatten a single-top-level archive into `output_dir`.
/// On Windows expects `.zip`; on Linux expects `.tar.xz`.
pub fn extract_and_flatten_node(archive_path: &Path, output_dir: &Path) -> Result<()> {
    let temp_dir = output_dir.parent().unwrap().join("temp_node_extract");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }

    #[cfg(windows)]
    { extract_zip(archive_path, &temp_dir)?; }

    #[cfg(not(windows))]
    { extract_tar_xz(archive_path, &temp_dir)?; }

    let entries = std::fs::read_dir(&temp_dir)?;
    let mut sub_dirs = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            sub_dirs.push(entry.path());
        }
    }

    // On Linux, Node.js tarball has bin/, lib/, include/, share/ inside
    // the top-level dir. On Windows the .exe and .cmd are at the top level.
    #[cfg(windows)]
    {
        if sub_dirs.len() == 1 {
            let inner_dir = &sub_dirs[0];
            std::fs::create_dir_all(output_dir)?;
            let inner_entries = std::fs::read_dir(inner_dir)?;
            for entry in inner_entries {
                let entry = entry?;
                let dest = output_dir.join(entry.file_name());
                if dest.exists() {
                    if dest.is_dir() { std::fs::remove_dir_all(&dest)?; }
                    else { std::fs::remove_file(&dest)?; }
                }
                std::fs::rename(entry.path(), dest)?;
            }
        } else {
            std::fs::create_dir_all(output_dir)?;
            let inner_entries = std::fs::read_dir(&temp_dir)?;
            for entry in inner_entries {
                let entry = entry?;
                let dest = output_dir.join(entry.file_name());
                std::fs::rename(entry.path(), dest)?;
            }
        }
    }

    #[cfg(not(windows))]
    {
        if sub_dirs.len() == 1 {
            let inner_dir = &sub_dirs[0];
            // Move everything from the inner dir into output_dir
            std::fs::create_dir_all(output_dir)?;
            copy_dir_all(inner_dir, output_dir)?;
        } else {
            std::fs::create_dir_all(output_dir)?;
            let inner_entries = std::fs::read_dir(&temp_dir)?;
            for entry in inner_entries {
                let entry = entry?;
                let dest = output_dir.join(entry.file_name());
                copy_dir_all(&entry.path(), &dest)?;
            }
        }
    }

    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    // Preserve symlinks instead of following them
    if src.is_symlink() {
        let target = std::fs::read_link(src)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, dst)?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&target, dst)?;
        return Ok(());
    }
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            copy_dir_all(&src_path, &dst_path)?;
        }
    } else {
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

// ─── Platform-specific constants ─────────────────────────────────

#[cfg(windows)]
mod platform {
    pub const LLAMA_OS_TAG: &str = "win";
    pub const LLAMA_ARCHIVE_EXT: &str = ".zip";
    pub const LLAMA_SERVER_EXE: &str = "llama-server.exe";
    pub const FALLBACK_LLAMA_CPU_URL: &str = "https://github.com/ggml-org/llama.cpp/releases/download/b9885/llama-b9885-bin-win-cpu-x64.zip";
    pub const FALLBACK_LLAMA_VULKAN_URL: &str = "https://github.com/ggml-org/llama.cpp/releases/download/b9885/llama-b9885-bin-win-vulkan-x64.zip";
    pub const NODE_DOWNLOAD_URL: &str = "https://nodejs.org/dist/v22.23.1/node-v22.23.1-win-x64.zip";
}

#[cfg(not(windows))]
mod platform {
    pub const LLAMA_OS_TAG: &str = "ubuntu";
    pub const LLAMA_ARCHIVE_EXT: &str = ".tar.gz";
    pub const LLAMA_SERVER_EXE: &str = "llama-server";
    pub const FALLBACK_LLAMA_CPU_URL: &str = "https://github.com/ggml-org/llama.cpp/releases/download/b9885/llama-b9885-bin-ubuntu-x64.tar.gz";
    pub const FALLBACK_LLAMA_VULKAN_URL: &str = "https://github.com/ggml-org/llama.cpp/releases/download/b9885/llama-b9885-bin-ubuntu-vulkan-x64.tar.gz";
    pub const NODE_DOWNLOAD_URL: &str = "https://nodejs.org/dist/v22.23.1/node-v22.23.1-linux-x64.tar.xz";
}

// ─── llama.cpp setup ─────────────────────────────────────────────

pub async fn get_latest_llama_url(backend: &str) -> Result<String> {
    let client = Client::new();
    let response = client.get("https://api.github.com/repos/ggml-org/llama.cpp/releases/latest")
        .header("User-Agent", "zerocline-installer")
        .send()
        .await
        .context("Failed to query GitHub API for llama.cpp")?;

    if !response.status().is_success() {
        return Err(anyhow!("GitHub API returned error: {}", response.status()));
    }

    let json: serde_json::Value = response.json().await?;
    let assets = json.get("assets").and_then(|a| a.as_array())
        .ok_or_else(|| anyhow!("No assets found in GitHub release response"))?;

    let os_tag = platform::LLAMA_OS_TAG;
    let ext = platform::LLAMA_ARCHIVE_EXT;

    for asset in assets {
        if let Some(name) = asset.get("name").and_then(|n| n.as_str()) {
            if name.contains(&format!("bin-{}", os_tag))
                && name.contains("x64")
                && name.ends_with(ext)
            {
                let matches_backend = if backend == "vulkan" {
                    name.contains("vulkan")
                } else {
                    !name.contains("vulkan") && !name.contains("cuda") && !name.contains("sycl") && !name.contains("openvino") && !name.contains("arm64")
                        && !name.contains("rocm") && !name.contains("hip")
                };

                if matches_backend {
                    if let Some(url) = asset.get("browser_download_url").and_then(|u| u.as_str()) {
                        return Ok(url.to_string());
                    }
                }
            }
        }
    }

    Err(anyhow!("Could not find suitable llama.cpp asset in latest release for backend: {} on OS: {}", backend, os_tag))
}

pub async fn setup_llama_backend(llama_dir: &Path, backend: &str) -> Result<()> {
    let backend_dir = llama_dir.join(backend);
    let server_exe = backend_dir.join(platform::LLAMA_SERVER_EXE);

    if server_exe.exists() {
        println!("{} for backend '{}' already exists. Skipping download.", platform::LLAMA_SERVER_EXE, backend);
        return Ok(());
    }

    std::fs::create_dir_all(&backend_dir)?;

    #[cfg(windows)]
    let llama_archive_name = "llama.zip";
    #[cfg(not(windows))]
    let llama_archive_name = "llama.tar.gz";

    let llama_archive_path = backend_dir.join(llama_archive_name);

    let llama_url = match get_latest_llama_url(backend).await {
        Ok(url) => url,
        Err(e) => {
            println!("Warning: failed to query GitHub API: {}. Using fallback URL.", e);
            if backend == "vulkan" {
                platform::FALLBACK_LLAMA_VULKAN_URL.to_string()
            } else {
                platform::FALLBACK_LLAMA_CPU_URL.to_string()
            }
        }
    };

    download_file(&llama_url, &llama_archive_path).await?;

    let temp_extract = backend_dir.parent().unwrap().join("temp_llama_extract");
    if temp_extract.exists() {
        std::fs::remove_dir_all(&temp_extract)?;
    }

    #[cfg(windows)]
    extract_zip(&llama_archive_path, &temp_extract)?;
    #[cfg(not(windows))]
    extract_tar_gz(&llama_archive_path, &temp_extract)?;

    // Flatten single top-level directory (common in llama.cpp archives)
    let entries: Vec<_> = std::fs::read_dir(&temp_extract)?.filter_map(|e| e.ok()).collect();
    if entries.len() == 1 && entries[0].path().is_dir() {
        let inner = entries[0].path();
        for entry in std::fs::read_dir(&inner)? {
            let entry = entry?;
            let dest = backend_dir.join(entry.file_name());
            if entry.path().is_dir() {
                copy_dir_all(&entry.path(), &dest)?;
            } else {
                std::fs::rename(&entry.path(), &dest)?;
            }
        }
        std::fs::remove_dir_all(&temp_extract)?;
    } else {
        // No wrapping directory — move everything up
        for entry in entries {
            let dest = backend_dir.join(entry.file_name());
            if entry.path().is_dir() {
                copy_dir_all(&entry.path(), &dest)?;
            } else {
                std::fs::rename(&entry.path(), &dest)?;
            }
        }
        std::fs::remove_dir_all(&temp_extract)?;
    }

    if llama_archive_path.exists() {
        std::fs::remove_file(&llama_archive_path)?;
    }

    // Make server executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if server_exe.exists() {
            std::fs::set_permissions(&server_exe, std::fs::Permissions::from_mode(0o755))?;
        }
    }

    Ok(())
}

// ─── Cline agent installation ────────────────────────────────────

pub fn install_cline_agent(workspace_dir: &Path) -> Result<()> {
    println!("Installing Cline coding agent in workspace...");

    #[cfg(windows)]
    {
        let npm_path = workspace_dir.join("node").join("npm.cmd");
        let output = std::process::Command::new("cmd")
            .args(["/C", &npm_path.to_string_lossy(), "install", "--ignore-scripts", "cline"])
            .current_dir(workspace_dir)
            .output()
            .context("Failed to run npm install for Cline agent")?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("npm install failed: {}", err));
        }
    }

    #[cfg(not(windows))]
    {
        let npm_path = workspace_dir.join("node").join("bin").join("npm");
        let node_bin = workspace_dir.join("node").join("bin");
        let new_path = format!("{}:{}", node_bin.to_string_lossy(),
            std::env::var("PATH").unwrap_or_default());
        let output = std::process::Command::new(&npm_path)
            .args(["install", "--ignore-scripts", "cline"])
            .env("PATH", &new_path)
            .current_dir(workspace_dir)
            .output()
            .context("Failed to run npm install for Cline agent")?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("npm install failed: {}", err));
        }
    }

    println!("Cline coding agent installed successfully.");
    Ok(())
}

// ─── Model config generation ─────────────────────────────────────

pub fn generate_default_model_config(model_dir: &Path, model_name: &str) -> Result<crate::config::ModelConfig> {
    std::fs::create_dir_all(model_dir)?;

    let mut gguf_filename = None;
    if let Ok(entries) = std::fs::read_dir(model_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("gguf") {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    gguf_filename = Some(name.to_string());
                    break;
                }
            }
        }
    }

    let defaults_data = include_str!("config.json");
    let defaults: Vec<crate::config::ModelConfig> = serde_json::from_str(defaults_data)
        .map_err(|e| anyhow::anyhow!("Failed to parse src/config.json: {}", e))?;

    let default_model_config = defaults.into_iter().find(|m| m.name == model_name);

    let config = if let Some(mut m) = default_model_config {
        if let Some(fname) = gguf_filename {
            m.filename = fname;
        }
        m
    } else {
        crate::config::ModelConfig {
            name: model_name.to_string(),
            filename: gguf_filename.unwrap_or_else(|| format!("{}.gguf", model_name)),
            download_url: "".to_string(),
            ctx_size: 6000,
            n_gpu_layers: 0,
            temperature: 0.0,
            thinking: model_name.to_lowercase().contains("thinking"),
        }
    };

    let config_path = model_dir.join("config.json");
    config.save(&config_path)?;
    Ok(config)
}

// ─── Auto-setup orchestrator ─────────────────────────────────────

pub async fn run_auto_setup(root_dir: &Path, backend: &str, default_model: &str) -> Result<()> {
    println!("=== Auto-Setup ZeroCline Portable Suite ===");

    let workspace_dir = root_dir.join("workspace");
    let llama_dir = root_dir.join("llama");
    let gguf_dir = root_dir.join("gguf");

    std::fs::create_dir_all(&workspace_dir)?;
    std::fs::create_dir_all(&llama_dir)?;
    std::fs::create_dir_all(&gguf_dir)?;

    // ── Node.js ────────────────────────────────────────────────
    let node_url = platform::NODE_DOWNLOAD_URL;
    let node_archive_name = Path::new(node_url).file_name().unwrap().to_string_lossy().to_string();
    let node_archive_path = workspace_dir.join(&node_archive_name);
    let node_dir = workspace_dir.join("node");

    let node_exists = if cfg!(windows) {
        node_dir.join("node.exe").exists()
    } else {
        node_dir.join("bin").join("node").exists()
    };

    if !node_exists {
        download_file(node_url, &node_archive_path).await?;
        if node_dir.exists() {
            std::fs::remove_dir_all(&node_dir)?;
        }
        extract_and_flatten_node(&node_archive_path, &node_dir)?;
        if node_archive_path.exists() {
            std::fs::remove_file(&node_archive_path)?;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let node_bin = node_dir.join("bin").join("node");
            if node_bin.exists() {
                std::fs::set_permissions(&node_bin, std::fs::Permissions::from_mode(0o755))?;
            }
            let npm_bin = node_dir.join("bin").join("npm");
            if npm_bin.exists() {
                std::fs::set_permissions(&npm_bin, std::fs::Permissions::from_mode(0o755))?;
            }
            let npx_bin = node_dir.join("bin").join("npx");
            if npx_bin.exists() {
                std::fs::set_permissions(&npx_bin, std::fs::Permissions::from_mode(0o755))?;
            }
        }
    } else {
        println!("Node.js already exists. Skipping download.");
    }

    // ── Install Cline agent ────────────────────────────────────
    if !workspace_dir.join("node_modules").join("cline").exists() {
        install_cline_agent(&workspace_dir)?;
    } else {
        println!("Cline coding agent already installed. Skipping.");
    }

    // ── llama.cpp backend ──────────────────────────────────────
    setup_llama_backend(&llama_dir, backend).await?;

    // ── Model GGUF ─────────────────────────────────────────────
    let model_dir = gguf_dir.join(default_model);
    let model_config_path = model_dir.join("config.json");

    if !model_config_path.exists() {
        generate_default_model_config(&model_dir, default_model)?;
    }

    let model_config = crate::config::ModelConfig::load(&model_config_path)?;
    let model_file_path = model_dir.join(&model_config.filename);

    if !model_file_path.exists() {
        if model_config.download_url.is_empty() {
            return Err(anyhow!(
                "Model GGUF file is missing at '{}'. Since no download URL is configured, please download the GGUF file and place it in that directory as '{}'.",
                model_file_path.display(),
                model_config.filename
            ));
        } else {
            download_file(&model_config.download_url, &model_file_path).await?;
        }
    } else {
        println!("Model GGUF file already exists. Skipping download.");
    }

    // ── Root config ────────────────────────────────────────────
    let root_config_path = root_dir.join("zerocline_config.json");
    if !root_config_path.exists() {
        let default_config = crate::config::ZeroClineConfig::default();
        let content = serde_json::to_string_pretty(&default_config)?;
        std::fs::write(root_config_path, content)?;
    }

    println!("\n=== Auto-Setup Complete! ===");
    println!("You can now run zerocline again to launch the server and agent.");
    Ok(())
}
