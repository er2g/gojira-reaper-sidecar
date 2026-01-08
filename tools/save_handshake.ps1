param(
  [string]$Uri = "ws://127.0.0.1:9001",
  [string]$OutFile = "$env:USERPROFILE\Desktop\gojira_handshake.json",
  [int]$TimeoutSeconds = 8
)

$ErrorActionPreference = "Stop"

$ws = [System.Net.WebSockets.ClientWebSocket]::new()
$ct = [Threading.CancellationToken]::None

function Receive-TextMessage {
  param([System.Net.WebSockets.ClientWebSocket]$Socket, [int]$TimeoutSeconds)
  $buffer = New-Object byte[] 65536
  $sb = New-Object System.Text.StringBuilder

  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  while ($true) {
    if ([DateTime]::UtcNow -gt $deadline) {
      throw "Timed out waiting for a WebSocket message."
    }
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

Write-Host "Connecting to $Uri ..."
$ws.ConnectAsync([Uri]$Uri, $ct).Wait()

$handshakeText = $null
for ($i = 0; $i -lt 50; $i++) {
  $text = Receive-TextMessage -Socket $ws -TimeoutSeconds $TimeoutSeconds
  try {
    $json = $text | ConvertFrom-Json
  } catch {
    continue
  }
  if ($null -ne $json.type -and $json.type -eq "handshake") {
    $handshakeText = $text
    break
  }
}

if ($null -eq $handshakeText) {
  throw "No handshake received (is REAPER running with the extension loaded?)."
}

[System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($OutFile)) | Out-Null
[System.IO.File]::WriteAllText($OutFile, $handshakeText, (New-Object System.Text.UTF8Encoding($false)))
Write-Host "Saved handshake to: $OutFile"

try { $ws.CloseAsync([System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure, "done", $ct).Wait() } catch {}
