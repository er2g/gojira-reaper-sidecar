param(
  [int]$RunForMs = 15000
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot

function Receive-TextMessage {
  param([System.Net.WebSockets.ClientWebSocket]$Socket)
  $buffer = New-Object byte[] 16384
  $sb = New-Object System.Text.StringBuilder
  $ct = [Threading.CancellationToken]::None
  while ($true) {
    $segment = [ArraySegment[byte]]::new($buffer)
    $result = $Socket.ReceiveAsync($segment, $ct).Result
    if ($result.MessageType -eq [System.Net.WebSockets.WebSocketMessageType]::Close) {
      throw "Socket closed by server."
    }
    $null = $sb.Append([Text.Encoding]::UTF8.GetString($buffer, 0, $result.Count))
    if ($result.EndOfMessage) { break }
  }
  return $sb.ToString()
}

function Send-TextMessage {
  param(
    [System.Net.WebSockets.ClientWebSocket]$Socket,
    [string]$Text
  )
  $ct = [Threading.CancellationToken]::None
  $bytes = [Text.Encoding]::UTF8.GetBytes($Text)
  $segment = [ArraySegment[byte]]::new($bytes)
  $Socket.SendAsync($segment, [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $ct).Wait()
}

$guid = [Guid]::NewGuid().ToString("N")
$addrFile = Join-Path $env:TEMP "gojira_mock_addr_$guid.txt"
$logFile = Join-Path $env:TEMP "gojira_mock_server_$guid.log"
$errFile = Join-Path $env:TEMP "gojira_mock_server_$guid.err.log"

try {
  $p = Start-Process -PassThru -NoNewWindow `
    -WorkingDirectory $RepoRoot `
    -FilePath "cargo" `
    -ArgumentList @("run","-p","reaper_gojira_dll","--bin","mock_sidecar","--","--addr-file",$addrFile,"--run-for-ms",$RunForMs) `
    -RedirectStandardOutput $logFile `
    -RedirectStandardError $errFile

  for ($i = 0; $i -lt 100; $i++) {
    if (Test-Path $addrFile) {
      $addr = (Get-Content $addrFile -TotalCount 1).Trim()
      if ($addr) { break }
    }
    Start-Sleep -Milliseconds 100
  }

  if (-not $addr) {
    throw "Failed to read addr file: $addrFile`nStdout: $logFile`nStderr: $errFile"
  }

  $uri = [Uri]("ws://$addr")
  Write-Host "Connecting to $uri ..."

  $ws = [System.Net.WebSockets.ClientWebSocket]::new()
  $ct = [Threading.CancellationToken]::None
  $ws.ConnectAsync($uri, $ct).Wait()

  $handshake = $null
  for ($i = 0; $i -lt 50; $i++) {
    $text = Receive-TextMessage -Socket $ws
    $json = $text | ConvertFrom-Json
    if ($json.type -eq "handshake") {
      $handshake = $json
      break
    }
  }
  if ($null -eq $handshake) {
    throw "No handshake received."
  }

  $token = $handshake.session_token
  if (-not $token) { throw "Handshake missing session_token" }
  if (-not $handshake.instances -or $handshake.instances.Count -lt 1) { throw "Handshake had 0 instances" }
  $fxGuid = $handshake.instances[0].fx_guid

  $setTone = @{
    type = "set_tone"
    session_token = $token
    command_id = "smoke-1"
    target_fx_guid = $fxGuid
    mode = "merge"
    params = @(@{ index = 30; value = 0.42 })
  } | ConvertTo-Json -Compress
  Send-TextMessage -Socket $ws -Text $setTone

  $ack = $null
  for ($i = 0; $i -lt 50; $i++) {
    $text = Receive-TextMessage -Socket $ws
    $json = $text | ConvertFrom-Json
    if ($json.type -eq "ack") { $ack = $json; break }
    if ($json.type -eq "error") { throw "Server returned error: $($json.code) $($json.msg)" }
  }
  if ($null -eq $ack) { throw "No ack received." }
  Write-Host ("ACK ok: command_id={0}, applied_count={1}" -f $ack.command_id, @($ack.applied_params).Count)

  $unauth = @{ type = "refresh_instances"; session_token = "WRONG" } | ConvertTo-Json -Compress
  Send-TextMessage -Socket $ws -Text $unauth
  $err = $null
  for ($i = 0; $i -lt 20; $i++) {
    $text = Receive-TextMessage -Socket $ws
    $json = $text | ConvertFrom-Json
    if ($json.type -eq "error") { $err = $json; break }
  }
  if ($null -eq $err) { throw "No unauthorized error received." }
  if ($err.code -ne "unauthorized") { throw "Expected unauthorized, got: $($err.code)" }
  Write-Host "Unauthorized check ok."

  try { $ws.CloseAsync([System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure, "done", $ct).Wait() } catch {}
} finally {
  if ($p -and -not $p.HasExited) {
    try { Stop-Process -Id $p.Id -Force } catch {}
  }
  if (Test-Path $addrFile) { Remove-Item -Force $addrFile }
}
