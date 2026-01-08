$ErrorActionPreference = "Stop"

$uri = [Uri]"ws://127.0.0.1:9001"
$ws = [System.Net.WebSockets.ClientWebSocket]::new()
$ct = [Threading.CancellationToken]::None

Write-Host "Connecting to $uri ..."
$ws.ConnectAsync($uri, $ct).Wait()

function Receive-TextMessage {
  param([System.Net.WebSockets.ClientWebSocket]$Socket)
  $buffer = New-Object byte[] 16384
  $sb = New-Object System.Text.StringBuilder
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
  $bytes = [Text.Encoding]::UTF8.GetBytes($Text)
  $segment = [ArraySegment[byte]]::new($bytes)
  $Socket.SendAsync($segment, [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $ct).Wait()
}

function Print-Report {
  param($Report)
  if ($null -eq $Report) {
    Write-Host "(none)"
    return
  }
  foreach ($k in @(
    "delay_active_101",
    "delay_mix_105",
    "delay_active_best_guess",
    "delay_mix",
    "reverb_active_112",
    "reverb_mix_114",
    "reverb_active_best_guess",
    "reverb_mix",
    "delay_window_98_110",
    "reverb_window_110_122"
  )) {
    if ($Report.PSObject.Properties.Name -contains $k) {
      Write-Host ("- {0}: {1}" -f $k, $Report.$k)
    }
  }
}

function Print-ParamMeta {
  param($Handshake)

  if ($null -eq $Handshake) { return }

  if ($Handshake.PSObject.Properties.Name -contains "param_enums") {
    $enums = $Handshake.param_enums
    if ($null -ne $enums) {
      foreach ($k in @("84","92","99","113","5")) {
        if ($enums.PSObject.Properties.Name -contains $k) {
          $count = @($enums.$k).Count
          Write-Host ("- enums[{0}] => {1} option(s)" -f $k, $count)
          $preview = @($enums.$k | Select-Object -First 5 | ForEach-Object { "$($_.label) ($([math]::Round($_.value,3)))" })
          if ($preview.Count -gt 0) {
            Write-Host ("  first: " + ($preview -join " | "))
          }
        }
      }
    }
  }

  if ($Handshake.PSObject.Properties.Name -contains "param_formats") {
    $fmts = $Handshake.param_formats
    if ($null -ne $fmts) {
      foreach ($k in @("87","88","94","95")) {
        if ($fmts.PSObject.Properties.Name -contains $k) {
          $t = $fmts.$k
          Write-Host ("- fmt[{0}] min='{1}' mid='{2}' max='{3}'" -f $k, $t.min, $t.mid, $t.max)
        }
      }
    }
  }
}

for ($attempt = 0; $attempt -lt 4; $attempt++) {
  $handshake = $null
  for ($i = 0; $i -lt 20; $i++) {
    $text = Receive-TextMessage -Socket $ws
    Write-Host "Raw message:"
    Write-Host $text

    try {
      $json = $text | ConvertFrom-Json
    } catch {
      Write-Warning "Failed to parse JSON: $($_.Exception.Message)"
      continue
    }

    if ($null -ne $json.type -and $json.type -eq "handshake") {
      $handshake = $json
      break
    }
  }

  if ($null -eq $handshake) {
    Write-Warning "No handshake received."
    break
  }

  Write-Host ""
  Write-Host "Handshake validation_report:"
  Print-Report -Report $handshake.validation_report

  Write-Host ""
  Write-Host "Handshake param meta:"
  Print-ParamMeta -Handshake $handshake

  if ($handshake.instances -and $handshake.instances.Count -gt 0) {
    break
  }

  Write-Host "Handshake had 0 instances; sending refresh_instances and waiting..."
  $refresh = @{ type = "refresh_instances"; session_token = $handshake.session_token } | ConvertTo-Json -Compress
  Send-TextMessage -Socket $ws -Text $refresh
  Start-Sleep -Milliseconds 300
}

try { $ws.CloseAsync([System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure, "done", $ct).Wait() } catch {}
