//! Standalone headless CLI & daemon binary (`coding-tools-mcp`).
//!
//! Compiles with `--no-default-features` so Linux servers, containers, and SSH
//! workstations do not link `libwebkit2gtk` or `libgtk-3`.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = coding_tools_mcp_desktop_lib::run_cli_entry(&args);
    std::process::exit(code);
}
