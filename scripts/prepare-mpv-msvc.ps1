[CmdletBinding()]
param([Parameter(Mandatory)][string]$Destination)
$ErrorActionPreference = 'Stop'
$Destination = (New-Item -ItemType Directory -Force -Path $Destination).FullName
& "$PSScriptRoot/get-mpv.ps1" -Destination $Destination
$dll = Join-Path $Destination 'libmpv-2.dll'
$vswhere = "${env:ProgramFiles(x86)}/Microsoft Visual Studio/Installer/vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
$tools = (Get-ChildItem "$vs/VC/Tools/MSVC/*/bin/Hostx64/x64/dumpbin.exe" | Sort-Object FullName | Select-Object -Last 1).DirectoryName
if (!$tools) { throw 'MSVC x64 tools are required' }
$symbols = & "$tools/dumpbin.exe" /exports $dll | ForEach-Object {
    if ($_ -match '^\s*\d+\s+[0-9A-F]+\s+[0-9A-F]+\s+(mpv_\w+)') { $Matches[1] }
}
if ($symbols -notcontains 'mpv_create') { throw 'Missing mpv exports' }
$def = Join-Path $Destination 'mpv.def'
@('LIBRARY libmpv-2.dll', 'EXPORTS') + $symbols | Set-Content -Encoding ascii $def
& "$tools/lib.exe" /nologo /machine:x64 "/def:$def" "/out:$Destination/mpv.lib"
if ($LASTEXITCODE) { throw 'Cannot create the MSVC import library' }

# This build imports vulkan-1.dll even when mpv uses Direct3D.
if (!(Test-Path "$env:windir/System32/vulkan-1.dll") -and !(Test-Path "$Destination/vulkan-1.dll")) {
    $work = Join-Path $Destination 'vulkan-build'
    foreach ($entry in @(
        @{name='Vulkan-Headers'; rev='2cd90f9d20df57eac214c148f3aed885372ddcfe'},
        @{name='Vulkan-Loader'; rev='da8d2caad9341ca8c5a7c3deba217d7da50a7c24'}
    )) {
        $src = Join-Path $work $entry.name
        if (!(Test-Path "$src/.git")) {
            git clone --depth 1 --branch v1.4.321 "https://github.com/KhronosGroup/$($entry.name).git" $src
            if ($LASTEXITCODE) { throw 'Vulkan source checkout failed' }
        }
        if ((git -C $src rev-parse HEAD) -ne $entry.rev) { throw 'Unexpected Vulkan source revision' }
    }
    $prefix = Join-Path $work 'prefix'
    cmake -S "$work/Vulkan-Headers" -B "$work/headers-build" "-DCMAKE_INSTALL_PREFIX=$prefix"
    if ($LASTEXITCODE) { throw 'Vulkan-Headers configure failed' }
    cmake --install "$work/headers-build" --config Release
    if ($LASTEXITCODE) { throw 'Vulkan-Headers install failed' }
    cmake -S "$work/Vulkan-Loader" -B "$work/loader-build" -A x64 "-DCMAKE_PREFIX_PATH=$prefix" -DBUILD_TESTS=OFF
    if ($LASTEXITCODE) { throw 'Vulkan-Loader configure failed' }
    cmake --build "$work/loader-build" --config Release --parallel
    if ($LASTEXITCODE) { throw 'Vulkan-Loader build failed' }
    $loader = @(Get-ChildItem "$work/loader-build" -Filter vulkan-1.dll -Recurse -File)
    if ($loader.Count -ne 1) { throw 'Expected one Vulkan loader' }
    Copy-Item -LiteralPath $loader[0].FullName -Destination $Destination
}
