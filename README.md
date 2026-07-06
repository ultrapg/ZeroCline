# ZeroCline: Portable Cline Coding Agent

ZeroCline is a portable, self-contained Rust executable that downloads, configures, and orchestrates the lifecycle of the **Cline coding agent CLI** alongside a **llama.cpp** backend server, keeping everything strictly isolated in a single directory. Supports **Windows** and **Linux**.

---

## Key Features

1. **Strict Portability & Isolation**
   - Zero dependencies on system-wide software.
   - Automatically downloads and installs a portable Node.js runtime and llama.cpp backend inside the folder.
   - Redirects Cline configurations (`providers.json`) locally within the workspace directory using explicit environment variables (`CLINE_DATA_DIR`, `CLINE_PROVIDER_SETTINGS_PATH`, `CLINE_GLOBAL_SETTINGS_PATH`), keeping your host system completely clean.

2. **Process Lifecycle Management**
   - **Windows**: Uses Job Objects (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) via FFI — closing the console or crashing guarantees child processes (`llama-server.exe`, `node.exe`) are terminated by the kernel.
   - **Linux**: Child processes are tracked by PID and cleaned up on exit via a signal handler.

3. **Automatic Terminal on Double-Click (Linux)**
   - When launched from a file manager (no terminal attached), ZeroCline spawns its own terminal emulator (`x-terminal-emulator`, `xterm`, `gnome-terminal`, etc.) so you see logs and the Cline TUI without manual setup.

4. **Robust Health Check Initialization**
   - Automatically polls the local `llama-server` `/health` endpoint on startup.
   - Blocks launching the Cline TUI until the local GGUF model is fully parsed and loaded in memory.

5. **Multi-Backend Support (Vulkan & CPU)**
   - Fully supports Vulkan hardware acceleration and standard CPU fallbacks for `llama.cpp` inference.

6. **Safe Default GPU Allocation**
   - Uses CPU layers allocation (`n_gpu_layers: 0`) as the default configuration out-of-the-box.

7. **Configurable Console Hiding (Windows)**
   - Hides the secondary `llama-server.exe` terminal automatically. Provides an optional setting to show logs in a second window (`hide_second_terminal`).

---

## Directory Structure

When running ZeroCline, it establishes and maintains the following structure within its directory:

### Windows
```text
zerocline\
├── README.md
├── Cargo.toml
├── zerocline_config.json
├── zerocline.exe
├── llama\
│   ├── vulkan\
│   │   └── (vulkan binaries)
│   └── cpu\
│       └── (cpu binaries)
├── gguf\
│   └── <model-name>\
│       ├── <model-name>.gguf
│       └── config.json
└── workspace\
    ├── node\
    ├── node_modules\
    ├── home\
    │   └── .cline\
    │       └── settings\
    │           └── providers.json
    └── run_cline.bat
```

### Linux
```text
zerocline/
├── README.md
├── Cargo.toml
├── zerocline_config.json
├── zerocline              # Executable
├── llama/
│   ├── vulkan/
│   │   └── (vulkan binaries)
│   └── cpu/
│       └── (cpu binaries)
├── gguf/
│   └── <model-name>/
│       ├── <model-name>.gguf
│       └── config.json
└── workspace/
    ├── node/
    ├── node_modules/
    ├── home/
    │   └── .cline/
    │       └── settings/
    │           └── providers.json
    └── run_cline.sh
```

---

## Configuration (`zerocline_config.json`)

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
- `default_model`: Folder name under `gguf/` containing the model GGUF and config.
- `llama_port`: The network port the `llama-server` listens on (default: `8080`).
- `llama_host`: The address host the backend binds to (default: `127.0.0.1`).
- `backend`: Inference backend to download and use (`vulkan` or `cpu`).
- `hide_second_terminal`: (Windows only) Set to `true` (default) to run `llama-server` invisibly. Set to `false` to open a second command prompt showing logs.

---

## Model Config (`gguf/<model-name>/config.json`)

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

- `n_gpu_layers`: Number of layers to offload to the GPU (set to `0` by default for safe CPU loading).
- `thinking`: Set to `true` for models that support native reasoning/thinking output patterns.

---

## Building and Running

### Prerequisites
- Rust compiler (2024 edition or newer)
- Active Internet Connection (only during the first launch or auto-setup)

### 1. Build
```sh
cargo build --release
```

### 2. Run
```sh
./target/release/zerocline
```

On the first run, ZeroCline will detect missing runtimes and automatically trigger the auto-setup routine to download Node.js, the specified llama backend, and the target model before booting directly into the interactive Cline CLI TUI.

### Linux Wrapper
A convenience `run.sh` script is provided — it calls the binary from the project root and passes through any arguments.

---

## Credits

ZeroCline orchestrates and depends upon the following excellent open-source projects:

- **[Cline CLI](https://github.com/cline/cline)**: The interactive terminal-based AI coding assistant.
- **[llama.cpp](https://github.com/ggerganov/llama.cpp)**: The highly optimized LLM inference engine powering the local backend server.

---

## License

GNU General Public License v3.0
