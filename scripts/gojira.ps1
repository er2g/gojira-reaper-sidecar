param(
  [ValidateSet('doctor', 'build-dll', 'install-dll', 'ui-dev', 'all')]
  [string]$Task = 'all',

  [switch]$Release,

  [string]$ReaperUserPluginsDir,

  [string]$EnvFile
)

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
if (-not $EnvFile) {
  $EnvFile = Join-Path $RepoRoot '.env'
}

function Invoke-Checked {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$Args
  )

  & $FilePath @Args
  if ($LASTEXITCODE -ne 0) {
    throw "Command failed ($LASTEXITCODE): $FilePath $($Args -join ' ')"
  }
}

function Load-DotEnv {
  param([Parameter(Mandatory = $true)][string]$Path)

  if (-not (Test-Path -LiteralPath $Path)) {
    return
  }

  Get-Content -LiteralPath $Path | ForEach-Object {
    $line = $_.Trim()
    if (-not $line) { return }
    if ($line.StartsWith('#')) { return }

    $eq = $line.IndexOf('=')
    if ($eq -lt 1) { return }

    $key = $line.Substring(0, $eq).Trim()
    $value = $line.Substring($eq + 1).Trim()

    if (($value.StartsWith('"') -and $value.EndsWith('"')) -or ($value.StartsWith("'") -and $value.EndsWith("'"))) {
      $value = $value.Substring(1, $value.Length - 2)
    }

    if ($key) {
      Set-Item -Path "env:$key" -Value $value
    }
  }

  if (-not $env:GEMINI_API_KEY -and $env:GEMINI_API) {
    $env:GEMINI_API_KEY = $env:GEMINI_API
  }
}

function Get-ReaperUserPluginsDir {
  if ($ReaperUserPluginsDir) { return $ReaperUserPluginsDir }
  if ($env:REAPER_USERPLUGINS_DIR) { return $env:REAPER_USERPLUGINS_DIR }

  if (-not $env:APPDATA) {
    throw "APPDATA is not set; pass -ReaperUserPluginsDir or set REAPER_USERPLUGINS_DIR"
  }

  return (Join-Path $env:APPDATA 'REAPER\UserPlugins')
}

function Get-BuiltDllPath {
  $profile = if ($Release) { 'release' } else { 'debug' }
  $dir = Join-Path (Join-Path $RepoRoot 'target') $profile
  return (Join-Path $dir 'reaper_gojira_dll.dll')
}

function Task-Doctor {
  Load-DotEnv -Path $EnvFile

  Write-Host "Repo root: $RepoRoot"
  Write-Host "Env file:  $EnvFile"
  Write-Host "DLL:       $(Get-BuiltDllPath)"
  Write-Host "Install:   $(Get-ReaperUserPluginsDir)"
  Write-Host ""
  Write-Host "Checks:"

  foreach ($cmd in @('cargo', 'npm')) {
    $found = Get-Command $cmd -ErrorAction SilentlyContinue
    if ($found) {
      Write-Host "  OK: $cmd -> $($found.Source)"
    } else {
      Write-Host "  MISSING: $cmd (install Rust toolchain / Node.js)"
    }
  }

  if ($env:GEMINI_API_KEY) {
    Write-Host "  OK: GEMINI_API_KEY is set"
  } else {
    Write-Host "  WARN: GEMINI_API_KEY is not set (.env.example)"
  }
}

function Task-BuildDll {
  Load-DotEnv -Path $EnvFile

  Push-Location $RepoRoot
  try {
    $args = @('build', '-p', 'reaper_gojira_dll')
    if ($Release) { $args += '--release' }
    Invoke-Checked -FilePath 'cargo' -Args $args
  } finally {
    Pop-Location
  }

  $dll = Get-BuiltDllPath
  if (-not (Test-Path -LiteralPath $dll)) {
    throw "Build succeeded but DLL not found: $dll"
  }

  Write-Host "Built: $dll"
}

