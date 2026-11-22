$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$forbiddenTracked = @("wrangler.toml", "crates/flood-worker/wrangler.toml", ".dev.vars", ".env")
$tracked = git ls-files 2>$null
if ($LASTEXITCODE -eq 0 -and $tracked) {
    foreach ($f in $forbiddenTracked) {
        if ($tracked -contains $f) {
            Write-Error "SECURITY FAIL: Forbidden file tracked in git: $f"
            exit 1
        }
    }
    $filesToScan = $tracked | Where-Object {
        $_ -notmatch '\.(png|woff2|ico|svg)$' -and
        $_ -notmatch '^crates/flood-web/vendor/'
    }
} else {
    $filesToScan = Get-ChildItem -Recurse -File |
        Where-Object {
            $_.FullName -notmatch '\\(target|dist|build|\.git|\.wrangler)\\' -and
            $_.FullName -notmatch '\\crates\\flood-web\\vendor\\' -and
            $_.Name -notin @("wrangler.toml", ".dev.vars", ".env") -and
            $_.Extension -notin @(".png", ".woff2", ".ico", ".wasm")
        } | ForEach-Object { $_.FullName }
}

$patterns = @(
    @{ Name = "Cloudflare Account ID"; Regex = "account_id\s*=\s*['""](?!0{32})[0-9a-f]{32}['""]" },
    @{ Name = "Non-zero UUID"; Regex = "\b(?!00000000-0000-0000-0000-000000000000)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b" },
    @{ Name = "Google API Key"; Regex = "AIza[0-9A-Za-z_\-]{20,}" },
    @{ Name = "GitHub Token"; Regex = "(ghp_|gho_|github_pat_)[0-9A-Za-z_]{10,}" },
    @{ Name = "AWS Access Key"; Regex = "AKIA[0-9A-Z]{16}" },
    @{ Name = "Hardcoded Secret"; Regex = "(?:API_KEY|SECRET|TOKEN)\s*[:=]\s*['""][A-Za-z0-9_\-]{16,}['""]" },
    @{ Name = "Email Address"; Regex = "\b(?!\d+[\w.\-]*@users\.noreply\.github\.com)[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}\b" },
    @{ Name = "Local Windows User Path"; Regex = "[A-Za-z]:\\Users\\[A-Za-z0-9._\-]+" },
    @{ Name = "Local Monorepo Path"; Regex = "D:\\transbulent\.id" }
)

$hits = 0
foreach ($file in $filesToScan) {
    if (-not (Test-Path $file)) { continue }
    $content = Get-Content -Path $file -Raw -ErrorAction SilentlyContinue
    if (-not $content) { continue }
    foreach ($p in $patterns) {
        if ($content -match $p.Regex) {
            Write-Host "SECURITY HIT [$($p.Name)] in $file" -ForegroundColor Red
            $hits++
        }
    }
}

if ($hits -gt 0) {
    Write-Error "Security check failed with $hits issue(s)."
    exit 1
}

Write-Host "SECURITY CHECK PASSED: 0 exposed secrets, account IDs, UUIDs, emails, or local paths." -ForegroundColor Green
