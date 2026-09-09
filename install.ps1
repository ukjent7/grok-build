#
# Grok CLI installer for PowerShell — BYOK fork build.
# Downloads release binaries from ukjent7/grok-build GitHub Releases.
#
# Auth: GROK_DEPLOYMENT_KEY env var (takes precedence) or ~/.grok/auth.json from `grok login`.
# Env: GROK_VERSION (a byok-vX.Y.Z tag, default: latest), GROK_BIN_DIR, GROK_PROXY_URL
#
# Usage:
#   irm https://raw.githubusercontent.com/ukjent7/grok-build/main/install.ps1 | iex                                    # latest byok release
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/ukjent7/grok-build/main/install.ps1))) -Version byok-v0.1.0  # pinned tag
#   $env:GROK_VERSION="byok-v0.1.0"; irm https://raw.githubusercontent.com/ukjent7/grok-build/main/install.ps1 | iex    # pinned tag (alt)
#   $env:GROK_DEPLOYMENT_KEY="<key>"; irm https://raw.githubusercontent.com/ukjent7/grok-build/main/install.ps1 | iex
#

param(
    [Parameter(Position = 0)]
    [string]$Version
)

$ErrorActionPreference = 'Stop'

# PS 5.1 defaults to TLS 1.0; GCS requires TLS 1.2.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

# PS 5.1's Invoke-WebRequest progress bar is extremely slow; disable it.
$ProgressPreference = 'SilentlyContinue'

# Accept version from environment variable (useful with irm | iex).
if (-not $Version -and $env:GROK_VERSION) {
    $Version = $env:GROK_VERSION
}

# This script is Windows-only. PS 5.1 has no Platform property and only runs on Windows.
if ($PSVersionTable.Platform -and $PSVersionTable.Platform -ne 'Win32NT') {
    Write-Error "This installer is for Windows. On macOS/Linux, use: curl -fsSL https://x.ai/cli/install.sh | bash"
    exit 1
}

$GrokDir = Join-Path $env:USERPROFILE '.grok'

# --- Helpers ---

function Download-String([string]$Url) {
    try {
        $response = Invoke-WebRequest -Uri $Url -UseBasicParsing
        return $response.Content
    } catch {
        return $null
    }
}

function Download-File([string]$Url, [string]$OutFile) {
    # TODO: parallel byte-range download (matches install.sh download_file_parallel).
    # Skipped for now: requires Start-ThreadJob / RunspacePool for true parallelism on PS 5.1
    # and HEAD + Range request orchestration. Single-connection HttpWebRequest below remains.
    # Stream via HttpWebRequest — faster than Invoke-WebRequest on PS 5.1 and supports progress.
    $request = [System.Net.HttpWebRequest]::Create($Url)
    $request.Timeout = 300000  # 5 min
    $request.AutomaticDecompression = [System.Net.DecompressionMethods]::GZip -bor [System.Net.DecompressionMethods]::Deflate
    $response = $request.GetResponse()
    $totalBytes = $response.ContentLength
    $stream = $response.GetResponseStream()
    $fileStream = [System.IO.File]::Create($OutFile)
    $buffer = New-Object byte[] 65536
    $totalRead = 0
    $lastPercent = -1
    $lastMb = -1

    try {
        while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
            $fileStream.Write($buffer, 0, $read)
            $totalRead += $read
            $mb = [math]::Round($totalRead / 1MB, 1)
            if ($totalBytes -gt 0) {
                $percent = [math]::Min(100, [math]::Floor(($totalRead / $totalBytes) * 100))
                if ($percent -ne $lastPercent) {
                    $totalMb = [math]::Round($totalBytes / 1MB, 1)
                    Write-Host "`r  Downloading... ${mb} MB / ${totalMb} MB (${percent}%)" -NoNewline
                    $lastPercent = $percent
                }
            } elseif ($mb -ne $lastMb) {
                Write-Host "`r  Downloading... ${mb} MB" -NoNewline
                $lastMb = $mb
            }
        }
        Write-Host ''
    } finally {
        $fileStream.Close()
        $stream.Close()
        $response.Close()
    }
}

function Read-GrokToken([string]$Scope) {
    $authFile = Join-Path $GrokDir 'auth.json'
    if (-not (Test-Path $authFile)) { return $null }
    try {
        $auth = Get-Content -Raw $authFile | ConvertFrom-Json
        $entry = $auth.$Scope
        if ($entry -and $entry.key) { return $entry.key }
    } catch {}
    return $null
}

# --- Validate version ---

if (-not $Version) {
    $Version = 'latest'
}
if ($Version -ne 'latest' -and $Version -notmatch '^byok-v\d+\.\d+\.\d+(-\S+)?$') {
    Write-Error "Invalid version format: $Version (expected 'latest' or a byok-vX.Y.Z tag)"
    exit 1
}