function Task-InstallDll {
  Task-BuildDll

  $dstDir = Get-ReaperUserPluginsDir
  New-Item -ItemType Directory -Force -Path $dstDir | Out-Null

  $src = Get-BuiltDllPath
  $dst = Join-Path $dstDir (Split-Path -Leaf $src)

  try {
    # If REAPER has the extension loaded, the DLL is locked and copy will fail.
    $fs = [System.IO.File]::Open($dst, [System.IO.FileMode]::OpenOrCreate, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
    $fs.Close()
  } catch {
    throw "Cannot overwrite '$dst'. Close REAPER (or unload the extension) and re-run GOJIRA_INSTALL_DLL."
  }

  Copy-Item -Force -LiteralPath $src -Destination $dst

  Write-Host "Installed: $dst"
}

function Test-CargoTauriAvailable {
  Push-Location (Join-Path $RepoRoot 'gojira_brain_ui\src-tauri')
  try {
    & cargo tauri -V *> $null
    return ($LASTEXITCODE -eq 0)
  } catch {
    return $false
  } finally {
    Pop-Location
  }
}

function Test-TcpPortOpen {
  param(
    [Parameter(Mandatory = $true)][string]$Hostname,
    [Parameter(Mandatory = $true)][int]$Port,
    [int]$TimeoutMs = 200
  )

  try {
    $client = [System.Net.Sockets.TcpClient]::new()
    $iar = $client.BeginConnect($Hostname, $Port, $null, $null)
    if (-not $iar.AsyncWaitHandle.WaitOne($TimeoutMs)) {
      try { $client.Close() } catch {}
      return $false
    }
    $client.EndConnect($iar) | Out-Null
    $client.Close()
    return $true
  } catch {
    return $false
  }
}

function Ensure-ViteDevServer {
  param(
    [Parameter(Mandatory = $true)][string]$UiDir,
    [int]$Port = 5173
  )

  if (Test-TcpPortOpen -Hostname '127.0.0.1' -Port $Port -TimeoutMs 150) {
    Write-Host "Vite already listening on port $Port."
    return
  }

  Write-Host "Starting Vite dev server on port $Port..."
  # IMPORTANT: On some systems `npm` resolves to `npm.ps1` and `Start-Process npm` will open it in Notepad
  # due to file association (ShellExecute). Always spawn via PowerShell so it executes as a command.
  Start-Process -WorkingDirectory $UiDir -FilePath 'powershell.exe' -ArgumentList @(
    '-NoProfile',
    '-ExecutionPolicy', 'Bypass',
    '-Command', 'npm run dev'
  ) | Out-Null

  $deadline = (Get-Date).AddSeconds(15)
  while ((Get-Date) -lt $deadline) {
    if (Test-TcpPortOpen -Hostname '127.0.0.1' -Port $Port -TimeoutMs 150) {
      Write-Host "Vite is up (port $Port)."
      return
    }
    Start-Sleep -Milliseconds 200
  }

  throw "Vite failed to start on port $Port within 15s."
}

function Stop-GojiraBrainUiIfRunning {
  $procs = Get-Process -Name 'gojira_brain_ui_tauri' -ErrorAction SilentlyContinue
  if (-not $procs) { return }

  Write-Host "Stopping existing 'gojira_brain_ui_tauri' process(es)..."
  foreach ($p in $procs) {
    try { Stop-Process -Id $p.Id -Force -ErrorAction Stop } catch {}
  }

  Start-Sleep -Milliseconds 300
}

function Task-UiDev {
  Load-DotEnv -Path $EnvFile

  $uiDir = Join-Path $RepoRoot 'gojira_brain_ui\ui'
  $tauriDir = Join-Path $RepoRoot 'gojira_brain_ui\src-tauri'

  if (-not (Test-Path -LiteralPath (Join-Path $uiDir 'node_modules'))) {
    Invoke-Checked -FilePath 'npm' -Args @('--prefix', $uiDir, 'install')
  }

  if (Test-CargoTauriAvailable) {
    Ensure-ViteDevServer -UiDir $uiDir -Port 5173
    Stop-GojiraBrainUiIfRunning
    $overrideConfig = Join-Path $RepoRoot 'scripts\tauri.dev.override.json'
    Push-Location $tauriDir
    try {
      # We manage Vite ourselves (idempotent); disable Tauri's beforeDevCommand to avoid port conflicts.
      Invoke-Checked -FilePath 'cargo' -Args @('tauri', 'dev', '--no-dev-server-wait', '--config', $overrideConfig)
    } finally {
      Pop-Location
    }
    return
  }

  Write-Host "WARN: 'cargo tauri' not found; falling back to 'npm dev' + 'cargo run'."

  Ensure-ViteDevServer -UiDir $uiDir -Port 5173

  Push-Location $tauriDir
  try {
    Invoke-Checked -FilePath 'cargo' -Args @('run')
  } finally {
    Pop-Location
  }
}

switch ($Task) {
  'doctor' { Task-Doctor }
  'build-dll' { Task-BuildDll }
  'install-dll' { Task-InstallDll }
  'ui-dev' { Task-UiDev }
  'all' {
    Task-InstallDll
    Task-UiDev
  }
}
