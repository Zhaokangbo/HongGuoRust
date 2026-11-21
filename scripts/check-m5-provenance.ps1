[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$path = Join-Path $root 'docs/architecture/ffmpeg-config-inventory.json'
$inventory = Get-Content -LiteralPath $path -Raw -Encoding utf8 | ConvertFrom-Json
if ($inventory.schema_version -ne 1) { throw 'Unexpected provenance inventory schema' }
if ($inventory.evidence_scope -ne 'upstream_ffmpeg_configure_output_not_proof_of_actual_linkage_or_license_compliance') {
    throw 'Provenance inventory scope has changed; manual review required'
}
if ($inventory.upstream.libmpv_dll_sha256 -ne 'df006bfc754d1212fb2084c6a6da04876882648c025ccb56cf1132b27b1e7913') {
    throw 'libmpv binary identity unexpectedly changed'
}
if ($inventory.upstream.ffmpeg_source -ne '7ddf7bc8a482cf28b4fab2e63b7093ba35cb6a80') {
    throw 'FFmpeg source identity unexpectedly changed'
}
if ($inventory.ffmpeg.license_reported -ne 'LGPL version 3 or later' -or $inventory.ffmpeg.build_kind -ne 'static') {
    throw 'FFmpeg configure evidence unexpectedly changed'
}
foreach ($group in @(@($inventory.ffmpeg.external_libraries), @($inventory.ffmpeg.hardware_acceleration_providers))) {
    $items = @($group)
    if (@($items | Where-Object { [string]::IsNullOrWhiteSpace($_) }).Count -ne 0) {
        throw 'Empty name in provenance inventory'
    }
    if (@($items | Sort-Object -Unique).Count -ne $items.Count) {
        throw 'Duplicate entry in provenance inventory'
    }
}
if (@($inventory.ffmpeg.external_libraries).Count -ne 37) {
    throw 'Unexpected FFmpeg configured library count; re-audit upstream logs'
}
if (@($inventory.ffmpeg.hardware_acceleration_providers).Count -ne 14) {
    throw 'Unexpected FFmpeg configured hardware provider count'
}
if ($inventory.audit.status -ne 'pending' -or
    $inventory.audit.dependency_license_review_complete -ne $false -or
    $inventory.audit.corresponding_source_bundle_complete -ne $false -or
    $inventory.audit.redistribution_authorized -ne $false) {
    throw 'Review gate must remain pending until supported by a documented audit'
}
$mpvScript = Get-Content -LiteralPath (Join-Path $root 'scripts/get-mpv.ps1') -Raw
if (-not $mpvScript.Contains($inventory.upstream.libmpv_dll_sha256)) {
    throw 'Pinned libmpv DLL hash no longer matches reviewed inventory'
}
Write-Host 'PASS: upstream config inventory is pinned (37 libraries, 14 hardware providers); redistribution review remains pending.'