# --- Resolve auth ---

$OidcScope = 'https://auth.x.ai::b1a00492-073a-47ea-816f-4c329264a828'
$LegacyScope = 'https://accounts.x.ai/sign-in'
$AuthSource = ''

if ($env:GROK_DEPLOYMENT_KEY) {
    $AuthSource = 'deployment key'
    Write-Host 'Auth: using deployment key.' -ForegroundColor DarkGray
} else {
    $oidcToken = Read-GrokToken $OidcScope
    $legacyToken = Read-GrokToken $LegacyScope
    if ($oidcToken) {
        $AuthSource = 'auth.json (oidc)'
        Write-Host 'Auth: using OIDC token from ~/.grok/auth.json.' -ForegroundColor DarkGray
    } elseif ($legacyToken) {
        $AuthSource = 'auth.json (legacy)'
        Write-Host 'Auth: using legacy token from ~/.grok/auth.json.' -ForegroundColor DarkGray
    }
}

# --- Detect architecture ---

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    'AMD64'   { 'x86_64' }
    'x86'     { 'x86_64' }   # 32-bit PS on 64-bit Windows
    'ARM64'   { 'aarch64' }
    default   { $null }
}

if (-not $arch) {
    Write-Error "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE"
    exit 1
}

# CI only builds Windows x64 so far; ARM64 has no asset yet.
if ($arch -ne 'x86_64') {
    Write-Error "No prebuilt binary for Windows $arch yet (only windows-x86_64 is built by CI)."
    exit 1
}
$platform = 'windows-x86_64'
$asset = 'xai-grok-pager-windows-x64'

# --- Resolve download URL (our GitHub Releases) ---
# No channels in this fork: GROK_CHANNEL is ignored.

$Repo = 'ukjent7/grok-build'
$DownloadDir = Join-Path $GrokDir 'downloads'
$BinDir = if ($env:GROK_BIN_DIR) { $env:GROK_BIN_DIR } else { Join-Path $GrokDir 'bin' }

New-Item -ItemType Directory -Path $DownloadDir -Force | Out-Null
New-Item -ItemType Directory -Path $BinDir -Force | Out-Null

if ($Version -eq 'latest') {
    $resolvedVersion = 'latest'
    $downloadUrl = "https://github.com/$Repo/releases/latest/download/$asset"
} else {
    $resolvedVersion = $Version
    $downloadUrl = "https://github.com/$Repo/releases/download/$Version/$asset"
}

if ($AuthSource) {
    Write-Host "Installing Grok $resolvedVersion ($platform, $AuthSource)..." -ForegroundColor Cyan
} else {
    Write-Host "Installing Grok $resolvedVersion ($platform)..." -ForegroundColor Cyan
}

# --- Download binary ---

$binaryPath = Join-Path $DownloadDir "grok-$platform.exe"

try {
    Download-File $downloadUrl $binaryPath
} catch {
    if (Test-Path $binaryPath) { Remove-Item $binaryPath -Force }
    Write-Error "Binary download failed from $downloadUrl"
    exit 1
}

# --- Install binary (locked-file safe) ---

foreach ($binName in @('grok.exe', 'agent.exe')) {
    $dest = Join-Path $BinDir $binName
    $old = "$dest.old"

    if (Test-Path $old) { Remove-Item $old -Force -ErrorAction SilentlyContinue }

    try {
        Copy-Item -Path $binaryPath -Destination $dest -Force
    } catch {
        try {
            if (Test-Path $dest) { Rename-Item $dest $old -Force -ErrorAction SilentlyContinue }
            Copy-Item -Path $binaryPath -Destination $dest -Force
        } catch {
            if (Test-Path $old) { Rename-Item $old $dest -Force -ErrorAction SilentlyContinue }
            Write-Error "Failed to install $binName"
            exit 1
        }
    }
}

Write-Host "  Installed to $BinDir\grok.exe and $BinDir\agent.exe." -ForegroundColor DarkGray

# --- Generate completions (best-effort) ---

$completionsDir = Join-Path (Join-Path $GrokDir 'completions') 'powershell'
try {
    New-Item -ItemType Directory -Path $completionsDir -Force | Out-Null
    & (Join-Path $BinDir 'grok.exe') completions powershell 2>$null |
        Set-Content (Join-Path $completionsDir 'grok.ps1') -ErrorAction SilentlyContinue
} catch {}

# --- Persist installer config ---

$ConfigFile = Join-Path $GrokDir 'config.toml'
# FORK(byok): fork installs track byok-v* releases, never the official channel.
$cliLines = @('installer = "byok"')
# No channels in this fork: only the installer marker is persisted.

