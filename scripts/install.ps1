param([string]$Version = 'v0.3.1', [string]$BinDir = "$env:LOCALAPPDATA\Lucy\bin")
$ErrorActionPreference = 'Stop'
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'This release provides Windows x86_64 only.' }
if ($Version -notmatch '^v\d+\.\d+\.\d+$') { throw 'Invalid release version' }
$Work = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $Work | Out-Null
try {
  $Asset = "lucy-$Version-x86_64-pc-windows-msvc.zip"
  $Base = "https://github.com/dhilipsiva/lucy/releases/download/$Version"
  Invoke-WebRequest "$Base/$Asset" -OutFile "$Work\$Asset"
  Invoke-WebRequest "$Base/SHA256SUMS" -OutFile "$Work\SHA256SUMS"
  $Lines = @(Get-Content "$Work\SHA256SUMS" | Where-Object { ($_ -split '\s+')[1] -eq $Asset })
  if ($Lines.Count -ne 1) { throw 'Missing or duplicate checksum' }
  $Expected = ($Lines[0] -split '\s+')[0]
  if ((Get-FileHash "$Work\$Asset" -Algorithm SHA256).Hash -ne $Expected) { throw 'Checksum mismatch' }
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $Zip = [IO.Compression.ZipFile]::OpenRead("$Work\$Asset")
  try {
    $Entry = $Zip.GetEntry('lucy.exe')
    if (!$Entry) { throw 'Missing binary' }
    [IO.Compression.ZipFileExtensions]::ExtractToFile($Entry, "$Work\lucy.exe")
  } finally { $Zip.Dispose() }
  & "$Work\lucy.exe" --version
  if ($LASTEXITCODE -ne 0) { throw 'Binary smoke check failed' }
  New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
  if (Test-Path "$BinDir\lucy.exe") { Copy-Item "$BinDir\lucy.exe" "$BinDir\lucy.previous.exe" -Force }
  Copy-Item "$Work\lucy.exe" "$BinDir\lucy.exe" -Force
  Write-Output "Installed $BinDir\lucy.exe. Add $BinDir to your user PATH. No service or model was installed."
} finally { Remove-Item -Recurse -Force $Work }
