# mcp-probe-kit-cli-version: 4.0.1
param(
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$ProbeArguments
)

$ErrorActionPreference = "Stop"
$PackageSpec = "mcp-probe-kit@4.0.1"
$WrapperRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$LocalCheckoutEntry = Join-Path $WrapperRoot "build\index.js"
$CacheRoot = if ($env:MCP_PROBE_NPX_CACHE) {
  $env:MCP_PROBE_NPX_CACHE
} elseif ($env:LOCALAPPDATA) {
  Join-Path $env:LOCALAPPDATA "mcp-probe-kit\npm-cache"
} else {
  Join-Path $WrapperRoot ".mcp-probe-kit\npm-cache"
}

if ($env:MCP_PROBE_LOCAL_ENTRY) {
  & node $env:MCP_PROBE_LOCAL_ENTRY @ProbeArguments
  exit $LASTEXITCODE
} else {
  & npx.cmd --yes --cache $CacheRoot $PackageSpec @ProbeArguments
  exit $LASTEXITCODE
}