if (-not (Test-Path $ConfigFile)) {
    New-Item -ItemType Directory -Path (Split-Path $ConfigFile) -Force | Out-Null
    $content = "[cli]`r`n" + ($cliLines -join "`r`n") + "`r`n"
    [System.IO.File]::WriteAllText($ConfigFile, $content, [System.Text.Encoding]::UTF8)
} elseif ((Get-Content -Raw $ConfigFile) -match '(?m)^\[cli\]') {
    # Section-aware: only replace installer/channel under [cli], not other sections.
    $existingLines = Get-Content $ConfigFile
    $output = [System.Collections.ArrayList]::new()
    $inCli = $false

    foreach ($line in $existingLines) {
        if ($line -match '^\[cli\]\s*(#.*)?$') {
            [void]$output.Add($line)
            foreach ($cl in $cliLines) { [void]$output.Add($cl) }
            $inCli = $true
            continue
        }
        if ($line -match '^\[.+\]\s*(#.*)?$') {
            $inCli = $false
        }
        if ($inCli -and $line -match '^\s*(installer|channel)\s*=') {
            continue
        }
        [void]$output.Add($line)
    }
    [System.IO.File]::WriteAllLines($ConfigFile, [string[]]$output.ToArray(), [System.Text.Encoding]::UTF8)
} else {
    Add-Content -Path $ConfigFile -Value "`r`n[cli]`r`n$($cliLines -join "`r`n")`r`n"
}

# --- Fetch deployment config (deployment key only) ---

if ($env:GROK_DEPLOYMENT_KEY) {
    $ProxyUrl = if ($env:GROK_PROXY_URL) { $env:GROK_PROXY_URL } else { 'https://cli-chat-proxy.grok.com/v1' }
    # Refuse cleartext / userinfo / empty-host proxies before attaching the key.
    try {
        $proxyUri = [Uri]$ProxyUrl
    } catch {
        Write-Error "GROK_PROXY_URL must be an https:// URL."
        exit 1
    }
    if (-not $proxyUri.IsAbsoluteUri -or $proxyUri.Scheme -ne 'https' -or -not $proxyUri.Host -or $proxyUri.UserInfo) {
        Write-Error "GROK_PROXY_URL must be an https:// URL."
        exit 1
    }
    Write-Host '  Fetching deployment config...' -ForegroundColor DarkGray
    try {
        $headers = @{ 'Authorization' = "Bearer $($env:GROK_DEPLOYMENT_KEY)" }
        # IRM follows redirects and would resend the Bearer token.
        $deployResponse = Invoke-RestMethod -Uri "$ProxyUrl/deployment/config" -Headers $headers -UseBasicParsing -MaximumRedirection 0
    } catch {
        Write-Host "  Warning: failed to fetch deployment config from $ProxyUrl/deployment/config" -ForegroundColor Yellow
        $deployResponse = $null
    }

    if ($deployResponse) {
        $managedConfig = $deployResponse.managed_config
        $requirements = $deployResponse.requirements

        $managedConfigPath = Join-Path $GrokDir 'managed_config.toml'
        $requirementsPath = Join-Path $GrokDir 'requirements.toml'

        if ($managedConfig -and $managedConfig -ne 'null') {
            [System.IO.File]::WriteAllText($managedConfigPath, $managedConfig, [System.Text.Encoding]::UTF8)
            Write-Host '  Managed config applied.' -ForegroundColor DarkGray
        } else {
            if (Test-Path $managedConfigPath) { Remove-Item $managedConfigPath -Force }
        }

        if ($requirements -and $requirements -ne 'null') {
            [System.IO.File]::WriteAllText($requirementsPath, $requirements, [System.Text.Encoding]::UTF8)
            Write-Host '  Requirements applied.' -ForegroundColor DarkGray
        } else {
            if (Test-Path $requirementsPath) { Remove-Item $requirementsPath -Force }
        }
    }
}

Write-Host "Grok $resolvedVersion installed to $BinDir\grok.exe" -ForegroundColor Green

# --- Ensure grok is on PATH ---

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$pathEntries = if ($userPath) { $userPath -split ';' | Where-Object { $_ -ne '' } } else { @() }
if ($pathEntries -notcontains $BinDir) {
    $newPath = (@($BinDir) + $pathEntries) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Host "  Added $BinDir to your User PATH." -ForegroundColor DarkGray
    # Update current session so grok works immediately.
    if ($env:Path -notlike "*$BinDir*") {
        $env:Path = "$BinDir;$env:Path"
    }
}

Write-Host ''
Write-Host "Run 'grok' or 'agent' to get started!" -ForegroundColor Cyan
