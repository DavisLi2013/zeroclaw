# ZeroClaw workspace directory setup script
# Usage: .\set-workspace.ps1 "D:\path\to\your\directory"

param(
    [Parameter(Mandatory=$true)]
    [string]$TargetDirectory
)

# Check if directory exists
if (-not (Test-Path $TargetDirectory -PathType Container)) {
    Write-Host "Error: Directory '$TargetDirectory' does not exist." -ForegroundColor Red
    Write-Host "Please create the directory first or provide a valid path."
    exit 1
}

# Get absolute path
$absolutePath = Resolve-Path $TargetDirectory

# Read current config
$configPath = "$env:USERPROFILE\.zeroclaw\config.toml"
$configContent = Get-Content $configPath -Raw

# Convert path to TOML format (escape backslashes)
$tomlPath = $absolutePath -replace '\\', '\\\\'

# Update workspace_dir (add if not exists)
if ($configContent -match 'workspace_dir\s*=') {
    $configContent = $configContent -replace 'workspace_dir\s*=.*', "workspace_dir = `"$tomlPath`""
} else {
    $configContent = $configContent -replace 'workspace_only\s*=\s*true', "workspace_only = true`nworkspace_dir = `"$tomlPath`""
}

# Update allowed_roots
$allowedRootsLine = "allowed_roots = [`n    `"$tomlPath`",`n]"
$configContent = $configContent -replace 'allowed_roots\s*=\s*\[.*?\]', $allowedRootsLine

# Write updated config
Set-Content $configPath $configContent -Encoding UTF8

Write-Host "ZeroClaw workspace configured to: $absolutePath" -ForegroundColor Green
Write-Host "ZeroClaw will now only access files in this directory." -ForegroundColor Yellow
