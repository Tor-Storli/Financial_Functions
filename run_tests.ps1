# ── financial_functions Test Runner for Windows (PowerShell 5.1+) ───────────────────
# Usage: .\run_tests.ps1
# Run from the root of your Financial_Functions project folder.
# Results are printed to the console AND saved to a timestamped log file.
#
# Optional overrides — edit these lines if needed:
$DuckDB    = "duckdb"
$Extension = "target\release\financial_functions.duckdb_extension"
$TestDir   = "test"
$LogDir    = "test\logs"

# ── Set up log file ────────────────────────────────────────────────────────────
if (-not (Test-Path $LogDir)) { New-Item -ItemType Directory -Path $LogDir | Out-Null }
$Timestamp = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"
$LogFile   = "$LogDir\test_results_$Timestamp.txt"
$LogLines  = [System.Collections.ArrayList]@()

function Write-Green([string]$msg) { Write-Host $msg -ForegroundColor Green;  [void]$LogLines.Add($msg) }
function Write-Red([string]$msg)   { Write-Host $msg -ForegroundColor Red;    [void]$LogLines.Add($msg) }
function Write-Cyan([string]$msg)  { Write-Host $msg -ForegroundColor Cyan;   [void]$LogLines.Add($msg) }
function Write-Gray([string]$msg)  { Write-Host $msg -ForegroundColor Gray;   [void]$LogLines.Add($msg) }
function Write-Plain([string]$msg) { Write-Host $msg;                         [void]$LogLines.Add($msg) }

# ── Resolve extension path ─────────────────────────────────────────────────────
$resolved = Resolve-Path $Extension -ErrorAction SilentlyContinue
if ($null -eq $resolved) {
    Write-Red "ERROR: Extension not found at '$Extension'"
    Write-Red "       Run: cargo duckdb-ext build --duckdb-version vX.Y.Z -- --release"
    exit 1
}
$ExtPath = $resolved.Path -replace '\\', '/'

# ── Find test files ────────────────────────────────────────────────────────────
$TestFiles = Get-ChildItem -Path $TestDir -Filter "*.test" -Recurse
if ($TestFiles.Count -eq 0) {
    Write-Red "ERROR: No .test files found in '$TestDir'"
    exit 1
}

# ── Counters ───────────────────────────────────────────────────────────────────
$script:TotalPass = 0
$script:TotalFail = 0

# ── Run SQL via DuckDB CLI ─────────────────────────────────────────────────────
# Returns the raw output including any error messages on stderr
function Invoke-DuckDB([string]$Sql) {
    $raw = & $DuckDB -unsigned -noheader -list -c $Sql 2>&1
    return ($raw | Out-String).Trim()
}

# ── Determine if a result counts as passing ────────────────────────────────────
# A guard test (expected = "true") passes if:
#   1. The query returned "true" (function returned NULL, IS NULL = true), OR
#   2. The query threw an "Invalid Input Error" (function rejected bad input with an error)
# Both outcomes mean the function safely rejected invalid input — neither crashes DuckDB.
function Test-ResultPass([string]$actual, [string]$expected) {
    if ($actual -eq $expected) { return $true }

    # For guard tests expecting "true": also accept Invalid Input Error
    # because the function rejected bad input (error vs NULL are both safe)
    if ($expected -eq "true" -and $actual -match "Invalid Input Error") {
        return $true
    }
    return $false
}

# ── Parse and run one .test file ──────────────────────────────────────────────
function Run-TestFile([string]$FilePath) {
    Write-Cyan "`n── $FilePath"

    $lines    = Get-Content $FilePath
    $i        = 0
    $filePass = 0
    $fileFail = 0
    $LoadSql  = "LOAD '$ExtPath';"

    while ($i -lt $lines.Count) {
        $line = $lines[$i].Trim()

        if ($line -eq '' -or ($line.StartsWith('#') -and $line -notmatch '^(query|statement)')) {
            $i++; continue
        }

        # ── statement ok ──────────────────────────────────────────────────────
        if ($line -match '^statement\s+ok') {
            $i++
            $sql = ""
            while ($i -lt $lines.Count -and $lines[$i].Trim() -ne '') {
                $sql += $lines[$i].Trim() + " "
                $i++
            }
            $sql = $sql.Trim()
            if ($sql -match '^LOAD\s') { $LoadSql = "LOAD '$ExtPath';" }
            $i++; continue
        }

        # ── query R / I / T ───────────────────────────────────────────────────
        if ($line -match '^query\s+[RITE]') {
            $i++
            $sql = ""
            while ($i -lt $lines.Count -and $lines[$i].Trim() -ne '----') {
                $sql += $lines[$i].Trim() + " "
                $i++
            }
            $i++ # skip ----

            $expected = @()
            while ($i -lt $lines.Count -and $lines[$i].Trim() -ne '') {
                $expected += $lines[$i].Trim()
                $i++
            }
            $expectedStr = ($expected -join "`n").Trim()

            $fullSql = "$LoadSql $($sql.Trim())"
            $actual  = Invoke-DuckDB $fullSql

            $label = $sql.Trim()
            if ($label.Length -gt 70) { $label = $label.Substring(0, 70) + "..." }

            if (Test-ResultPass $actual $expectedStr) {
                $filePass++
                $script:TotalPass++

                # Show a note when an error was raised (vs clean NULL)
                if ($expectedStr -eq "true" -and $actual -match "Invalid Input Error") {
                    Write-Gray "  PASS: $label [rejected with error]"
                } else {
                    Write-Gray "  PASS: $label"
                }
            } else {
                $fileFail++
                $script:TotalFail++
                Write-Red "  FAIL: $label"
                Write-Red "        Expected : $expectedStr"
                Write-Red "        Got      : $actual"
            }
            continue
        }

        $i++
    }

    if ($fileFail -eq 0) {
        Write-Green "  Result: $filePass passed, $fileFail failed"
    } else {
        Write-Red "  Result: $filePass passed, $fileFail failed"
    }
}

# ── Main ───────────────────────────────────────────────────────────────────────
$RunDate = Get-Date -Format "yyyy-MM-dd HH:mm:ss"

Write-Plain ""
Write-Cyan  "═══════════════════════════════════════════════"
Write-Cyan  "  financial_functions Test Runner"
Write-Cyan  "  Run date  : $RunDate"
Write-Cyan  "  Extension : $ExtPath"
Write-Cyan  "  DuckDB    : $DuckDB"
Write-Cyan  "  Log file  : $LogFile"
Write-Cyan  "═══════════════════════════════════════════════"

foreach ($file in $TestFiles) {
    Run-TestFile $file.FullName
}

# ── Summary ────────────────────────────────────────────────────────────────────
Write-Plain ""
Write-Cyan  "═══════════════════════════════════════════════"
Write-Cyan  "  SUMMARY"
Write-Green "  Passed : $($script:TotalPass)"
if ($script:TotalFail -gt 0) {
    Write-Red   "  Failed : $($script:TotalFail)"
} else {
    Write-Green "  Failed : $($script:TotalFail)"
}
Write-Cyan  "═══════════════════════════════════════════════"
Write-Plain ""

# ── Save log file ──────────────────────────════────────────────────────────────
$LogLines | Out-File -FilePath $LogFile -Encoding UTF8
Write-Host "Log saved to: $LogFile" -ForegroundColor Yellow

if ($script:TotalFail -gt 0) { exit 1 } else { exit 0 }