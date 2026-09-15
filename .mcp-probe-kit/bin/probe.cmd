@echo off
rem mcp-probe-kit-cli-version: 4.0.1
setlocal
for %%I in ("%~dp0..\..") do set "MCP_PROBE_WRAPPER_ROOT=%%~fI"
if defined MCP_PROBE_NPX_CACHE (
  set "MCP_PROBE_CACHE=%MCP_PROBE_NPX_CACHE%"
) else (
  set "MCP_PROBE_CACHE=%LOCALAPPDATA%\mcp-probe-kit\npm-cache"
)
if defined MCP_PROBE_LOCAL_ENTRY (
  call node "%MCP_PROBE_LOCAL_ENTRY%" %*
) else (
  call npx.cmd --yes --cache "%MCP_PROBE_CACHE%" mcp-probe-kit@4.0.1 %*
)
set "MCP_PROBE_EXIT=%ERRORLEVEL%"
exit /b %MCP_PROBE_EXIT%
