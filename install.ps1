#
# Grok CLI installer for PowerShell — BYOK fork build.
# Downloads release binaries from ukjent7/grok-build GitHub Releases.
#
# Auth: GROK_DEPLOYMENT_KEY env var (takes precedence) or ~/.grok/auth.json from `grok login`.
# Env: GROK_VERSION (a byok-vX.Y.Z tag, default: latest), GROK_BIN_DIR, GROK_PROXY_URL,
#      GROK_GH_PROXY (mirror prefix, default: https://axisnow.gh-proxy.org), GROK_GH_MIRROR=off (disable mirror)
#
# Downloads go through the axisnow.gh-proxy.org mirror first (fast in CN) with a direct
# GitHub fallback; the SHA256 checksum comes from the repo tree over jsDelivr
# (checksums/<tag>/, committed by the release workflow), falling back to the
# release-asset copies. Verified with Get-FileHash before installing.
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
        $content = (Invoke-WebRequest -Uri $Url -UseBasicParsing).Content
    } catch {
        return $null
    }
    if ($null -eq $content) { return $null }
    if ($content -is [string]) { return $content }
    # Extensionless files (e.g. checksums/latest) are served as
    # application/octet-stream, and then .Content is raw bytes instead of
    # text — calling .Trim() on that fails with
    # "[System.Byte] does not contain a method named 'Trim'". Decode
    # explicitly so callers always get a string.
    try {
        $text = [System.Text.Encoding]::UTF8.GetString([byte[]]$content)
    } catch {
        $text = "$content"
    }
    return $text.TrimStart([char]0xFEFF)
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

