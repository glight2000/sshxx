param(
  [Parameter(Position = 0)]
  [ValidateSet(
    "help", "install", "start", "stop", "restart", "status", "logs",
    "check-update", "update", "uninstall", "run"
  )]
  [string]$Command = "help",
  [string]$Workspace = "",
  [ValidateSet("User")]
  [string]$Scope = "User",
  [string]$Listen = "127.0.0.1",
  [ValidateRange(1, 65535)]
  [int]$Port = 8051,
  [ValidateSet("server", "terminal-host", "daemon")]
  [string]$Role = "server",
  [string]$InstallRoot = "",
  [switch]$Force,
  [switch]$PurgeData
)

$ErrorActionPreference = "Stop"
$Repository = "glight2000/sshxx"
$TaskPrefix = "sshxx-"

function Show-Usage {
  @"
Manage a persistent sshxx Runtime with Windows Task Scheduler.

Usage: sshxx-service <command> [options]

Commands:
  install       Register and start three per-user background tasks.
  start         Start all tasks without disrupting an already-running host.
  stop          Stop server and daemon; the terminal host remains running.
  restart       Restart server and daemon; the terminal host remains running.
  status        Show task, HTTP, and terminal-host status.
  logs          Follow service logs.
  check-update  Compare the installed Runtime with the latest GitHub Release.
  update        Install the latest Runtime and restart server and daemon.
  uninstall     Remove tasks and Runtime; workspace data is kept by default.

Install options:
  -Workspace PATH       Durable daemon data directory.
  -Listen ADDRESS       Server listen address (default: 127.0.0.1).
  -Port PORT            Server port (default: 8051).

Uninstall options:
  -Force                Disconnect active hosted terminals.
  -PurgeData            Also remove the configured workspace directory.
"@ | Write-Host
}

if (-not $InstallRoot) {
  if ($env:SSHXX_INSTALL_ROOT) {
    $InstallRoot = $env:SSHXX_INSTALL_ROOT
  } else {
    $InstallRoot = Split-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) -Parent
  }
}
$InstallRoot = [System.IO.Path]::GetFullPath($InstallRoot)
$ConfigDirectory = Join-Path $InstallRoot "service"
$ConfigPath = Join-Path $ConfigDirectory "config.json"
$CurrentVersionPath = Join-Path $InstallRoot "current-version"
$WrapperDirectory = Join-Path $InstallRoot "bin"
$LogsDirectory = Join-Path $InstallRoot "logs"

function Get-Configuration {
  if (-not (Test-Path $ConfigPath)) {
    throw "Managed tasks are not installed. Run 'sshxx-service install' first."
  }
  Get-Content -Raw $ConfigPath | ConvertFrom-Json
}

function Get-ServerUrl([object]$Configuration) {
  $HostName = $Configuration.listen
  if ($HostName -eq "0.0.0.0") { $HostName = "127.0.0.1" }
  if ($HostName -eq "::" -or $HostName -eq "[::]") { $HostName = "::1" }
  if ($HostName.Contains(":") -and -not $HostName.StartsWith("[")) {
    $HostName = "[$HostName]"
  }
  "http://${HostName}:$($Configuration.port)"
}

function Get-Wrapper([string]$Name) {
  Join-Path $WrapperDirectory "$Name.cmd"
}

