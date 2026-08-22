param(
  [string]$Version = "",
  [string]$InstallRoot = "$env:LOCALAPPDATA\sshxx",
  [switch]$Run
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$Repository = "glight2000/sshxx"

$Architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($Architecture -ne [System.Runtime.InteropServices.Architecture]::X64) {
  throw "The current Windows release supports x64 only; detected $Architecture."
}
$Target = "x86_64-pc-windows-msvc"

if (-not $Version) {
  $Headers = @{ "User-Agent" = "sshxx-installer" }
  $Release = Invoke-RestMethod -Headers $Headers `
    -Uri "https://api.github.com/repos/$Repository/releases/latest"
  $Version = $Release.tag_name
}
$Version = $Version -replace '^v', ''
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
  throw "Invalid release version: $Version"
}

$Asset = "sshxx-runtime-$Version-$Target.zip"
$BaseUrl = "https://github.com/$Repository/releases/download/v$Version"
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) `
  ("sshxx-install-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $TempDir | Out-Null

try {
  $ArchivePath = Join-Path $TempDir $Asset
  $ChecksumsPath = Join-Path $TempDir "SHA256SUMS"
  Write-Host "Downloading sshxx v$Version for $Target..."
  Invoke-WebRequest -Uri "$BaseUrl/$Asset" -OutFile $ArchivePath
  Invoke-WebRequest -Uri "$BaseUrl/SHA256SUMS" -OutFile $ChecksumsPath

  $EscapedAsset = [regex]::Escape($Asset)
  $ChecksumLine = Get-Content $ChecksumsPath | Where-Object {
    $_ -match "^([0-9a-fA-F]{64})\s+\*?$EscapedAsset$"
  } | Select-Object -First 1
  if (-not $ChecksumLine) {
    throw "SHA256SUMS does not contain $Asset"
  }
  $Expected = ($ChecksumLine -split '\s+')[0].ToLowerInvariant()
  $Actual = (Get-FileHash -Algorithm SHA256 $ArchivePath).Hash.ToLowerInvariant()
  if ($Actual -ne $Expected) {
    throw "Checksum verification failed for $Asset"
  }

  $ExtractDir = Join-Path $TempDir "extract"
  Expand-Archive -Path $ArchivePath -DestinationPath $ExtractDir
  $ArchiveRoot = "sshxx-runtime-$Version-$Target"
  $SourceDir = Join-Path $ExtractDir $ArchiveRoot
  $RequiredFiles = @(
    "bin\sshxx-daemon.exe",
    "bin\sshxx-terminal-host.exe",
    "bin\sshxx-server.exe",
    "build\spa.html"
  )
  foreach ($File in $RequiredFiles) {
    if (-not (Test-Path (Join-Path $SourceDir $File))) {
      throw "Release archive is incomplete: missing $File"
    }
  }

  $VersionsDir = Join-Path $InstallRoot "versions"
  $VersionDir = Join-Path $VersionsDir $Version
  $WrapperDir = Join-Path $InstallRoot "bin"
  New-Item -ItemType Directory -Force -Path $VersionsDir, $WrapperDir | Out-Null
  if (-not (Test-Path $VersionDir)) {
    Move-Item -Path $SourceDir -Destination $VersionDir
  } else {
    foreach ($File in $RequiredFiles) {
      if (-not (Test-Path (Join-Path $VersionDir $File))) {
        throw "Existing installation is incomplete: $VersionDir"
      }
    }
  }
  Set-Content -Encoding ASCII -Path (Join-Path $InstallRoot "current-version") `
    -Value $Version

  $DaemonWrapper = @'
@echo off
set "ROOT=%~dp0.."
set /p VERSION=<"%ROOT%\current-version"
"%ROOT%\versions\%VERSION%\bin\sshxx-daemon.exe" %*
'@
  $HostWrapper = @'
@echo off
set "ROOT=%~dp0.."
set /p VERSION=<"%ROOT%\current-version"
"%ROOT%\versions\%VERSION%\bin\sshxx-terminal-host.exe" %*
'@
  $ServerWrapper = @'
@echo off
set "ROOT=%~dp0.."
set /p VERSION=<"%ROOT%\current-version"
cd /d "%ROOT%\versions\%VERSION%"
".\bin\sshxx-server.exe" %*
'@
  Set-Content -Encoding ASCII -Path (Join-Path $WrapperDir "sshxx-daemon.cmd") `
    -Value $DaemonWrapper
  Set-Content -Encoding ASCII -Path (Join-Path $WrapperDir "sshxx-terminal-host.cmd") `
    -Value $HostWrapper
  Set-Content -Encoding ASCII -Path (Join-Path $WrapperDir "sshxx-server.cmd") `
    -Value $ServerWrapper

  $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
  $PathEntries = @($UserPath -split ';' | Where-Object { $_ })
  if ($PathEntries -notcontains $WrapperDir) {
    [Environment]::SetEnvironmentVariable(
      "Path",
      (($PathEntries + $WrapperDir) -join ';'),
      "User"
    )
  }
  if (($env:Path -split ';') -notcontains $WrapperDir) {
    $env:Path = "$WrapperDir;$env:Path"
  }

  Write-Host "Installed sshxx v$Version in $VersionDir"
  Write-Host "Commands are available in $WrapperDir"

  if ($Run) {
    Write-Host "Starting a local sshxx server on http://127.0.0.1:8051..."
    $Server = Start-Process -PassThru -WorkingDirectory $VersionDir `
      -FilePath (Join-Path $VersionDir "bin\sshxx-server.exe") `
      -ArgumentList "--listen", "127.0.0.1"
    try {
      $Ready = $false
      for ($Attempt = 0; $Attempt -lt 50; $Attempt++) {
        if ($Server.HasExited) {
          throw "sshxx-server exited before becoming ready; is port 8051 already in use?"
        }
        try {
          Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:8051/" | Out-Null
          $Ready = $true
          break
        } catch {
          Start-Sleep -Milliseconds 100
        }
      }
      if (-not $Ready) {
        throw "sshxx-server did not become ready."
      }
      Write-Host "Starting sshxx-daemon; local data uses the current directory."
      & (Join-Path $VersionDir "bin\sshxx-daemon.exe") `
        --server http://127.0.0.1:8051
    } finally {
      if ($Server -and -not $Server.HasExited) {
        Stop-Process -Id $Server.Id
      }
    }
  } else {
    Write-Host "Run a minimal local workspace with:"
    Write-Host "  & ([scriptblock]::Create((irm https://raw.githubusercontent.com/$Repository/main/scripts/install.ps1))) -Run"
  }
} finally {
  Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}
