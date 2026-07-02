# ZeroCline: Portable Cline Coding Agent Suite

ZeroCline is a portable, self-contained Rust executable designed for Windows to download, configure, and orchestrate the lifecycle of the **Cline coding agent CLI** alongside a **llama.cpp** backend server, keeping everything strictly isolated in a single directory.

---

## Key Features

1. **Strict Portability & Isolation**
   - Zero dependencies on system-wide software.
   - Automatically downloads and installs a portable Node.js runtime and llama.cpp backend inside the folder.
   - Redirects the Cline configurations (`providers.json`) locally within the workspace directory using explicit environment variables (`CLINE_DATA_DIR`, `CLINE_PROVIDER_SETTINGS_PATH`, `CLINE_GLOBAL_SETTINGS_PATH`), keeping your host system completely clean.

2. **Advanced Process Lifecycle Management (Windows Job Objects)**
   - Utilizes standard Windows FFI bindings for **Job Objects**.
   - Spawns child processes (`llama-server.exe` and the `node.exe` Cline CLI) inside a shared Job Object configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
   - **Graceful Termination**: Closing the main console window, force-killing `zerocline.exe`, or any sudden crash immediately guarantees that the background model server and the agent CLI processes are terminated cleanly by the Windows kernel.

3. **Robust Health Check Initialization**
   - Automatically polls the local `llama-server` `/health` endpoint on startup.
   - Blocks launching the Cline TUI until the local GGUF model is fully parsed and loaded in memory (handling model initialization latency and preventing immediate "Error: Loading model" failures).

4. **Multi-Backend Support (Vulkan & CPU)**
   - Fully supports Vulkan hardware acceleration and standard CPU fallbacks for `llama.cpp` inference.

5. **Safe Default GPU Allocation**
   - Uses CPU layers allocation (`n_gpu_layers: 0`) as the default configuration out-of-the-box. This ensures reliable startups without Vulkan VRAM-allocation abort failures on diverse system memory sizes. Users can easily customize layers to utilize more VRAM.

6. **Configurable Console Hiding**
   - Hides the secondary `llama-server.exe` terminal automatically for a clean workspace, while providing an optional setting to display logs in a second window.

---

## Directory Structure

When running ZeroCline, it establishes and maintains the following clean structure within its directory:

```text
zerocline\
├── README.md               # Detailed project documentation
├── Cargo.toml              # Rust project description
├── zerocline_config.json   # Suite configuration
├── zerocline.exe           # Orchestrator binary
├── llama\
│   └── vulkan\
│       └── (vulkan binaries)
├── gguf\
│   └── <model-name>\
│       ├── <model-name>.gguf
│       └── config.json     # Model config (defaults to CPU loading)
└── workspace\              # Home of the Cline Agent
    ├── node\               # Isolated Node.js runtime
    ├── node_modules\       # Installed agent modules
    ├── home\
    │   └── .cline\         # Localized portable Cline data folder
    │       └── settings\
    │           └── providers.json # Registered provider details
    └── run_cline.bat       # Execution wrapper
```

---

## Configuration (`zerocline_config.json`)

The global configuration file `zerocline_config.json` allows configuring the backend, target model, and runner options:

```json
{
  "default_model": "nvidia-nemotron-3-nano-4b",
  "llama_port": 8080,
  "llama_host": "127.0.0.1",
  "backend": "vulkan",
  "hide_second_terminal": true
}
```

### Config Options
* `default_model`: Folder name under `gguf/` containing the model GGUF and config.
* `llama_port`: The network port the `llama-server` listens on (default: `8080`).
* `llama_host`: The address host the backend binds to (default: `127.0.0.1`).
* `backend`: Inference backend to download and use (`vulkan` or `cpu`).
* `hide_second_terminal`: Set to `true` (default) to run the `llama-server` invisibly in the background. Set to `false` to open a second command prompt displaying real-time generation and token inference logs.

---

## Model Config (`gguf/<model-name>/config.json`)

If a model folder is scanned and does not contain a configuration, ZeroCline automatically generates one with safe defaults:

```json
{
  "name": "nvidia-nemotron-3-nano-4b",
  "filename": "NVIDIA-Nemotron3-Nano-4B-Q4_K_M.gguf",
  "download_url": "https://huggingface.co/nvidia/NVIDIA-Nemotron-3-Nano-4B-GGUF/resolve/main/NVIDIA-Nemotron3-Nano-4B-Q4_K_M.gguf",
  "ctx_size": 10000,
  "n_gpu_layers": 0,
  "temperature": 0.4,
  "thinking": true
}
```

* `n_gpu_layers`: Number of layers to offload to the GPU (set to `0` by default for safe CPU loading). Users with capable GPUs can change this to offload more layers to memory.
* `thinking`: A boolean (defaults to `false`). Set to `true` for models that support native reasoning/thinking output patterns.

---

## Building and Running

### Prerequisites
- Rust compiler (2024 edition or newer)
- Active Internet Connection (only during the first launch or auto-setup)

### 1. Build the Release Binary
Run standard cargo build to generate the optimized executable:
```powershell
cargo build --release
```

### 2. Copy the Executable
Copy the compiled binary from the target directory to your root directory:
```powershell
Copy-Item -Path "target/release/zerocline.exe" -Destination "zerocline.exe" -Force
```

### 3. Run
Launch the application:
```powershell
./zerocline.exe
```
*On the first run, ZeroCline will detect missing runtimes and automatically trigger the auto-setup routine to download Node.js, the specified llama backend, and the target model, and configure the workspace environment before booting directly into the interactive Cline CLI TUI.*

---

## Credits

ZeroCline orchestrates and depends upon the following excellent open-source projects:

- **[Cline CLI](https://github.com/cline/cline)**: The interactive terminal-based AI coding assistant.
- **[llama.cpp](https://github.com/ggerganov/llama.cpp)**: The highly optimized LLM inference engine powering the local backend server.

---

## License

GNU General Public License v3.0
