# Signs one file with KIVO's Authenticode identity (DIST-11, REL-09). Tauri calls it for KIVO.exe,
# the sidecars (kivo-runtime.exe, which is also the browser's native-messaging host, and
# kivo-infer.exe) and the installers through `bundle.windows.signCommand`.
#
# Two routes (DISTRIBUTION §3); the owner chooses one and sets its secrets in the `release`
# environment (REL-11):
#   - Azure Artifact Signing: AZURE_SIGNING_ENDPOINT, AZURE_SIGNING_ACCOUNT, AZURE_SIGNING_PROFILE,
#     with the runner signed in to Azure (AZURE_CLIENT_ID / AZURE_TENANT_ID / AZURE_CLIENT_SECRET)
#     and AZURE_SIGNING_DLIB pointing at Azure.CodeSigning.Dlib.dll;
#   - an OV certificate: SIGNING_CERT_SHA1, the thumbprint of a certificate the runner can use
#     (a cloud HSM's key storage provider).
# Every signature is SHA-256 and timestamped.
param([Parameter(Mandatory = $true)][string]$Path)
$ErrorActionPreference = 'Stop'

$kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
$arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
$signtool = Get-ChildItem -Path $kits -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
  Where-Object { $_.Directory.Name -eq $arch } |
  Sort-Object FullName -Descending |
  Select-Object -First 1
if (-not $signtool) { throw 'signtool.exe was not found in the Windows SDK' }

if ($env:AZURE_SIGNING_ENDPOINT) {
  $metadata = Join-Path ([IO.Path]::GetTempPath()) 'kivo-signing-metadata.json'
  @{
    Endpoint               = $env:AZURE_SIGNING_ENDPOINT
    CodeSigningAccountName = $env:AZURE_SIGNING_ACCOUNT
    CertificateProfileName = $env:AZURE_SIGNING_PROFILE
  } | ConvertTo-Json | Set-Content -Path $metadata -Encoding utf8NoBOM
  & $signtool.FullName sign /v /fd SHA256 /tr 'http://timestamp.acs.microsoft.com' /td SHA256 `
    /dlib $env:AZURE_SIGNING_DLIB /dmdf $metadata $Path
} elseif ($env:SIGNING_CERT_SHA1) {
  & $signtool.FullName sign /v /fd SHA256 /tr 'http://timestamp.digicert.com' /td SHA256 `
    /sha1 $env:SIGNING_CERT_SHA1 $Path
} else {
  throw 'No signing identity is configured (AZURE_SIGNING_ENDPOINT or SIGNING_CERT_SHA1).'
}
if ($LASTEXITCODE -ne 0) { throw "signtool failed for $Path" }