# FORK(byok): try each URL in order, return the one that worked. Throws the
# last error when all fail. Partial files are removed between attempts.
function Try-Download-File([string[]]$Urls, [string]$OutFile) {
    $lastErr = $null
    foreach ($u in $Urls) {
        try {
            if (Test-Path $OutFile) { Remove-Item $OutFile -Force -ErrorAction SilentlyContinue }
            Download-File $u $OutFile
            return $u
        } catch {
            $lastErr = $_
            Write-Host "  Download failed from $u, trying next..." -ForegroundColor Yellow
        }
    }
    throw $lastErr
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

# FORK(byok): add a `key = value` line only when the key is absent — never
# overwrites an explicit user setting (e.g. someone whose gateway really does
# execute server-side search and re-enabled it).
# Section = '' targets top-level keys: inserted before the first [section]
# header so the key is not swallowed by another table. Section keys are added
# right after their section header, or as a new trailing section.
function Add-TomlValueIfMissing([string[]]$Lines, [string]$Section, [string]$Line) {
    if ($null -eq $Lines) { $Lines = @() }
    $key = ($Line -split '=', 2)[0].Trim()
    $keyPattern = "^[\s]*$([regex]::Escape($key))[\s]*="
    if ($Section -eq '') {
        if ($Lines -match $keyPattern) { return $Lines }
        $out = [System.Collections.ArrayList]::new()
        $inserted = $false
        foreach ($l in $Lines) {
            if (-not $inserted -and $l -match '^\s*\[.+\]') {
                [void]$out.Add($Line)
                $inserted = $true
            }
            [void]$out.Add($l)
        }
        if (-not $inserted) { [void]$out.Add($Line) }
        return [string[]]$out.ToArray()
    }
    $sectionPattern = "^[\s]*\[$([regex]::Escape($Section))\][\s]*(#.*)?$"
    $sectionSeen = $false
    $keySeen = $false
    $inSection = $false
    foreach ($l in $Lines) {
        if ($l -match '^\s*\[.+\]') {
            $inSection = $l -match $sectionPattern
            if ($inSection) { $sectionSeen = $true }
        } elseif ($inSection -and $l -match $keyPattern) {
            $keySeen = $true
        }
    }
    if ($sectionSeen -and $keySeen) { return $Lines }
    $out = [System.Collections.ArrayList]::new()
    if (-not $sectionSeen) {
        foreach ($l in $Lines) { [void]$out.Add($l) }
        [void]$out.Add('')
        [void]$out.Add("[$Section]")
        [void]$out.Add($Line)
        return [string[]]$out.ToArray()
    }
    $inserted = $false
    foreach ($l in $Lines) {
        [void]$out.Add($l)
        if (-not $inserted -and $l -match $sectionPattern) {
            [void]$out.Add($Line)
            $inserted = $true
        }
    }
    return [string[]]$out.ToArray()
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

# --- Resolve download URLs (our GitHub Releases) ---
# No channels in this fork: GROK_CHANNEL is ignored.
# FORK(byok): axisnow.gh-proxy.org mirror first (fast in CN), direct GitHub fallback.
# Set GROK_GH_MIRROR=off to skip the mirror, or GROK_GH_PROXY to use another
# prefix-style mirror (format: <prefix>/<full-original-url>).

$Repo = 'ukjent7/grok-build'
# FORK(byok): checksums live in the repo tree (checksums/<tag>/) so they can
# be fetched over jsDelivr where direct GitHub is unreachable. The release
# workflow commits them right after publishing the binaries.
$JsDelivr = 'https://cdn.jsdelivr.net/gh/ukjent7/grok-build@main'
$DownloadDir = Join-Path $GrokDir 'downloads'
$BinDir = if ($env:GROK_BIN_DIR) { $env:GROK_BIN_DIR } else { Join-Path $GrokDir 'bin' }

$GhMirror = ''
if ($env:GROK_GH_MIRROR -ne 'off') {
    if ($env:GROK_GH_PROXY) { $GhMirror = $env:GROK_GH_PROXY.TrimEnd('/') }
    else { $GhMirror = 'https://axisnow.gh-proxy.org' }
}

New-Item -ItemType Directory -Path $DownloadDir -Force | Out-Null
New-Item -ItemType Directory -Path $BinDir -Force | Out-Null

if ($Version -eq 'latest') {
    # Single-pointer rule: the tag read from the repo tree is the only truth.
    # The tree commit lands after the release publish (plus jsDelivr cache),
    # so releases/latest may already serve N while the tree still says N-1;
    # pinning both to the resolved tag installs N-1 correctly instead of
    # aborting on a hash mismatch. Unresolvable tag falls back to live latest.
    $latestTag = Download-String "$JsDelivr/checksums/latest"
    if ($latestTag) { $latestTag = $latestTag.Trim() }
    if ($latestTag -match '^byok-v\d+\.\d+\.\d+(-\S+)?$') {
        $resolvedVersion = $latestTag
        $directUrl = "https://github.com/$Repo/releases/download/$latestTag/$asset"
    } else {
        $resolvedVersion = 'latest'
        $directUrl = "https://github.com/$Repo/releases/latest/download/$asset"
    }
} else {
    $resolvedVersion = $Version
    $directUrl = "https://github.com/$Repo/releases/download/$Version/$asset"
}
$directChecksumUrl = "$directUrl.sha256"
# Checksum trust order: repo tree over jsDelivr first (owner-controlled, no
# GitHub access needed), then the release-asset copies. The tree copy is the
# anchor: a tampered binary mirror cannot forge it.
$treeChecksumUrls = @()
if ($resolvedVersion -ne 'latest') {
    $treeChecksumUrls = @("$JsDelivr/checksums/$resolvedVersion/$asset.sha256")
}
if ($GhMirror) {
    # gh-proxy format: <mirror>/<full-original-url>
    $mirrorUrl = "$GhMirror/$directUrl"
    $mirrorChecksumUrl = "$GhMirror/$directChecksumUrl"
    # Binary: mirror first for speed.
    $binaryUrls = @($mirrorUrl, $directUrl)
    $checksumUrls = @($treeChecksumUrls + @($mirrorChecksumUrl, $directChecksumUrl))
} else {
    $binaryUrls = @($directUrl)
    $checksumUrls = @($treeChecksumUrls + @($directChecksumUrl))
}

if ($AuthSource) {
    Write-Host "Installing Grok $resolvedVersion ($platform, $AuthSource)..." -ForegroundColor Cyan
} else {
    Write-Host "Installing Grok $resolvedVersion ($platform)..." -ForegroundColor Cyan
}

# --- Download binary ---

$binaryPath = Join-Path $DownloadDir "grok-$platform.exe"

try {
    $usedUrl = Try-Download-File $binaryUrls $binaryPath
    Write-Host "  Downloaded from $usedUrl" -ForegroundColor DarkGray
} catch {
    if (Test-Path $binaryPath) { Remove-Item $binaryPath -Force }
    Write-Error "Binary download failed (tried: $($binaryUrls -join ', '))"
    exit 1
}

# --- Verify SHA256 (repo tree via jsDelivr, then the release-asset copies) ---

$checksumFile = "$binaryPath.sha256"
$expectedHash = $null
try {
    $usedChecksumUrl = Try-Download-File $checksumUrls $checksumFile
    $firstLine = (Get-Content $checksumFile | Select-Object -First 1)
    if ($firstLine) { $expectedHash = ($firstLine -split '\s+')[0].Trim() }
    if ($expectedHash -notmatch '^[0-9a-fA-F]{64}$') { $expectedHash = $null }
    if ($expectedHash) {
        Write-Host "  Checksum source: $usedChecksumUrl" -ForegroundColor DarkGray
    }
} catch {
    $expectedHash = $null
} finally {
    if (Test-Path $checksumFile) { Remove-Item $checksumFile -Force -ErrorAction SilentlyContinue }
}
if ($expectedHash) {
    $actualHash = (Get-FileHash -Path $binaryPath -Algorithm SHA256).Hash
    if ($actualHash -ne $expectedHash) {
        Remove-Item $binaryPath -Force -ErrorAction SilentlyContinue
        Write-Error "SHA256 mismatch for $asset (expected $expectedHash, got $actualHash). The download may be corrupt or tampered; aborted."
        exit 1
    }
    Write-Host '  SHA256 verified.' -ForegroundColor DarkGray
} else {
    # Releases before checksums (or a tree commit that has not landed yet).
    Write-Host '  Warning: no SHA256 checksum found; skipping verification.' -ForegroundColor Yellow
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

# FORK(byok): secure-by-default for third-party endpoints. Most BYOK gateways
# cannot execute server-side search (the model answers from weights and may
# still claim it searched). Point `web_search` at a non-existent model so the
# tool never registers even when logged in (login is still useful for product
# skills/bundle sync) — do NOT set a global `disable_web_search=true`, it would also
# kill `web_fetch` (the kill-switch gates both). Instead explicitly enable
# `web_fetch`, which fetches pages locally and works fine on BYOK but
# defaults off without remote settings. Media generation hits xAI-only
# endpoints with your third-party key; telemetry/trace upload/feedback should
# be an explicit opt-in on a fork. Missing keys only — explicit user values win.
$cfgLines = Get-Content $ConfigFile
$cfgLines = Add-TomlValueIfMissing $cfgLines 'features' 'web_fetch = true'
$cfgLines = Add-TomlValueIfMissing $cfgLines 'models' 'web_search = "__disabled__"'
$cfgLines = Add-TomlValueIfMissing $cfgLines 'features' 'telemetry = false'
$cfgLines = Add-TomlValueIfMissing $cfgLines 'features' 'image_gen = false'
$cfgLines = Add-TomlValueIfMissing $cfgLines 'features' 'video_gen = false'
$cfgLines = Add-TomlValueIfMissing $cfgLines 'features' 'feedback = false'
$cfgLines = Add-TomlValueIfMissing $cfgLines 'telemetry' 'trace_upload = false'
[System.IO.File]::WriteAllLines($ConfigFile, [string[]]$cfgLines, [System.Text.Encoding]::UTF8)

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

# FORK(byok): image_edit has no [features] key (only the GROK_IMAGE_EDIT env),
# but it hits the same xAI-only endpoint as image_gen. Default it off the
# same way as the TOML defaults above: only when the user has set nothing
# (User-level or current process), never overwriting an explicit value.
# Re-enable later with: [Environment]::SetEnvironmentVariable('GROK_IMAGE_EDIT', '1', 'User')
$userImageEdit = [Environment]::GetEnvironmentVariable('GROK_IMAGE_EDIT', 'User')
if ([string]::IsNullOrEmpty($userImageEdit) -and [string]::IsNullOrEmpty($env:GROK_IMAGE_EDIT)) {
    [Environment]::SetEnvironmentVariable('GROK_IMAGE_EDIT', '0', 'User')
    $env:GROK_IMAGE_EDIT = '0'
    Write-Host '  Disabled image editing by default (GROK_IMAGE_EDIT=0, xAI-only endpoint).' -ForegroundColor DarkGray
}

Write-Host ''
Write-Host "Run 'grok' or 'agent' to get started!" -ForegroundColor Cyan