function Invoke-HostStatus([object]$Configuration) {
  Push-Location $Configuration.workspace
  try {
    & (Get-Wrapper "sshxx-terminal-host") status --state-dir `
      (Join-Path $Configuration.workspace "cache\terminal-host")
    if ($LASTEXITCODE -ne 0) { throw "sshxx-terminal-host status failed." }
  } finally {
    Pop-Location
  }
}

function Wait-Host([object]$Configuration) {
  for ($Attempt = 0; $Attempt -lt 50; $Attempt++) {
    try {
      Invoke-HostStatus $Configuration | Out-Null
      return
    } catch {
      Start-Sleep -Milliseconds 100
    }
  }
  throw "sshxx-terminal-host did not become ready."
}

function Wait-Web([object]$Configuration) {
  $Url = "$(Get-ServerUrl $Configuration)/"
  for ($Attempt = 0; $Attempt -lt 50; $Attempt++) {
    try {
      Invoke-WebRequest -UseBasicParsing $Url | Out-Null
      return
    } catch {
      Start-Sleep -Milliseconds 100
    }
  }
  throw "sshxx-server did not become ready at $Url"
}

function Register-ManagedTasks([object]$Configuration) {
  $CurrentVersion = (Get-Content $CurrentVersionPath).Trim()
  $ServiceScript = Join-Path $InstallRoot "versions\$CurrentVersion\scripts\service.ps1"
  $PowerShell = "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe"
  $UserId = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
  $Principal = New-ScheduledTaskPrincipal -UserId $UserId `
    -LogonType Interactive -RunLevel Limited
  $Trigger = New-ScheduledTaskTrigger -AtLogOn -User $UserId
  $Settings = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew `
    -RestartCount 10 -RestartInterval (New-TimeSpan -Minutes 1) `
    -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable

  foreach ($TaskRole in @("server", "terminal-host", "daemon")) {
    $Arguments = "-NoProfile -NonInteractive -ExecutionPolicy Bypass " +
      "-File `"$ServiceScript`" run -Role $TaskRole -InstallRoot `"$InstallRoot`""
    $Action = New-ScheduledTaskAction -Execute $PowerShell -Argument $Arguments `
      -WorkingDirectory $Configuration.workspace
    Register-ScheduledTask -TaskName "$TaskPrefix$TaskRole" -Action $Action `
      -Trigger $Trigger -Principal $Principal -Settings $Settings -Force | Out-Null
  }
}

function Start-ManagedTasks([object]$Configuration) {
  Start-ScheduledTask -TaskName "${TaskPrefix}server"
  Wait-Web $Configuration
  Start-ScheduledTask -TaskName "${TaskPrefix}terminal-host"
  Wait-Host $Configuration
  Start-ScheduledTask -TaskName "${TaskPrefix}daemon"
}

function Stop-FrontendTasks {
  Stop-ScheduledTask -TaskName "${TaskPrefix}daemon" -ErrorAction SilentlyContinue
  Stop-ScheduledTask -TaskName "${TaskPrefix}server" -ErrorAction SilentlyContinue
}

function Install-ManagedTasks {
  if (-not $Workspace) { $Workspace = Join-Path $HOME "sshxx-workspace" }
  $Workspace = [System.IO.Path]::GetFullPath($Workspace)
  New-Item -ItemType Directory -Force $Workspace, $ConfigDirectory, $LogsDirectory | Out-Null
  $Configuration = [ordered]@{
    workspace = $Workspace
    scope = "User"
    listen = $Listen
    port = $Port
  }
  $Configuration | ConvertTo-Json | Set-Content -Encoding UTF8 $ConfigPath
  Stop-FrontendTasks
  Register-ManagedTasks $Configuration
  Start-ManagedTasks $Configuration
  Write-Host "Registered Windows per-user managed tasks."
}

function Show-Status([object]$Configuration) {
  Get-ScheduledTask -TaskName "${TaskPrefix}server", "${TaskPrefix}terminal-host", `
    "${TaskPrefix}daemon" | Select-Object TaskName, State | Format-Table
  $Url = "$(Get-ServerUrl $Configuration)/"
  Invoke-WebRequest -UseBasicParsing $Url | Out-Null
  Write-Host "Web check: PASS ($Url)"
  Invoke-HostStatus $Configuration
}

function Follow-Logs {
  $Paths = @("server", "terminal-host", "daemon") |
    ForEach-Object { Join-Path $LogsDirectory "$_.log" }
  foreach ($Path in $Paths) {
    if (-not (Test-Path $Path)) { New-Item -ItemType File $Path | Out-Null }
  }
  Get-Content -Tail 100 -Wait $Paths
}

function Get-LatestVersion {
  $Headers = @{ "User-Agent" = "sshxx-service" }
  $Release = Invoke-RestMethod -Headers $Headers `
    -Uri "https://api.github.com/repos/$Repository/releases/latest"
  $Release.tag_name -replace '^v', ''
}

function Check-Update {
  $Installed = (Get-Content $CurrentVersionPath).Trim()
  $Latest = Get-LatestVersion
  Write-Host "Installed Runtime: $Installed"
  Write-Host "Latest Runtime:    $Latest"
  if ($Installed -eq $Latest) {
    Write-Host "Runtime is up to date."
  } else {
    Write-Host "Runtime update available. Run: sshxx-service update"
  }
}

function Update-Runtime([object]$Configuration) {
  $CurrentVersion = (Get-Content $CurrentVersionPath).Trim()
  $Installer = Join-Path $InstallRoot "versions\$CurrentVersion\scripts\install.ps1"
  if (-not (Test-Path $Installer)) {
    throw "Installed Runtime does not contain its installer: $Installer"
  }
  & $Installer -InstallRoot $InstallRoot -Managed `
    -Workspace $Configuration.workspace -Scope User `
    -Listen $Configuration.listen -Port $Configuration.port
}

function Stop-TerminalHost([object]$Configuration) {
  $Arguments = @(
    "stop", "--state-dir",
    (Join-Path $Configuration.workspace "cache\terminal-host")
  )
  if ($Force) { $Arguments += "--force" }
  Push-Location $Configuration.workspace
  try {
    & (Get-Wrapper "sshxx-terminal-host") @Arguments
    if ($LASTEXITCODE -ne 0) {
      throw "Uninstall stopped because terminal-host still owns active terminals."
    }
  } finally {
    Pop-Location
  }
  Stop-ScheduledTask -TaskName "${TaskPrefix}terminal-host" `
    -ErrorAction SilentlyContinue
}

function Uninstall-Managed([object]$Configuration) {
  Stop-TerminalHost $Configuration
  Stop-FrontendTasks
  foreach ($TaskRole in @("daemon", "server", "terminal-host")) {
    Unregister-ScheduledTask -TaskName "$TaskPrefix$TaskRole" `
      -Confirm:$false -ErrorAction SilentlyContinue
  }

  $WorkspacePath = [System.IO.Path]::GetFullPath($Configuration.workspace)
  $HomePath = [System.IO.Path]::GetFullPath($HOME)
  if ($PurgeData) {
    if ($WorkspacePath -eq $HomePath -or $WorkspacePath -eq `
      [System.IO.Path]::GetPathRoot($WorkspacePath)) {
      throw "Refusing to purge unsafe workspace path: $WorkspacePath"
    }
    Remove-Item -Recurse -Force $WorkspacePath
    Write-Host "Removed workspace data: $WorkspacePath"
  } elseif ($WorkspacePath.StartsWith("$InstallRoot\", [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Move the workspace outside $InstallRoot or rerun with -PurgeData."
  }

  $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
  $Entries = @($UserPath -split ';' | Where-Object {
    $_ -and $_ -ne $WrapperDirectory
  })
  [Environment]::SetEnvironmentVariable("Path", ($Entries -join ';'), "User")
  Remove-Item -Recurse -Force $InstallRoot
  Write-Host "Removed sshxx Runtime. Workspace data was preserved at $WorkspacePath"
}

function Run-ManagedRole([object]$Configuration) {
  $LogPath = Join-Path $LogsDirectory "$Role.log"
  New-Item -ItemType Directory -Force $LogsDirectory | Out-Null
  Push-Location $Configuration.workspace
  try {
    switch ($Role) {
      "server" {
        & (Get-Wrapper "sshxx-server") --listen $Configuration.listen `
          --port $Configuration.port >> $LogPath 2>&1
      }
      "terminal-host" {
        & (Get-Wrapper "sshxx-terminal-host") serve --state-dir `
          (Join-Path $Configuration.workspace "cache\terminal-host") `
          >> $LogPath 2>&1
      }
      "daemon" {
        Wait-Host $Configuration
        Wait-Web $Configuration
        & (Get-Wrapper "sshxx-daemon") --server `
          (Get-ServerUrl $Configuration) >> $LogPath 2>&1
      }
    }
    exit $LASTEXITCODE
  } finally {
    Pop-Location
  }
}

switch ($Command) {
  "help" { Show-Usage }
  "install" { Install-ManagedTasks }
  "start" { Start-ManagedTasks (Get-Configuration) }
  "stop" {
    Stop-FrontendTasks
    Write-Host "Stopped daemon and server; terminal-host remains running."
  }
  "restart" {
    $Configuration = Get-Configuration
    Stop-FrontendTasks
    Start-ScheduledTask -TaskName "${TaskPrefix}server"
    Wait-Web $Configuration
    Start-ScheduledTask -TaskName "${TaskPrefix}daemon"
    Write-Host "Restarted daemon and server; terminal-host was not restarted."
  }
  "status" { Show-Status (Get-Configuration) }
  "logs" { Follow-Logs }
  "check-update" { Check-Update }
  "update" { Update-Runtime (Get-Configuration) }
  "uninstall" { Uninstall-Managed (Get-Configuration) }
  "run" { Run-ManagedRole (Get-Configuration) }
}
