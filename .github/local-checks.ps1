[CmdletBinding()]
param([string]$Task = 'check', [switch]$List)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'local-checks.json') -Raw | ConvertFrom-Json -AsHashtable
if ($List) { $manifest.tasks.Keys | Sort-Object; return }
if (-not $manifest.tasks.ContainsKey($Task)) { throw "Unknown task '$Task'. Available: $($manifest.tasks.Keys -join ', ')" }
$outputDirectory = Join-Path $PSScriptRoot 'local-output'
New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
$runId = "$(Get-Date -Format 'yyyyMMdd-HHmmss')-$PID"
$logPath = Join-Path $outputDirectory "$Task-$runId.log"
$reportPath = Join-Path $outputDirectory "$Task-$runId.json"
$results = [Collections.Generic.List[object]]::new()
$failed = $false
Start-Transcript -LiteralPath $logPath | Out-Null
try {
  foreach ($step in $manifest.tasks[$Task]) {
    $workingDirectory = [IO.Path]::GetFullPath((Join-Path $repoRoot $step.cwd))
    $arguments = @($step.command | Select-Object -Skip 1 | ForEach-Object { $_.Replace('{repo}', $repoRoot) })
    $program = [string]$step.command[0]
    if ($IsWindows -and $program -in @('npm', 'npx', 'corepack')) { $program += '.cmd' }
    if ($program -eq 'git-bash') {
      if ($IsWindows) {
        $gitDirectory = Split-Path (Get-Command git -ErrorAction Stop).Source -Parent
        $program = [IO.Path]::GetFullPath((Join-Path $gitDirectory '../bin/bash.exe'))
        if (-not (Test-Path -LiteralPath $program -PathType Leaf)) { throw "Git Bash is missing: $program" }
      } else { $program = 'bash' }
    } elseif ($program.Contains('/') -or $program.Contains('\')) { $program = Join-Path $repoRoot $program }
    $savedEnvironment = @{}
    if ($step.environment) {
      foreach ($key in $step.environment.Keys) {
        $savedEnvironment[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
        [Environment]::SetEnvironmentVariable($key, ([string]$step.environment[$key]).Replace('{repo}', $repoRoot), 'Process')
      }
    }
    $started = Get-Date
    $code = 0
    Write-Host "Running $($step.name) in $workingDirectory"
    Push-Location $workingDirectory
    try {
      $global:LASTEXITCODE = 0
      & $program @arguments
      $code = $LASTEXITCODE
      if ($null -eq $code) { $code = 0 }
    } catch { Write-Host $_ -ForegroundColor Red; $code = 1 }
    finally {
      Pop-Location
      foreach ($key in $savedEnvironment.Keys) { [Environment]::SetEnvironmentVariable($key, $savedEnvironment[$key], 'Process') }
    }
    if ($code -ne 0) { $failed = $true }
    $results.Add(@{name=$step.name; command=$step.command; cwd=$step.cwd; exitCode=$code; durationSeconds=((Get-Date)-$started).TotalSeconds})
    @{task=$Task; completed=$false; results=@($results.ToArray())} | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $reportPath -Encoding utf8
  }
} catch {
  $failed = $true
  Write-Host $_ -ForegroundColor Red
} finally {
  @{task=$Task; completed=$true; passed=(-not $failed); results=@($results.ToArray())} | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $reportPath -Encoding utf8
  Stop-Transcript | Out-Null
}
Write-Host "Local verification receipt: $reportPath"
if ($failed) { exit 1 }
