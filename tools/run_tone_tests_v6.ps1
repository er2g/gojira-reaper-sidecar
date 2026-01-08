$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$outDir = Join-Path $root "tone_runs_v6"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$vertexProject = $env:VERTEX_PROJECT
if ([string]::IsNullOrWhiteSpace($vertexProject)) { $vertexProject = "gen-lang-client-0142843869" }
$vertexLocation = $env:VERTEX_LOCATION
if ([string]::IsNullOrWhiteSpace($vertexLocation)) { $vertexLocation = "us-central1" }

$brainCli = Join-Path $root "gojira_brain_ui/target/debug/brain_cli.exe"
if (!(Test-Path $brainCli)) {
  throw "brain_cli.exe not found: $brainCli (build with: cd gojira_brain_ui; cargo build -p brain_cli)"
}

$tests = @(
  @{
    name = "01_gojira_fmts_raw_rhythm"
    prompt = @'
Target: Gojira — The Link / From Mars to Sirius era raw/organic rhythm tone. Tight but not overly modern.
Use Rust/Crunch amp; explain why, and how you shape low-mids vs fizz with graphic EQ band regions. Keep cab voicing mid-forward.
Guitar: bridge humbucker, medium output.
'@.Trim()
  },
  @{
    name = "02_gojira_stranded_modern_chug"
    prompt = @'
Target: Gojira — Stranded/Silvera modern tight chug rhythm. Very percussive palm-mutes, controlled low end, aggressive high-mids without harsh fizz.
Use Hot/Lead amp; include OD boost and gate choices; explain cab choice and mic level gain-staging (avoid clipping).
'@.Trim()
  },
  @{
    name = "03_pantera_cfh_rhythm"
    prompt = @'
Target: Pantera — Cowboys From Hell rhythm (early 90s). Scooped-ish but still present; punchy low end, biting upper mids, not overly saturated.
Use whichever amp fits best and justify. Guitar: bridge humbucker (passive).
'@.Trim()
  },
  @{
    name = "04_metallica_black_album_rhythm"
    prompt = @'
Target: Metallica — Black Album rhythm tone. Thick low-mids, smooth top, tight but not djent. Minimal effects.
Explain amp selection and EQ band regions.
'@.Trim()
  },
  @{
    name = "05_in_flames_clayman_melodic_dm"
    prompt = @'
Target: In Flames — Clayman melodic death rhythm. Tight, crunchy, articulate, slightly bright pick attack. Keep the low end controlled.
Mention how you avoid fizz while keeping bite.
'@.Trim()
  },
  @{
    name = "06_death_symbolic_lead"
    prompt = @'
Target: Death — Symbolic lead tone. Singing sustain, clear note separation, not too scooped.
Add subtle space if needed (delay/reverb with sensible mix). Explain choices. Guitar: neck humbucker for leads.
'@.Trim()
  },
  @{
    name = "07_slipknot_iowa_heavy"
    prompt = @'
Target: Slipknot — Iowa era heavy rhythm. Dense, aggressive, thick low end but not flubby, controlled noise.
Use Hot/Lead and describe how you manage noise gate vs sustain.
'@.Trim()
  },
  @{
    name = "08_bad_religion_punk"
    prompt = @'
Target: Bad Religion / 90s punk rhythm. Bright, crunchy, fast attack, not too much gain, minimal low end.
Prefer Crunch/Rust and explain EQ moves to keep it thin but not harsh.
'@.Trim()
  },
  @{
    name = "09_deftones_white_pony_clean_ambient"
    prompt = @'
Target: Deftones — White Pony clean/ambient texture. Wide and airy, clear lows, shimmering space.
Use Clean amp. Add reverb (shimmer if appropriate) and justify mix/time/filters. Guitar: single coils or split-coil.
'@.Trim()
  },
  @{
    name = "10_meshuggah_obzen_djent"
    prompt = @'
Target: Meshuggah — obZen tight djent rhythm. Extremely tight low end, aggressive mids, minimal fizz, very staccato palm-mutes.
Use Hot/Lead + OD + gate, and explain cab voicing.
'@.Trim()
  }
)

function Run-One($name, $prompt, $modelPrimary, $modelFallback) {
  $promptPath = Join-Path $outDir "$name.prompt.txt"
  Set-Content -Path $promptPath -Value $prompt -Encoding UTF8

  $args = @(
    "--backend", "vertex",
    "--gemini-model", $modelPrimary,
    "--vertex-project", $vertexProject,
    "--vertex-location", $vertexLocation,
    "--no-ws",
    "--prompt-file", $promptPath
  )

  $logPath = Join-Path $outDir "$name.$modelPrimary.log"
  Write-Host "==> $name ($modelPrimary)"
  $errPath = "$logPath.err"
  $p = Start-Process -FilePath $brainCli -ArgumentList $args -Wait -NoNewWindow -PassThru -RedirectStandardOutput $logPath -RedirectStandardError $errPath
  if (Test-Path $errPath) {
    Add-Content -Path $logPath -Value (Get-Content -Path $errPath -Raw)
    Remove-Item -Force $errPath
  }
  $code = $p.ExitCode
  if ($code -eq 0) { return }

  if ($modelFallback) {
    $args[3] = $modelFallback
    $logPath2 = Join-Path $outDir "$name.$modelFallback.log"
    Write-Host "==> retry $name ($modelFallback)"
    $errPath2 = "$logPath2.err"
    $p2 = Start-Process -FilePath $brainCli -ArgumentList $args -Wait -NoNewWindow -PassThru -RedirectStandardOutput $logPath2 -RedirectStandardError $errPath2
    if (Test-Path $errPath2) {
      Add-Content -Path $logPath2 -Value (Get-Content -Path $errPath2 -Raw)
      Remove-Item -Force $errPath2
    }
  }
}

foreach ($t in $tests) {
  Run-One $t.name $t.prompt "gemini-2.5-pro" "gemini-2.5-flash"
}

$report = Join-Path $outDir "report.md"
python (Join-Path $root "tools/tone_report.py") --dir $outDir --out $report
Write-Host "Wrote: $report"
